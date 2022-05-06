use std::pin::Pin;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use futures::{Stream, StreamExt};
use futures_util::SinkExt;
use lazy_static::lazy_static;
use log::debug;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;
use tokio::time::sleep;
use tokio_stream::{wrappers::ReceiverStream};
use tonic::{Request, Response, Status, Streaming};
use crate::{client, random_string};
use crate::server::api::{LoginBody, LoginReply, TransferBody, TransferReply, ListenNotification, Protocol, ListenParam, TStatus};
use crate::server::api::tunnel_server::{Tunnel};
use crate::server::{Config, grpc, Payload, XData, CONNS, conns_get};
use crate::server::api::user_server::User;

pub mod api {
    tonic::include_proto!("api");
}

lazy_static! {
    pub static ref SESSIONS: Mutex<HashMap<String, String>> = Mutex::new(HashMap::new());
}

pub fn check_auth(req: Request<()>) -> Result<Request<()>, Status> {
    println!("{:?}", req);
    match req.metadata().get("authorization") {
        Some(session) => {
            if let Some(_) = SESSIONS.lock().unwrap().get(session.to_str().unwrap()) {
                return Ok(req);
            }
            Err(Status::unauthenticated("invalid session"))
        }

        _ => Err(Status::unauthenticated("No valid auth token")),
    }
}

const AUTH_METHOD_TOKEN: &str = "token";
const AUTH_METHOD_OIDC: &str = "oidc";

#[derive(Debug)]
pub struct RSLUser {
    cfg: Config,
}

impl RSLUser {
    pub fn new(cfg: Config) -> Self {
        RSLUser { cfg }
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

#[tonic::async_trait]
impl User for RSLUser {
    async fn login(&self, request: Request<LoginBody>) -> Result<Response<LoginReply>, Status> {
        let param = request.into_inner();
        let token = param.token;

        // 验证token是否正确并获取用户名
        let username = self.token2username(token)?;
        debug!("{}", username);

        let session_id: String = random_string(128);
        debug!("{:?}", session_id);

        // 存储Session
        SESSIONS.lock().unwrap().insert(session_id.clone(), username.clone());
        Ok(Response::new(LoginReply {
            session_id,
            username,
        }))
    }
}

const ACTION_READY: &str = "ready";
const ACTION_COMING: &str = "coming";

#[derive(Debug)]
pub struct RSLServer {
    cfg: Config,
    tx_tcp: Sender<Payload>,
    tx_http: Sender<Payload>,

    // conns: HashMap<String, server::Connection>,
}

impl RSLServer {
    pub fn new(cfg: Config, tx_tcp: Sender<Payload>, tx_http: Sender<Payload>) -> Self {
        Self { cfg, tx_tcp, tx_http }
    }

    fn build_open_endpoint(&self, lp: ListenParam) -> String {
        match Protocol::from_i32(lp.protocol).unwrap() {
            Protocol::Http => {
                // 如果没有指定子域名则随机生成一个
                let mut subdomain = lp.subdomain;
                if subdomain.is_empty() {
                    subdomain = random_string(8);
                }

                format!("http://{}.{}", subdomain, self.cfg.http.default_domain).to_lowercase()
            }
            Protocol::Tcp => format!("tcp://0.0.0.0:{}", 50000), // todo 从允许的范围内获取可用的端口
        }
    }

    fn select_protocol_tx(&self, protocol: Protocol) -> Sender<Payload> {
        match protocol {
            Protocol::Http => self.tx_http.clone(),
            Protocol::Tcp => self.tx_tcp.clone(),
            // Protocol::Udp => {}
        }
    }
}

#[tonic::async_trait]
impl Tunnel for RSLServer {
    type ListenStream = Pin<Box<dyn Stream<Item=Result<grpc::api::ListenNotification, Status>> + Send>>;

    async fn listen(&self, req: tonic::Request<grpc::api::ListenParam>) -> Result<Response<Self::ListenStream>, Status> {
        debug!("client connected from: {:?}", req.remote_addr());
        let lp = req.into_inner();
        let open_endpoint = self.build_open_endpoint(lp.clone());
        let event_tx = self.select_protocol_tx(Protocol::from_i32(lp.protocol).unwrap());

        // todo 检查开放端点是否已存在
        // if get_site_host(host.clone()).is_some() {
        //     return Err(Status::already_exists("vhost already exist"));
        // }

        debug!("start a new endpoint: {:?}", open_endpoint);
        let (tx, rx) = mpsc::channel(128);
        tx.send(Ok(ListenNotification { action: ACTION_READY.to_string(), message: open_endpoint.clone() })).await.unwrap();

        let txc = tx.clone();
        let etx = event_tx.clone();
        let oec = open_endpoint.clone();
        tokio::spawn(async move {
            loop {
                if txc.is_closed() {
                    debug!("closed");
                    let (tx, _) = mpsc::channel(128);
                    etx.send(Payload { tx, bind_addr: oec }).await.unwrap();
                    return;
                }
                sleep(Duration::from_secs(1)).await;
            }
        });

        // 通知有新客户端连入
        let (otx, mut orx) = mpsc::channel(128);
        event_tx.send(Payload { tx: otx, bind_addr: open_endpoint }).await.unwrap();
        debug!("send done");

        tokio::spawn(async move {
            while let Some(conn) = orx.recv().await {
                if tx.is_closed() { break; }
                debug!("req_id {:?}", conn.id); // 接收来自入口的请求

                // 发送给目标服务
                CONNS.lock().unwrap().insert(conn.id.clone(), conn.clone());
                tx.send(Ok(ListenNotification { action: ACTION_COMING.to_string(), message: conn.id.clone() })).await.unwrap();
            }
            debug!("orx exit");
        });

        Ok(Response::new(
            Box::pin(ReceiverStream::new(rx)) as Self::ListenStream
        ))
    }

    type TransferStream = Pin<Box<dyn Stream<Item=Result<TransferReply, Status>> + Send>>;

    async fn transfer(&self, req: Request<Streaming<TransferBody>>) -> Result<Response<Self::TransferStream>, Status> {
        let (req_tx, req_rx) = mpsc::channel(128);

        tokio::spawn(async move {
            let mut in_stream = req.into_inner();
            while let Some(result) = in_stream.next().await {
                match result {
                    Ok(pr) => {
                        let conn = conns_get(pr.conn_id.clone()).unwrap();
                        let ts = TStatus::from_i32(pr.status).unwrap();
                        match ts {
                            TStatus::Ready => {
                                // 这里是要发送出去的请求数据
                                let (tx, mut rx) = mpsc::channel(128);
                                conn.tx.send(XData::TX(tx)).await.unwrap(); // 通知Conn开始接收请求数据
                                while let Some(req_data) = rx.recv().await {
                                    debug!("send req len: {:?}", req_data.len());
                                    if req_data.is_empty() {
                                        break;
                                    }

                                    req_tx.send(Ok(TransferReply { conn_id: pr.conn_id.clone(), req_data })).await.unwrap();
                                }
                                req_tx.send(Ok(TransferReply { conn_id: pr.conn_id.clone(), req_data: vec![] })).await.unwrap();
                                debug!("send req done");
                            }
                            TStatus::Working => {
                                // 返回接收到的响应数据
                                debug!("receive resp len: {}", pr.resp_data.len());
                                conn.tx.send(XData::Data(pr.resp_data)).await.unwrap();
                            }
                            TStatus::Done => {
                                debug!("receive resp done");
                                conn.tx.send(XData::Data(Vec::from("EOF"))).await.unwrap();
                                break;
                            }
                        }
                    }
                    Err(err) => {
                        debug!("err: {:?}", err);
                    }
                }
            }
        });

        // 这里是需要发出去的请求数据
        Ok(Response::new(
            Box::pin(ReceiverStream::new(req_rx)) as Self::TransferStream
        ))
    }
}