use std::pin::Pin;

use futures::{Stream, StreamExt};
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream};
use tonic::{transport::Server, Request, Response, Status, Streaming};
use crate::server::api::{LoginBody, LoginReply, ProxyRequest, ProxyResponse};
use crate::server::api::rs_locald_server::{RsLocald, RsLocaldServer};
use crate::server::{setup_site_host};

pub mod api {
    tonic::include_proto!("api");
}

#[derive(Debug)]
pub struct RSLServer {}

impl RSLServer {
    pub fn new() -> Self {
        Self {}
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
        // VHOST.lock().unwrap().insert(host.to_string(), Site { tx });

        // Ok(Response::new(LoginReply { endpoint: host }))
        todo!()
    }

    type ListenStream = Pin<Box<dyn Stream<Item=Result<ProxyRequest, Status>> + Send>>;

    async fn listen(&self, resp: Request<Streaming<ProxyResponse>>) -> Result<Response<Self::ListenStream>, Status> {
        println!("\tclient connected from: {:?}", resp.remote_addr());
        let mut resp_stream = resp.into_inner();

        let (tx, rx) = mpsc::channel(128);
        tokio::spawn(async move {
            let (otx, mut orx) = mpsc::channel(128);
            setup_site_host("host".to_string(), otx);
            println!("aaaa");

            while let Some(msg) = orx.recv().await {
                println!("got {:?}", msg); // 接收来自入口的请求

                // 发送给目标服务
                tx.send(Result::Ok(ProxyRequest { req_id: msg.req.req_id, data: msg.req.data })).await;

                // 等待目标服务响应
                let response = resp_stream.next().await;
                msg.otx.send(response.unwrap().unwrap());
            }
        });

        // echo just write the same data that was received
        let out_stream = ReceiverStream::new(rx);

        Ok(Response::new(
            Box::pin(out_stream) as Self::ListenStream
        ))
    }
}

#[tokio::main]
pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::1]:8422".parse()?;
    let rsl = RSLServer::new();

    println!("1111");
    Server::builder()
        .add_service(RsLocaldServer::new(rsl))
        .serve(addr)
        .await?;
    Ok(())
}