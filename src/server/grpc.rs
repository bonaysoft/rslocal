use std::pin::Pin;
use std::collections::HashMap;
use std::sync::Mutex;
use anyhow::anyhow;

use futures::{Stream, StreamExt};
use lazy_static::lazy_static;
use rand::{Rng, thread_rng};
use rand::distributions::Alphanumeric;
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream};
use tonic::{transport::Server, Request, Response, Status, Streaming};
use crate::server::api::{LoginBody, LoginReply, ProxyRequest, ProxyResponse};
use crate::server::api::rs_locald_server::{RsLocald, RsLocaldServer};
use crate::server::{Config, get_site_host, remove_site_host, setup_site_host, web};
use crate::server::config::Core;

pub mod api {
    tonic::include_proto!("api");
}

#[derive(Debug)]
pub struct RSLServer {
    cfg: Config,
}

const AUTH_METHOD_TOKEN: &str = "token";
const AUTH_METHOD_OIDC: &str = "oidc";

impl RSLServer {
    pub fn new(cfg: Config) -> Self {
        Self { cfg }
    }

    fn token2username(&self, token: String) -> Result<String, Status> {
        let cfg = self.cfg.clone();
        if cfg.core.auth_method == AUTH_METHOD_OIDC.to_string() {
            // todo implement oidc auth
            return Err(Status::invalid_argument("oidc not implement"));
        }

        for (k, v) in cfg.tokens {
            if v == token {
                return Ok(k);
            }
        };

        Err(Status::invalid_argument("invalid token"))
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

        // 验证token是否正确并获取用户名
        let username = self.token2username(token)?;
        println!("{}", username);

        // 如果没有指定子域名则随机生成一个
        let mut subdomain = param.subdomain;
        if subdomain.is_empty() {
            subdomain = random_string(8);
        }

        let vhost = format!("{}.{}", subdomain, self.cfg.http.default_domain).to_lowercase();
        let session_id: String = random_string(128);
        println!("{:?}", session_id);

        // 存储 token => vhost 对应关系, 在listen里需要获取vhost
        SESSIONS.lock().unwrap().insert(session_id.clone(), vhost.clone());
        Ok(Response::new(LoginReply {
            session_id,
            username,
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
                // println!("got {:?}", msg.req.data.len()); // 接收来自入口的请求

                // 发送给目标服务
                tx.send(Result::Ok(ProxyRequest { req_id: msg.req.req_id, data: msg.req.data })).await.unwrap();

                // 等待目标服务响应
                let response = irx.recv().await.unwrap().unwrap();
                msg.header_tx.send(response.header).await.unwrap();
                msg.body_tx.send(Ok(response.data)).await.unwrap();
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
pub async fn grpc_serve() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = Config::new().unwrap();
    let addr = cfg.core.bind_addr.parse()?;
    let rsl = RSLServer::new(cfg);

    Server::builder()
        .add_service(RsLocaldServer::new(rsl))
        .serve(addr)
        .await?;
    Ok(())
}

fn random_string(len: usize) -> String {
    thread_rng().sample_iter(&Alphanumeric).take(len).map(char::from).collect()
}