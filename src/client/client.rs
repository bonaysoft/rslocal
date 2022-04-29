pub mod api {
    tonic::include_proto!("api");
}

use std::str::FromStr;
use tokio::io;
use tokio::net::{TcpSocket};
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;
use tonic::transport::{Endpoint};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Streaming};
use crate::client::api::rs_locald_client::RsLocaldClient;
use crate::client::api::{LoginBody, ProxyRequest, ProxyResponse};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CustomError {
    #[error("connection disconnected")]
    Disconnect(String),

    #[error("{0}")]
    Message(String),

    #[error("unknown error")]
    Unknown,
}

impl From<tonic::Status> for CustomError {
    fn from(err: tonic::Status) -> Self {
        CustomError::Message(format!("{}", err.message()))
    }
}

impl From<tonic::transport::Error> for CustomError {
    fn from(err: tonic::transport::Error) -> Self {
        CustomError::Disconnect(format!("{}", err.to_string()))
    }
}

#[tokio::main]
pub async fn run(endpoint: String, token: String, target: String, subdomain: String) -> Result<(), CustomError> {
    // println!("{}", token);
    // println!("{}", endpoint);

    // 根据域名和Token连接服务器
    // 第一次连接进行登录，获取session_token和endpoint
    let mut client = RsLocaldClient::connect(endpoint.clone()).await?;
    let req = Request::new(LoginBody { token: "abc".to_string(), subdomain });
    let login_service = client.login(req).await?;
    let lr = login_service.into_inner();
    println!("{} => {}", lr.endpoint, target);

    // 第二次连接，开始监听
    let ep = Endpoint::from_str(endpoint.as_str())?;
    let channel = ep.connect().await?;
    let mut client = RsLocaldClient::with_interceptor(channel, move |mut req: Request<()>| {
        req.metadata_mut().insert("session_id", lr.session_id.parse().unwrap());
        Ok(req)
    });

    // 开始监听
    let (tx, rx) = mpsc::channel(128);
    let rs = ReceiverStream::new(rx);
    let response = client.listen(rs).await?;
    proxy_dispatch(response, tx, target).await;

    Ok(())
}

async fn proxy_dispatch(response: Response<Streaming<ProxyRequest>>, tx: Sender<ProxyResponse>, target: String) {
    println!("proxy_dispatch");
    let addr = target.parse().unwrap();

    let mut resp_stream = response.into_inner();
    while let Some(recived) = resp_stream.next().await {
        let recived = recived.unwrap();

        let socket = TcpSocket::new_v4().unwrap();
        let mut stream = socket.connect(addr).await.unwrap();
        let mut buf = io::BufReader::new(&*recived.data);
        io::copy(&mut buf, &mut stream).await.unwrap(); // 发送请求

        let mut resp = vec![0u8; 0];
        io::copy(&mut stream, &mut resp).await.unwrap(); //接收响应

        // 将Response发送会Server端
        tx.send(ProxyResponse { req_id: recived.req_id, data: resp.to_vec() }).await.unwrap();
        println!("resp: {:?}", String::from_utf8(resp));
    }
}
