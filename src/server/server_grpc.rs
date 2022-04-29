use std::pin::Pin;
use std::collections::HashMap;
use std::sync::Mutex;

use futures::{Stream, StreamExt};
use lazy_static::lazy_static;
use rand::{Rng, thread_rng};
use rand::distributions::Alphanumeric;
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream};
use tonic::{transport::Server, Request, Response, Status, Streaming};
use crate::server::api::{LoginBody, LoginReply, ProxyRequest, ProxyResponse};
use crate::server::api::rs_locald_server::{RsLocald, RsLocaldServer};
use crate::server::{get_site_host, remove_site_host, setup_site_host};

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

lazy_static! {
    pub static ref SESSIONS: Mutex<HashMap<String, String>> = Mutex::new(HashMap::new());
}

#[tonic::async_trait]
impl RsLocald for RSLServer {
    async fn login(&self, mut request: Request<LoginBody>) -> Result<Response<LoginReply>, Status> {
        let param = request.into_inner();
        let token = param.token;
        let mut subdomain = param.subdomain;

        println!("{:?}", token);
        // todo 验证token是否正确
        // todo 根据token获取用户名

        if subdomain.is_empty() {
            subdomain = random_string(8);
        }

        let domain = "localtest.me:8080"; // todo 从配置文件获取根域名
        let vhost = format!("{}.{}", subdomain, domain).to_lowercase();
        let session_id: String = random_string(128);
        println!("{:?}", session_id);

        // 存储 token => vhost 对应关系, 在listen里需要获取vhost
        SESSIONS.lock().unwrap().insert(session_id.clone(), vhost.clone());
        Ok(Response::new(LoginReply {
            session_id: session_id.to_string(),
            endpoint: format!("http://{}/", vhost).to_string(),
        }))
    }

    type ListenStream = Pin<Box<dyn Stream<Item=Result<ProxyRequest, Status>> + Send>>;

    async fn listen(&self, resp: Request<Streaming<ProxyResponse>>) -> Result<Response<Self::ListenStream>, Status> {
        println!("\tclient connected from: {:?}", resp.remote_addr());
        // todo 判断是否为tcp协议，如果是则启动一个tcp端口接收外部请求并转发给客户端

        // 根据session_token获取host
        let session_id = resp.metadata().get("session_id").unwrap();
        let host = SESSIONS.lock().unwrap().get(session_id.to_str().unwrap()).unwrap().to_string();
        if get_site_host(host.clone()).is_some() {
            return Err(Status::already_exists("vhost already exist"));
        }

        println!("start a new vhost: {:?}", host);
        let host1 = host.clone();
        let host2 = host.clone();

        let (itx, mut irx) = mpsc::channel(128);
        let mut resp_stream = resp.into_inner();
        tokio::spawn(async move {
            while let Some(response) = resp_stream.next().await {
                if itx.is_closed() {
                    return;
                }

                itx.send(response).await.unwrap();
            }
            remove_site_host(host1);
            println!("client exit");
        });

        let (tx, rx) = mpsc::channel(128);
        tokio::spawn(async move {
            let (otx, mut orx) = mpsc::channel(128);
            if !setup_site_host(host2, otx) {
                println!("vhost already exist.");
                return;
            }

            while let Some(msg) = orx.recv().await {
                println!("got {:?}", msg); // 接收来自入口的请求

                // 发送给目标服务
                tx.send(Result::Ok(ProxyRequest { req_id: msg.req.req_id, data: msg.req.data })).await.unwrap();

                // 等待目标服务响应
                let response = irx.recv().await;
                // let response = resp_stream.next().await;
                msg.otx.send(response.unwrap().unwrap()).unwrap();
            }
            println!("orx exit");
        });

        // echo just write the same data that was received
        Ok(Response::new(
            Box::pin(ReceiverStream::new(rx)) as Self::ListenStream
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

fn random_string(len: usize) -> String {
    thread_rng().sample_iter(&Alphanumeric).take(len).map(char::from).collect()
}