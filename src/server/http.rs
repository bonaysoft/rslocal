use std::borrow::Borrow;
use std::convert::Infallible;
use futures_util::StreamExt;
use http::{HeaderMap, HeaderValue, Request, Response, StatusCode};
use http::header::HeaderName;
use hyper::{Body, Client, Server};
use hyper::service::{make_service_fn, service_fn};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use crate::client::api::ProxyRequest;
use crate::server::{Config, get_site_host, R};

static NOTFOUND: &[u8] = b"Not Found";

fn uppercase_first_letter(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

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
        buf.push_str(&format!("{}: {}\r\n", uppercase_first_letter(name.as_str()), val.to_str().unwrap()));
    }
    buf.push_str("\r\n");
    // todo 准备Body数据

    // 发送给client
    let (htx, mut hrx) = mpsc::channel(128);
    let (btx, brx) = mpsc::channel(128);
    let warp_req: R = R { header_tx: htx, body_tx: btx, req: ProxyRequest { req_id: "".to_string(), data: buf.into_bytes() } };
    tx.unwrap().send(warp_req).await.unwrap();

    // 解析Headers
    let header = hrx.recv().await.unwrap();
    let mut headers = [httparse::EMPTY_HEADER; 64];
    httparse::Response::new(&mut headers).parse(header.as_slice()).unwrap();

    // 接收Body响应并返回
    let mut builder = Response::builder();
    let header_map = builder.headers_mut().unwrap();
    for h in headers.iter().clone() {
        if h.name.is_empty() { continue; }
        header_map.insert(HeaderName::try_from(h.name).unwrap(), HeaderValue::try_from(h.value).unwrap());
    }

    println!("{:?}", header_map);
    let stream = ReceiverStream::new(brx);
    let body = builder.body(Body::wrap_stream(stream));
    Ok(body.unwrap())
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