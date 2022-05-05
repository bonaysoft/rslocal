use std::convert::Infallible;
use std::fmt::Error;

use http::{HeaderValue, Request, Response, StatusCode};
use http::header::HeaderName;
use http::response::Builder;
use hyper::{Body};
use hyper::service::{make_service_fn, service_fn};
use log::{debug, info};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use url::Url;
use crate::random_string;
use crate::server::{Connection, get_site_host, Payload, remove_site_host, setup_site_host, XData};
use crate::server::config::HTTPConfig;

static NOTFOUND: &[u8] = b"Not Found";

#[derive(Clone)]
pub struct HttpServer {
    cfg: HTTPConfig,
}

impl HttpServer {
    pub fn new(cfg: HTTPConfig) -> Self {
        HttpServer { cfg }
    }

    pub fn event_handler(&self, pl: Payload) {
        let u = Url::parse(pl.bind_addr.as_str()).unwrap();
        let mut host = u.host_str().unwrap().to_string();
        if let Some(port) = u.port() {
            host = format!("{}:{}", host, port);
        }

        if pl.tx.is_closed() {
            remove_site_host(host);
            return;
        }

        setup_site_host(host, pl.tx);
    }

    pub fn start(&self) {
        println!("start http-server");
        let cfg = self.cfg.clone();
        tokio::spawn(async move {
            let addr = cfg.bind_addr.parse().unwrap();
            let make_service = make_service_fn(move |_| {
                async move { Ok::<_, Infallible>(service_fn(move |req| proxy(req))) }
            });

            let server = hyper::Server::bind(&addr)
                .http1_preserve_header_case(true)
                .http1_title_case_headers(true)
                .serve(make_service);

            println!("Listening on http://{}", addr);
            if let Err(e) = server.await {
                eprintln!("server error: {}", e);
            }
        });
    }
}

fn not_found() -> Response<Body> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(NOTFOUND.into())
        .unwrap()
}

async fn build_raw_request(req: Request<Body>) -> anyhow::Result<Vec<u8>> {
    let mut buf = format!("{} {} {:?}\r\n", req.method(), req.uri(), req.version());
    for (name, val) in req.headers() {
        buf.push_str(&format!("{}: {}\r\n", name.as_str(), val.to_str().unwrap()));
    }
    buf.push_str("\r\n");
    let body = hyper::body::to_bytes(req.into_body()).await?;
    let mut data = buf.clone().into_bytes();
    data.write_all(body.as_ref()).await.unwrap(); // todo 改成流式发送
    Ok(data)
}

pub async fn proxy(req: Request<Body>) -> http::Result<Response<Body>> {
    // 获取发送Request的Sender
    let host = req.headers().get("Host").unwrap().to_str().unwrap().to_string();
    let conn_tx = get_site_host(host);
    if conn_tx.is_none() {
        return Ok(not_found());
    }
    // 通知客户端开始连接已就绪
    let req_id = random_string(64);
    let (tx, mut rx) = mpsc::channel(128);
    conn_tx.unwrap().send(Connection { id: req_id, tx }).await.unwrap();
    debug!("send done");

    // 准备Request数据
    let raw_req = build_raw_request(req).await.unwrap();
    let (htx, mut hrx) = mpsc::channel(128);
    let (btx, brx) = mpsc::channel(128);
    tokio::spawn(async move {
        while let Some(xd) = rx.recv().await {
            match xd {
                XData::TX(tx) => {
                    debug!("start send ProxyRequest");
                    tx.send(raw_req.clone()).await.unwrap();
                }
                XData::Data(resp) => {
                    if resp.eq("EOF".as_bytes()) {
                        break;
                    }

                    println!("response data: {:?}", String::from_utf8_lossy(&*resp));
                    let resp_length = resp.len();
                    let mut header_length = 1024;
                    if resp_length < header_length {
                        header_length = resp_length;
                    }
                    let mut body = resp.clone();
                    if let Some(split_idx) = String::from_utf8_lossy(&resp[..header_length]).find("\r\n\r\n") {
                        let header = resp[..split_idx + 2].to_owned();
                        htx.send(header).await.unwrap();
                        body = resp[split_idx + 4..].to_owned();
                    }

                    let br: Result<Vec<u8>, Error> = Ok(body);
                    btx.send(br).await.unwrap();
                }
            }
        }
    });

    // 解析Headers
    let headers = hrx.recv().await.unwrap();
    let builder = init_builder_from_headers(headers);
    builder.body(Body::wrap_stream(ReceiverStream::new(brx)))
}

fn init_builder_from_headers(header: Vec<u8>) -> Builder {
    debug!("raw_headers:{:?}", String::from_utf8_lossy(header.clone().as_slice()));
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut resp = httparse::Response::new(&mut headers);
    resp.parse(header.as_slice()).unwrap();

    // 设置Headers
    let mut builder = Response::builder().status(resp.code.unwrap());
    let header_map = builder.headers_mut().unwrap();
    for h in headers.iter().clone() {
        if h.name.is_empty() {
            continue;
        }

        header_map.insert(HeaderName::try_from(h.name).unwrap(), HeaderValue::try_from(h.value).unwrap());
    }
    debug!("final_headers:{:?}", header_map);
    builder
}
