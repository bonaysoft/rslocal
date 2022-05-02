use std::convert::Infallible;
use std::time::Duration;
use http::{HeaderValue, Request, Response, StatusCode};
use http::header::HeaderName;
use hyper::{Body, Client, Server};
use hyper::service::{make_service_fn, service_fn};
use log::{debug, info};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;
use tokio::time::{sleep};
use tokio_stream::wrappers::ReceiverStream;
use tonic::Status;
use crate::server::grpc::api::ProxyRequest;
use crate::random_string;
use crate::server::{Config, get_site_host, R, REQS, rr_set};

static NOTFOUND: &[u8] = b"Not Found";

fn not_found() -> Response<Body> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(NOTFOUND.into())
        .unwrap()
}

async fn proxy(req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    // 获取发送Request的Sender
    let host = req.headers().get("Host").unwrap().to_str().unwrap().to_string();
    let tx = get_site_host(host);
    if tx.is_none() {
        return Ok(not_found());
    }

    // 准备Request数据
    let mut buf = format!("{} {} {:?}\r\n", req.method(), req.uri(), req.version());
    for (name, val) in req.headers() {
        buf.push_str(&format!("{}: {}\r\n", name.as_str(), val.to_str().unwrap()));
    }
    buf.push_str("\r\n");
    // todo 准备Body数据
    let whole_body = hyper::body::to_bytes(req.into_body()).await?;

    // 发送给client
    let (htx, mut hrx) = mpsc::channel(128);
    let (btx, brx) = mpsc::channel(128);
    let req_id = random_string(64);
    rr_set(req_id.clone(), R { header_tx: htx, body_tx: btx });
    // debug!("req_id: {}, uri: {}",req_id, req.uri());

    debug!("start notify");
    tx.unwrap().send(req_id.clone()).await.unwrap();

    // wait client fetch
    let fetch_tx = wait_fetch(req_id.clone()).await;

    debug!("start send ProxyRequest");
    let mut data = buf.into_bytes();
    data.write_all(whole_body.as_ref()).await.unwrap(); // todo 改成流式发送
    fetch_tx.send(Ok(ProxyRequest { req_id, data })).await.unwrap();
    debug!("send done");

    // 解析Headers
    let header = hrx.recv().await.unwrap();
    debug!("raw_headers:{:?}", String::from_utf8_lossy(header.clone().as_slice()));
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut resp = httparse::Response::new(&mut headers);
    resp.parse(header.as_slice()).unwrap();

    // 接收Body响应并返回
    let mut builder = Response::builder().status(resp.code.unwrap());
    let header_map = builder.headers_mut().unwrap();
    for h in headers.iter().clone() {
        // println!("h:{:?}", h);
        if h.name.is_empty() {
            continue;
        }

        header_map.insert(HeaderName::try_from(h.name).unwrap(), HeaderValue::try_from(h.value).unwrap());
    }
    debug!("header:{:?}", header_map);
    let stream = ReceiverStream::new(brx);
    let body = builder.body(Body::wrap_stream(stream));
    Ok(body.unwrap())
}

async fn wait_fetch(id: String) -> Sender<Result<ProxyRequest, Status>> {
    loop {
        if let Some(tx) = REQS.lock().unwrap().get(id.as_str()) {
            return tx.clone();
        }

        sleep(Duration::from_micros(100)).await;
    }
}

#[tokio::main]
pub async fn http_serve() {
    let cfg = Config::new().unwrap();
    let addr = cfg.http.bind_addr.parse().unwrap();
    let make_service = make_service_fn(move |_| {
        async move { Ok::<_, Infallible>(service_fn(move |req| proxy(req))) }
    });

    let server = Server::bind(&addr)
        .http1_preserve_header_case(true)
        .http1_title_case_headers(true)
        .serve(make_service);

    println!("Listening on http://{}", addr);

    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
}