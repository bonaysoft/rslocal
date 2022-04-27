use std::collections::HashMap;
use std::convert::Infallible;
use std::future::Future;
use std::net::SocketAddr;
use hyper::{Body, HeaderMap, Request, Response, Server};
use hyper::service::{make_service_fn, service_fn};
use std::sync::{Arc, mpsc, Mutex};
use futures::StreamExt;
use lazy_static::lazy_static;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::oneshot;
use tonic::Status;
use crate::client::api::ProxyRequest;
use crate::server::api::ProxyResponse;

// async fn build_hello_world(tx: Sender<Result<ProxyResponse, Status>>) -> impl FnMut(Request<Body>) -> Result<Response<Body>, Infallible> {
//     move |_req: Request<Body>| -> Result<Response<Body>, Infallible> {
//         let host = _req.headers().get("Host").unwrap().to_str();
//         println!("Host: {:?}", host);
//         // todo 查找Host是否存在，如果不存在直接报错
//
//         // let v = VHOST.lock().unwrap();
//         // let site = v.get(host.unwrap());
//         // println!("{:?}", site);
//         // 转发请求数据
//         // tx.send(ProxyResponse { req_id: "".to_string(), data: _req.headers() });
//
//         // 接收返回的数据
//         // rx.receive()
//
//         // let result = site.unwrap().response(_req.headers(), _req.body())?;
//         Ok(Response::new(Default::default()))
//     }
// }

#[derive(Debug)]
pub struct R {
    pub otx: oneshot::Sender<ProxyResponse>,
    pub req: ProxyRequest,
}


async fn proxy(req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    println!("req: {:?}", req);

    // 获取input_tx
    let tx = get_site_host("host".to_string());

    // 发送数据
    let (otx, orx) = oneshot::channel();
    let result: R = R { otx, req: ProxyRequest { req_id: "".to_string(), data: "this is a req".as_bytes().to_vec() } };
    tx.send(result).await;
    let resp = orx.await.unwrap();
    Ok(Response::new(Body::from(resp.data)))
}

lazy_static! {
    pub static ref VHOST: Mutex<HashMap<String, Sender<R>>> = Mutex::new(HashMap::new());
}

pub fn get_site_host(host: String) -> Sender<R> {
    VHOST.lock().unwrap().get(host.as_str()).unwrap().clone()
}

pub fn setup_site_host(host: String, otx: Sender<R>) {
    VHOST.lock().unwrap().insert(host, otx);
}

#[tokio::main]
pub async fn webserver() {
    // We'll bind to 127.0.0.1:3000
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));

    let make_service = make_service_fn(move |_| {
        async move { Ok::<_, Infallible>(service_fn(move |req| proxy(req))) }
    });

    let server = Server::bind(&addr).serve(make_service);

    // Run this server for... forever!
    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
}