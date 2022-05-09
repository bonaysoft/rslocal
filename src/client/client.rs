pub mod api {
    tonic::include_proto!("api");
}

use std::error::Error;
use std::str::FromStr;
use futures::FutureExt;
use anyhow::anyhow;
use log::{debug, info};
use tokio::{io};
use tokio::net::{TcpStream};
use tokio::sync::{mpsc};

use tonic::transport::{Channel, Endpoint};
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Status};
use crate::server::api::tunnel_client::TunnelClient;
use crate::server::api::user_client::UserClient;
use crate::server::api::{LoginBody, LoginReply, TransferBody, TransferReply, ListenParam, Protocol, TStatus};
use thiserror::Error;
use tokio::io::{AsyncWriteExt};
use tokio_stream::StreamExt;
use tokio_util::sync::{PollSender};
use tonic::codegen::{InterceptedService};
use tonic::service::Interceptor;
use crate::{RxReader, TxWriter};

#[derive(Error, Debug)]
pub enum ClientError {
    #[error(transparent)]
    Connect(#[from] tonic::transport::Error),

    #[error(transparent)]
    Disconnect(anyhow::Error),

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
    pub async fn connect(endpoint: &str, token: &str) -> Result<Tunnel, ClientError> {
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

    async fn build_tunnel(&mut self, protocol: Protocol, target: String, subdomain: String) -> Result<(), ClientError> {
        debug!("protocol: {:?}, target: {:?}", protocol, target);
        let response = self.client.listen(ListenParam { protocol: protocol.into(), subdomain }).await?;
        let mut resp_stream = response.into_inner();
        while let Some(resp_stream_result) = resp_stream.next().await {
            if let Err(err) = resp_stream_result {
                return Err(ClientError::Disconnect(anyhow!(err)));
            }

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

    pub async fn start(&mut self, protocol: Protocol, target: String, subdomain: &str) -> Result<(), ClientError> {
        self.build_tunnel(protocol, target, subdomain.to_string()).await
    }
}

type TunClient = TunnelClient<InterceptedService<Channel, SessionInterceptor>>;

// 这个函数里处理的是一次完整请求
async fn coming_handle(mut client: TunClient, conn_id: String, _protocol: Protocol, target: String) {
    let (tx, rx) = mpsc::channel(128);
    tx.send(TransferBody { conn_id: conn_id.clone(), status: TStatus::Ready as i32, resp_data: vec![] }).await.unwrap(); // 通过conn_id连入服务端

    let (tx2, rx2) = mpsc::channel(128);
    let inbound_reader = RxReader { rx: rx2 };
    let inbound_writer = TxWriter { conn_id, tx: PollSender::new(tx) };
    tokio::spawn(async move {
        let response = client.transfer(ReceiverStream::new(rx)).await.unwrap();
        let mut resp_stream = response.into_inner();
        while let Some(received) = resp_stream.next().await {
            let tr = received.unwrap();
            tx2.send(tr).await.unwrap();
        }
    });

    tokio::spawn(transfer_to_target(inbound_reader, inbound_writer, target).map(|r| {
        debug!("transfer map: {:?}", r);
        if let Err(e) = r {
            println!("Failed to transfer; error={}", e);
        }
    }));
    // if protocol == Protocol::Http {
    //     http_access_log(req_data, resp);
    // }
}

async fn transfer_to_target(mut ri: RxReader<TransferReply>, mut wi: TxWriter<TransferBody>, proxy_addr: String) -> Result<(), Box<dyn Error>> {
    debug!("transfer");
    let mut outbound = TcpStream::connect(proxy_addr).await?;
    let (mut ro, mut wo) = outbound.split();

    let client_to_server = async {
        debug!("client_to_server");
        io::copy(&mut ri, &mut wo).await?;
        wo.shutdown().await
    };

    let server_to_client = async {
        debug!("server_to_client");
        io::copy(&mut ro, &mut wi).await?;
        wi.shutdown().await
    };

    tokio::try_join!(client_to_server, server_to_client)?;

    Ok(())
}

fn http_access_log(_req_bytes: Vec<u8>, resp: Vec<u8>) {
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
    req.parse(header.as_slice()).unwrap();

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