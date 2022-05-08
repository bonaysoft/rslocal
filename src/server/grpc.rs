use std::pin::Pin;
use std::collections::HashMap;
use std::sync::{Arc};
use std::time::Duration;
use dashmap::DashSet;

use futures::{Stream, StreamExt};
use log::{debug, info};
use tokio::sync::{mpsc, Mutex, MutexGuard};
use tokio::sync::mpsc::Sender;
use tokio::time::sleep;
use tokio_stream::{wrappers::ReceiverStream};
use tonic::{Request, Response, Status, Streaming};
use tonic::service::Interceptor;
use crate::{random_string};
use crate::server::api::{LoginBody, LoginReply, TransferBody, TransferReply, ListenNotification, Protocol, ListenParam, TStatus};
use crate::server::api::tunnel_server::{Tunnel};
use crate::server::{Config, grpc, Payload, XData, Connection};
use crate::server::api::user_server::User;

pub mod api {
    tonic::include_proto!("api");
}

const AUTH_METHOD_TOKEN: &str = "token";
const AUTH_METHOD_OIDC: &str = "oidc";

#[derive(Debug, Clone)]
pub struct RSLUser {
    cfg: Config,

    sessions: Arc<parking_lot::Mutex<HashMap<String, String>>>,
}

impl Interceptor for RSLUser {
    fn call(&mut self, req: tonic::Request<()>) -> Result<tonic::Request<()>, Status> {
        let ss = self.sessions.clone();

        match req.metadata().get("authorization") {
            Some(session) => {
                if let Some(_) = ss.lock().get(session.to_str().unwrap()) {
                    return Ok(req);
                }
                Err(Status::unauthenticated("invalid session"))
            }

            _ => Err(Status::unauthenticated("No valid auth token")),
        }
    }
}

impl RSLUser {
    pub fn new(cfg: Config) -> Self {
        RSLUser { cfg, sessions: Arc::new(Default::default()) }
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
        info!("user {} logged in", username);

        let session_id: String = random_string(128);
        debug!("user {} session: {:?}", username, session_id);

        // 存储Session
        let mut sessions = self.sessions.lock();
        sessions.insert(session_id.clone(), username.clone());
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

    conns: Arc<Mutex<HashMap<String, Connection>>>,
    entrypoints: Arc<Mutex<DashSet<String>>>,
}

impl RSLServer {
    pub fn new(cfg: Config, tx_tcp: Sender<Payload>, tx_http: Sender<Payload>) -> Self {
        Self { cfg, tx_tcp, tx_http, conns: Default::default(), entrypoints: Default::default() }
    }

    fn build_http_host(&self, oep_set: &MutexGuard<DashSet<String>>, lp: ListenParam) -> Result<String, Status> {
        let mut subdomain = lp.subdomain;
        if subdomain.is_empty() {
            subdomain = random_string(8); // 如果没有指定子域名则随机生成一个
            // fixme: 又很小几率生成出来的正好已经在使用了，这时会报错。
            // fixme: 随机生成时应该保证生成的域名可用，不应该报错
        }

        let key = format!("http://{}.{}", subdomain, self.cfg.http.default_domain).to_lowercase();
        if oep_set.contains(key.as_str()) {
            return Err(Status::already_exists("subdomain already exist"));
        }

        Ok(key.to_string())
    }

    fn build_tcp_addr(&self, oep_set: &MutexGuard<DashSet<String>>) -> Result<String, Status> {
        let (min_str, max_str) = self.cfg.core.allow_ports.split_once("-").unwrap();
        let min: u16 = min_str.parse().unwrap();
        let max: u16 = max_str.parse().unwrap();
        for port in min..max {
            let oep = format!("tcp://0.0.0.0:{}", port);
            if !oep_set.contains(oep.as_str()) {
                return Ok(oep);
            }
        }
        // todo 目前仅支持自动生成一个端口，不支持指定端口号。是否有必要向http的subdomain给tcp增加一个remote-port

        Err(Status::internal("none valid tcp port"))
    }

    async fn build_entrypoint(&self, lp: ListenParam) -> Result<String, Status> {
        let oep_set = self.entrypoints.lock().await;
        let oep_result = match Protocol::from_i32(lp.protocol).unwrap() {
            Protocol::Http => self.build_http_host(&oep_set, lp),
            Protocol::Tcp => self.build_tcp_addr(&oep_set)
        };

        if let Ok(key) = oep_result {
            oep_set.insert(key.clone());
            return Ok(key);
        };

        oep_result
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
        info!("client connected from: {:?}", req.remote_addr());
        let lp = req.into_inner();
        let event_tx = self.select_protocol_tx(Protocol::from_i32(lp.protocol).unwrap());

        // 创建一个外部访问端点
        let entrypoint = self.build_entrypoint(lp.clone()).await?;
        info!("entrypoint: {} registered", entrypoint);
        let (tx, rx) = mpsc::channel(128);
        tx.send(Ok(ListenNotification { action: ACTION_READY.to_string(), message: entrypoint.clone() })).await.unwrap();

        // 监听客户端断开
        let txc = tx.clone();
        let etx = event_tx.clone();
        let epc = entrypoint.clone();
        let eps = self.entrypoints.clone();
        tokio::spawn(async move {
            loop {
                if txc.is_closed() {
                    let (tx, _) = mpsc::channel(128);
                    etx.send(Payload { tx, entrypoint: epc.clone() }).await.unwrap();
                    eps.lock().await.remove(epc.as_str());
                    info!("entrypoint {} unregistered", epc);
                    return;
                }
                sleep(Duration::from_secs(1)).await;
            }
        });

        // 通知有新客户端连入
        let (otx, mut orx) = mpsc::channel(128);
        event_tx.send(Payload { tx: otx, entrypoint }).await.unwrap();
        debug!("send done");

        // 监听外部请求
        let conns = Arc::clone(&self.conns);
        tokio::spawn(async move {
            while let Some(conn) = orx.recv().await {
                if tx.is_closed() { break; }
                info!("coming new connection: {}", conn.id); // 接收来自入口的请求

                // 发送给目标服务
                conns.lock().await.insert(conn.id.clone(), conn.clone());
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
        let conns = Arc::clone(&self.conns);
        tokio::spawn(async move {
            let mut in_stream = req.into_inner();
            while let Some(result) = in_stream.next().await {
                let pr = result.unwrap();
                let mg = conns.lock().await;
                let conn = mg.get(pr.conn_id.as_str()).unwrap();
                let ts = TStatus::from_i32(pr.status).unwrap();
                match ts {
                    TStatus::Ready => {
                        debug!("connection ready to transfer: {}", pr.conn_id);
                        let rtx = req_tx.clone();
                        let (tx, mut rx) = mpsc::channel(128);
                        conn.tx.send(XData::TX(tx)).await.unwrap(); // 通知Conn开始接收请求数据
                        // 使用异步来接收数据，否则会导致mutex无法释放
                        tokio::spawn(async move {
                            // 这里是要发送出去的请求数据
                            while let Some(req_data) = rx.recv().await {
                                debug!("send req len: {:?}", req_data.len());
                                if req_data.is_empty() {
                                    break;
                                }

                                rtx.send(Ok(TransferReply { conn_id: pr.conn_id.clone(), req_data })).await.unwrap();
                            }
                            rtx.send(Ok(TransferReply { conn_id: pr.conn_id.clone(), req_data: vec![] })).await.unwrap();
                            debug!("send req done");
                        });
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
        });

        // 这里是需要发出去的请求数据
        Ok(Response::new(
            Box::pin(ReceiverStream::new(req_rx)) as Self::TransferStream
        ))
    }
}