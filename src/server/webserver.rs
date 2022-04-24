use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use hyper::{Body, HeaderMap, Request, Response, Server};
use hyper::service::{make_service_fn, service_fn};
use std::sync::{Arc, Mutex};
use lazy_static::*;

#[derive(Debug)]
pub struct Site {}

impl Site {
    fn dispatch(&self, headers: &HeaderMap, body: &Body) -> Result<(String), ()> {
        // 通过Channel向目标服务转发请求
        // todo 将body转发给找到的目标地址
        todo!()
    }
}

lazy_static! {
    pub static ref VHOST: Mutex<HashMap<String, Site>> = Mutex::new(HashMap::new());
}

async fn hello_world(_req: Request<Body>) -> Result<Response<Body>, Infallible> {
    let host = _req.headers().get("Host").unwrap().to_str();
    println!("Host: {:?}", host);
    // todo 查找Host是否存在，如果不存在直接报错

    let v = VHOST.lock().unwrap();
    let site = v.get(host.unwrap());
    println!("{:?}", site);

    let result = site.unwrap().dispatch(_req.headers(), _req.body())?;
    Ok(Response::new(result))
}

#[tokio::main]
pub async fn webserver() {
    // We'll bind to 127.0.0.1:3000
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));

    // A `Service` is needed for every connection, so this
    // creates one from our `hello_world` function.
    let make_svc = make_service_fn(|_conn| async {
        // service_fn converts our function into a `Service`
        Ok::<_, Infallible>(service_fn(hello_world))
    });

    let server = Server::bind(&addr).serve(make_svc);

    // Run this server for... forever!
    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
}