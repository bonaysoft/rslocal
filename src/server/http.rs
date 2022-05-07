use std::collections::HashMap;
use std::fmt::Error;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use http::{HeaderMap, HeaderValue, Request, Response, StatusCode};
use http::header::HeaderName;
use http::response::Builder;
use hyper::{Body};
use hyper::service::{Service};
use log::{debug, info};
use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, Mutex};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio_stream::wrappers::ReceiverStream;
use url::Url;
use crate::random_string;
use crate::server::{Connection, Payload, XData};
use crate::server::config::HTTPConfig;

static NOTFOUND: &[u8] = b"vHost Not Found";

#[derive(Clone)]
pub struct HttpServer {
    pub inner: Arc<Mutex<HttpServerInner>>,
}

impl HttpServer {
    pub fn new(cfg: HTTPConfig) -> Self {
        Self { inner: Arc::new(Mutex::new(HttpServerInner::new(cfg))) }
    }
}

impl Service<Request<Body>> for HttpServer {
    type Response = Response<Body>;
    type Error = hyper::Error;
    type Future = Pin<Box<dyn Future<Output=Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let inner = Arc::clone(&self.inner);
        let res = async move {
            inner.lock().await.proxy(req).await
        };

        Box::pin(res)
    }
}

#[derive(Clone)]
pub struct HttpServerInner {
    cfg: HTTPConfig,

    vhosts: HashMap<String, Sender<Connection>>,
}

impl HttpServerInner {
    pub fn new(cfg: HTTPConfig) -> Self {
        HttpServerInner { cfg, vhosts: Default::default() }
    }

    pub async fn event_handler(&mut self, pl: Payload) {
        let u = Url::parse(pl.entrypoint.as_str()).unwrap();
        let mut host = u.host_str().unwrap().to_string();
        if let Some(port) = u.port() {
            host = format!("{}:{}", host, port);
        }

        if pl.tx.is_closed() {
            debug!("host {:?} removed", host);
            self.vhosts.remove(host.as_str());
            return;
        }

        debug!("host {:?} registered", host);
        self.vhosts.insert(host, pl.tx);
        debug!("vhosts {:?}", self.vhosts);
    }

    async fn vhost_match(&self, headers: HeaderMap) -> Option<&Sender<Connection>> {
        let host = headers.get("Host").unwrap().to_str().unwrap().to_string();
        debug!("host: {}", host);
        debug!("vhosts: {:?}", self.vhosts);
        return self.vhosts.get(host.as_str());
    }

    fn vhost_not_found(&self) -> Response<Body> {
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(NOTFOUND.into())
            .unwrap()
    }

    async fn proxy(&self, req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
        let vhost = self.vhost_match(req.headers().clone()).await;
        if vhost.is_none() {
            return Ok(self.vhost_not_found());
        }

        // 通知客户端开始连接已就绪
        let req_id = random_string(64);
        let (tx, rx) = mpsc::channel(128);
        vhost.unwrap().send(Connection { id: req_id, tx }).await.unwrap();
        debug!("send done");

        // 准备接收客户端发送的数据并转发
        let (htx, mut hrx) = mpsc::channel(128);
        let (btx, brx) = mpsc::channel(128);
        tokio::spawn(async move {
            Self::transfer(rx, req, htx, btx).await;
        });

        // 解析Headers
        let headers = hrx.recv().await.unwrap();
        let builder = Self::init_builder_from_headers(headers);
        Ok(builder.body(Body::wrap_stream(ReceiverStream::new(brx))).unwrap())
    }

    async fn transfer(mut rx: Receiver<XData>, req: Request<Body>, htx: Sender<Vec<u8>>, btx: Sender<Result<Vec<u8>, Error>>) {
        let req_bytes = Self::build_raw_request(req).await.unwrap();
        while let Some(xd) = rx.recv().await {
            match xd {
                XData::TX(tx) => {
                    debug!("start send ProxyRequest");
                    tx.send(req_bytes.clone()).await.unwrap();
                }
                XData::Data(resp) => {
                    if resp.eq("EOF".as_bytes()) {
                        break;
                    }

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
}

#[derive(Clone)]
pub struct MakeHttpServer {
    pub http_server: HttpServer,
}

impl<T> Service<T> for MakeHttpServer {
    type Response = HttpServer;
    type Error = hyper::Error;
    type Future = Pin<Box<dyn Future<Output=Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _: T) -> Self::Future {
        let inner = self.http_server.clone();
        let fut = async move { Ok(inner) };
        Box::pin(fut)
    }
}