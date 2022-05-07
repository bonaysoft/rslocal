pub mod api {
    tonic::include_proto!("api");
}

use std::error::Error;
use std::str::FromStr;
use anyhow::anyhow;
use log::{debug, info};
use tokio::{io};
use tokio::net::{TcpSocket, TcpStream};
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;
use tonic::transport::{Channel, Endpoint};
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};
use crate::client::client::api::tunnel_client::TunnelClient;
use crate::client::api::{LoginBody, LoginReply, TransferBody, TransferReply, ListenParam, Protocol, TStatus};
use thiserror::Error;
use tokio::io::{AsyncWriteExt};
use tokio_stream::StreamExt;
use tonic::codegen::{InterceptedService};
use tonic::service::Interceptor;
use crate::client::api::user_client::UserClient;

#[derive(Error, Debug)]
pub enum ClientError {
    #[error(transparent)]
    Connect(#[from] tonic::transport::Error),

    #[error("{0}")]
    Status(#[from] tonic::Status),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Clone)]
struct SessionInterceptor {
    session: String,
}

impl SessionInterceptor {
    pub fn new(session: String) -> Self {
        SessionInterceptor { session }
    }
}

impl Interceptor for SessionInterceptor {
    fn call(&mut self, mut request: tonic::Request<()>) -> Result<tonic::Request<()>, Status> {
        request.metadata_mut().insert("authorization", self.session.parse().unwrap());
        Ok(request)
    }
}


pub struct Tunnel {
    client: TunnelClient<InterceptedService<Channel, SessionInterceptor>>,

    user_info: LoginReply,
}

impl Tunnel {
    // 连接服务器并完成登录
    pub async fn connect(endpoint: &str, token: &str) -> anyhow::Result<Tunnel> {
        let ep = Endpoint::from_str(endpoint)?;
        let channel = ep.connect().await?;

        // 登录逻辑，使用Token连接服务器获取session_id
        let req = Request::new(LoginBody { token: token.to_string() });
        let login_svc = UserClient::new(channel.clone()).login(req).await?;
        let user_info = login_svc.into_inner();

        // 注入session_id
        let interceptor = SessionInterceptor::new(user_info.session_id.clone());
        let client = TunnelClient::with_interceptor(channel, interceptor);
        Ok(Tunnel { client, user_info })
    }

    pub async fn start(&mut self, protocol: Protocol, target: String, subdomain: &str) -> anyhow::Result<()> {
        let result = self.build_tunnel(protocol, target, subdomain.to_string()).await;
        if let Err(err) = result {
            match err {
                ClientError::Connect(err) => { Err(anyhow!("{}", err.source().unwrap().to_string())) }
                ClientError::Status(status) => { Err(anyhow!("{}", status.message())) }
                ClientError::Other(err) => { Err(err) }
            }
        } else {
            Ok(())
        }
    }

    async fn build_tunnel(&mut self, protocol: Protocol, target: String, subdomain: String) -> Result<(), ClientError> {
        debug!("protocol: {:?}, target: {:?}", protocol, target);
        let response = self.client.listen(ListenParam { protocol: protocol.into(), subdomain }).await?;
        let mut resp_stream = response.into_inner();
        while let Some(resp_stream_result) = resp_stream.next().await {
            let ln = resp_stream_result.unwrap(); //todo 处理连接断开的情况
            match ln.action.as_str() {
                "ready" => {
                    println!("Username: {}", self.user_info.username);
                    println!("Forwarding: {} => {}", ln.message, target);
                }
                "coming" => {
                    debug!("conn_id: {:?}", ln.message);
                    let client = self.client.clone();
                    let target = target.clone();
                    tokio::spawn(async move {
                        coming_handle(client, ln.message, protocol, target).await;
                    });
                }
                _ => {}
            }
        }
        Ok(())
    }
}

type TunClient = TunnelClient<InterceptedService<Channel, SessionInterceptor>>;

// 这个函数里处理的是一次完整请求
async fn coming_handle(mut client: TunClient, conn_id: String, protocol: Protocol, target: String) {
    let (tx, rx) = mpsc::channel(128);
    tx.send(TransferBody { conn_id: conn_id.clone(), status: TStatus::Ready as i32, resp_data: vec![] }).await.unwrap(); // 通过conn_id连入服务端

    // 建立连接
    // todo 复用连接
    let addr = target.parse().unwrap();
    debug!("socket.connect to {:?}",addr);
    let socket = TcpSocket::new_v4().unwrap();
    let mut target_stream = socket.connect(addr).await.unwrap();

    let response = client.transfer(ReceiverStream::new(rx)).await.unwrap();
    let mut resp_stream = response.into_inner();
    while let Some(received) = resp_stream.next().await {
        let received = received.unwrap();
        debug!("received conn: `{}`", received.conn_id);
        let req = received.req_data;
        if req.is_empty() {
            break;
        }

        // 发送请求
        debug!("send req: {:?}",req.len());
        target_stream.write(req.as_slice()).await.unwrap();
    }

    // 接收响应
    stream_forward(target_stream, conn_id, tx).await.unwrap();
    // if protocol == Protocol::Http {
    //     http_access_log(req_data, resp);
    // }
}

async fn stream_forward(stream: TcpStream, conn_id: String, tx: Sender<TransferBody>) -> anyhow::Result<()> {
    debug!("stream_forward");
    loop {
        // Wait for the socket to be readable
        stream.readable().await?;
        debug!("start read");

        // Creating the buffer **after** the `await` prevents it from
        // being stored in the async task.
        let mut buf = vec![0u8; 48 * 1024];

        // Try to read data, this may still fail with `WouldBlock`
        // if the readiness event is a false positive.
        match stream.try_read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                debug!("read {} bytes", n);
                tx.send(TransferBody { conn_id: conn_id.clone(), status: TStatus::Working as i32, resp_data: buf[..n].to_owned() }).await.unwrap();
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                debug!("WouldBlock");
                continue;
            }
            Err(e) => {
                return Err(e.into());
            }
        }
    }

    tx.send(TransferBody { conn_id: conn_id.clone(), status: TStatus::Done as i32, resp_data: vec![] }).await.unwrap();
    debug!("stream_forward done");
    Ok(())
}

fn http_access_log(req_bytes: Vec<u8>, resp: Vec<u8>) {
    let resp_length = resp.len();
    let mut header_length = 1024;
    if resp_length < header_length {
        header_length = resp_length;
    }
    let split_idx = String::from_utf8_lossy(&resp[..header_length]).find("\r\n\r\n").unwrap();
    let header = resp[..split_idx + 2].to_owned();

    // 解析http请求
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut req = httparse::Request::new(&mut headers);
    req.parse(req_bytes.as_slice()).unwrap();

    // 解析http响应
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut parsed_resp = httparse::Response::new(&mut headers);
    parsed_resp.parse(resp.as_slice()).unwrap();
    debug!("{:?}", parsed_resp.headers);

    // 输出访问日志
    // todo 支持tcp日志
    info!("\"{} {} HTTP/1.{}\" {} {} {}", req.method.unwrap().to_string(), req.path.unwrap(), req.version.unwrap(),
            parsed_resp.code.unwrap(), parsed_resp.reason.unwrap(), resp_length);
}