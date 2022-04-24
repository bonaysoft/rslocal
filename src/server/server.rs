use std::fmt::format;
use std::pin::Pin;
use std::thread;
use std::thread::sleep;
use std::time::Duration;

use chrono::{DateTime, Utc};
use futures::{Stream, StreamExt};
use getrandom::getrandom;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream};
use tonic::{transport::Server, Request, Response, Status};
use crate::server::api::{LoginBody, LoginReply, ProxyRequest, ProxyResponse};
use crate::server::api::rs_locald_server::{RsLocald, RsLocaldServer};
use crate::server::{Site, VHOST, webserver};

pub mod api {
    tonic::include_proto!("api");
}

#[derive(Debug)]
pub struct RSLServer {
    tx: mpsc::Sender<Result<ProxyResponse, Status>>,
    rx: mpsc::Receiver<Result<ProxyResponse, Status>>,
}

impl RSLServer {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel(128);
        Self { tx: tx, rx: rx }
    }
}

#[tonic::async_trait]
impl RsLocald for RSLServer {
    async fn login(&self, request: Request<LoginBody>) -> Result<Response<LoginReply>, Status> {
        // todo 验证token是否正确
        // todo 根据token获取用户名
        
        // 将连接加入vHost列表
        let username = "saltbo";
        let domain = "localtest.me:3000";
        let host = format!("{}.{}", username, domain);
        VHOST.lock().unwrap().insert(host.to_string(), Site {});

        Ok(Response::new(LoginReply { endpoint: host }))
    }

    type ListenStream = Pin<Box<dyn Stream<Item=Result<ProxyRequest, Status>> + Send>>;

    async fn listen(&self, req: Request<()>) -> Result<Response<Self::ListenStream>, Status> {
        println!("\tclient connected from: {:?}", req.remote_addr());

        let (tx, rx) = mpsc::channel(128);

        // this spawn here is required if you want to handle connection error.
        // If we just map `in_stream` and write it back as `out_stream` the `out_stream`
        // will be drooped when connection error occurs and error will never be propagated
        // to mapped version of `in_stream`.
        tokio::spawn(async move {
            for i in 0..100 {
                let now: DateTime<Utc> = Utc::now();
                let mut buf = [0u8; 1];
                getrandom(&mut buf).unwrap();
                tx.send(Ok(ProxyRequest { req_id: buf[0].to_string(), data: format!("{}: {}", i, buf[0]).into_bytes() })).await;
            }

            sleep(Duration::from_secs(30));
        });

        // echo just write the same data that was received
        let out_stream = ReceiverStream::new(rx);

        Ok(Response::new(
            Box::pin(out_stream) as Self::ListenStream
        ))
    }

    async fn send_response(&self, request: Request<api::ProxyResponse>) -> Result<Response<()>, Status> {
        let response = request.into_inner();
        let req_id = response.req_id;
        // todo 通过conn_id查找目标连接

        // todo 响应外部请求连接

        println!("{}: {:?}", req_id, String::from_utf8(response.data).unwrap());

        Ok(Response::new(()))
    }

    // todo
    // 在外部监听另一个端口接收数据，拿到请求后匹配vHost并把请求发给目标vHost
}

#[tokio::main]
pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::1]:8422".parse()?;
    let rsl = RSLServer::new();

    thread::spawn(webserver);

    println!("1111");
    Server::builder()
        .add_service(RsLocaldServer::new(rsl))
        .serve(addr)
        .await?;
    Ok(())
}