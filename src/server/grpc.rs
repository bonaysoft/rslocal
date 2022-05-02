use std::pin::Pin;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use futures::{Stream, StreamExt};
use lazy_static::lazy_static;
use log::debug;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio_stream::{wrappers::ReceiverStream};
use tonic::{transport::Server, Request, Response, Status, Streaming};
use crate::random_string;
use crate::server::api::{LoginBody, LoginReply, RequestEntry, ProxyRequest, ProxyResponse};
use crate::server::api::rs_locald_server::{RsLocald, RsLocaldServer};
use crate::server::{Config, get_site_host, grpc, remove_site_host, REQS, RR, rr_get, setup_site_host, web};

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
    async fn login(&self, request: Request<LoginBody>) -> Result<Response<LoginReply>, Status> {
        let param = request.into_inner();
        let token = param.token;

        // 验证token是否正确并获取用户名
        let username = self.token2username(token)?;
        debug!("{}", username);

        // 如果没有指定子域名则随机生成一个
        let mut subdomain = param.subdomain;
        if subdomain.is_empty() {
            subdomain = random_string(8);
        }

        let vhost = format!("{}.{}", subdomain, self.cfg.http.default_domain).to_lowercase();
        let session_id: String = random_string(128);
        debug!("{:?}", session_id);

        // 存储 token => vhost 对应关系, 在listen里需要获取vhost
        SESSIONS.lock().unwrap().insert(session_id.clone(), vhost.clone());
        Ok(Response::new(LoginReply {
            session_id,
            username,
            endpoint: format!("http://{}/", vhost).to_string(),
        }))
    }

    type ListenStream = Pin<Box<dyn Stream<Item=Result<grpc::api::RequestEntry, Status>> + Send>>;

    async fn listen(&self, req: tonic::Request<()>) -> Result<Response<Self::ListenStream>, Status> {
        debug!("client connected from: {:?}", req.remote_addr());
        // todo 判断是否为tcp协议，如果是则启动一个tcp端口接收外部请求并转发给客户端
        // todo 发送给TCP服务器一个信号，拉起一个TcpListener

        // 根据session_token获取host
        let session_id = req.metadata().get("session_id").unwrap();
        let host = SESSIONS.lock().unwrap().get(session_id.to_str().unwrap()).unwrap().to_string();
        if get_site_host(host.clone()).is_some() {
            return Err(Status::already_exists("vhost already exist"));
        }

        debug!("start a new vhost: {:?}", host);
        let (tx, rx) = mpsc::channel(128);
        let txc = tx.clone();
        let host_cloned = host.clone();
        tokio::spawn(async move {
            loop {
                if txc.is_closed() {
                    debug!("closed");
                    remove_site_host(host_cloned);
                    return;
                }
                sleep(Duration::from_secs(1)).await;
            }
        });
        tokio::spawn(async move {
            let (otx, mut orx) = mpsc::channel(128);
            if !setup_site_host(host.clone(), otx) {
                debug!("vhost already exist.");
                return;
            }

            while let Some(req_id) = orx.recv().await {
                if tx.is_closed() { break; }
                debug!("req_id {:?}", req_id); // 接收来自入口的请求

                // 发送给目标服务
                tx.send(Ok(RequestEntry { id: req_id })).await.unwrap();
            }
            debug!("orx exit");
        });

        Ok(Response::new(
            Box::pin(ReceiverStream::new(rx)) as Self::ListenStream
        ))
    }

    type FetchRequestStream = Pin<Box<dyn Stream<Item=Result<ProxyRequest, Status>> + Send>>;

    async fn fetch_request(&self, req: tonic::Request<grpc::api::RequestEntry>) -> Result<Response<Self::FetchRequestStream>, Status> {
        let re = req.into_inner();
        let (tx, rx) = mpsc::channel(128);
        debug!("fetch_request: {:?}", re.id);
        REQS.lock().unwrap().insert(re.id, tx);

        Ok(Response::new(
            Box::pin(ReceiverStream::new(rx)) as Self::FetchRequestStream
        ))
    }

    async fn publish_response(&self, req: Request<Streaming<ProxyResponse>>) -> Result<Response<()>, Status> {
        let mut in_stream = req.into_inner();
        while let Some(result) = in_stream.next().await {
            match result {
                Ok(pr) => {
                    let r = rr_get(pr.req_id).unwrap();
                    if !pr.header.is_empty() {
                        r.header_tx.send(pr.header).await.unwrap();
                    }
                    r.body_tx.send(Ok(pr.data)).await.unwrap();
                }
                Err(err) => {
                    debug!("err: {:?}", err);
                }
            }
        }
        debug!("stream ended");
        Ok(Response::new(()))
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