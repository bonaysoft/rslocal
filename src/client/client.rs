pub mod api {
    tonic::include_proto!("api");
}

use std::borrow::Borrow;
use std::error::Error;
use std::str::FromStr;
use anyhow::anyhow;
use log::{debug, info};
use tokio::{io};
use tokio::net::{TcpSocket};
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;
use tonic::transport::{Channel, Endpoint};
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};
use crate::client::client::api::rs_locald_client::RsLocaldClient;
use crate::client::api::{LoginBody, LoginReply, ProxyRequest, ProxyResponse, RequestEntry};
use thiserror::Error;
use tokio_stream::StreamExt;
use tonic::codegen::{InterceptedService, StdError};
use tonic::service::Interceptor;

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
        request.metadata_mut().insert("session_id", self.session.parse().unwrap());
        Ok(request)
    }
}


pub struct Tunnel {
    client: RsLocaldClient<InterceptedService<Channel, SessionInterceptor>>,

    user_info: LoginReply,
}

impl Tunnel {
    // 连接服务器并完成登录
    pub async fn connect(endpoint: &str, token: &str, subdomain: &str) -> anyhow::Result<Tunnel> {
        let ep = Endpoint::from_str(endpoint)?;
        let channel = ep.connect().await?;

        // 登录逻辑，使用Token连接服务器获取session_id
        let req = Request::new(LoginBody { token: token.to_string(), subdomain: subdomain.to_string() });
        let login_svc = RsLocaldClient::new(channel.clone()).login(req).await?;
        let user_info = login_svc.into_inner();

        // 注入session_id
        let interceptor = SessionInterceptor::new(user_info.session_id.clone());
        let client = RsLocaldClient::with_interceptor(channel, interceptor);
        Ok(Tunnel { client, user_info })
    }

    pub async fn start(&mut self, protocol: &str, port: String) -> anyhow::Result<()> {
        let target = format!("127.0.0.1:{}", port);
        println!("Username: {}", self.user_info.username);
        println!("Forwarding: {} => {}", self.user_info.endpoint, target);

        if let Err(err) = match protocol {
            "HTTP" => self.build_http_tunnel(target).await,
            "TCP" => self.build_tcp_tunnel(target).await,
            _ => Err(ClientError::Other(anyhow!("unknown protocol")))
        } {
            return match err {
                ClientError::Connect(err) => { Err(anyhow!("{}", err.source().unwrap().to_string())) }
                ClientError::Status(status) => { Err(anyhow!("{}", status.message())) }
                ClientError::Other(err) => { Err(err) }
            };
        }
        Ok(())
    }

    async fn build_http_tunnel(&mut self, target: String) -> Result<(), ClientError> {
        let response = self.client.listen(()).await?;
        let mut resp_stream = response.into_inner();
        while let Some(resp_stream_result) = resp_stream.next().await {
            let proxy_request = resp_stream_result.unwrap(); //todo 处理连接断开的情况
            let client = self.client.clone();
            tokio::spawn(async move {
                debug!("req_id: {:?}", proxy_request.id);
                http_serve(client, proxy_request.id).await;
            });
        }
        Ok(())
    }

    async fn build_tcp_tunnel(&mut self, target: String) -> Result<(), ClientError> {
        // let response = client.listen(()).await?;
        // listen_handle(anonymous_client, response).await;
        Ok(())
    }
}

async fn http_serve(mut client: RsLocaldClient<InterceptedService<Channel, SessionInterceptor>>, proxy_request_id: String) {
    let resp = client.fetch_request(RequestEntry { id: proxy_request_id }).await.unwrap();
    let mut req_stream = resp.into_inner();
    while let Some(req_stream_result) = req_stream.next().await {
        let (tx, rx) = mpsc::channel(128);
        tokio::spawn(async move {
            let pr = req_stream_result.unwrap();
            debug!("pr: {:?}", pr.req_id);

            proxy_transfer(pr, tx).await
        });

        client.publish_response(ReceiverStream::new(rx)).await.unwrap();
    }
}

async fn proxy_transfer(pr: ProxyRequest, tx: Sender<ProxyResponse>) {
    let addr = "127.0.0.1:8000".parse().unwrap();

    // 建立连接
    // todo 复用连接
    let socket = TcpSocket::new_v4().unwrap();
    let mut stream = socket.connect(addr).await.unwrap();
    let mut req_buf = io::BufReader::new(pr.data.as_slice());
    // 发送请求
    io::copy(&mut req_buf, &mut stream).await.unwrap();
    // 接收响应
    let mut resp = vec![0u8; 0];
    io::copy(&mut stream, &mut resp).await.unwrap();
    let resp_length = resp.len();
    let mut header_length = 1024;
    if resp_length < header_length {
        header_length = resp_length;
    }

    // 分割header和body，方便服务端处理
    let split_idx = String::from_utf8_lossy(&resp[..header_length]).find("\r\n\r\n").unwrap();
    let header = resp[..split_idx + 2].to_owned();
    let data = resp[split_idx + 4..].to_owned();
    debug!("data len: {:?}", data.len());

    let per_send_length = 1 * 1024 * 1024;
    let mut start_idx = 0;
    let mut end_idx = per_send_length;
    loop {
        if end_idx > data.len() {
            end_idx = data.len();
        }
        // println!("{:?}, {:?}", start_idx, end_idx);
        let mut sheader = vec![];
        if start_idx == 0 {
            sheader = header.clone()
        }

        if tx.is_closed() {
            debug!("disconnect");
            break;
        }

        let result = tx.send(ProxyResponse { req_id: pr.req_id.clone(), header: sheader, data: data[start_idx..end_idx].to_owned() }).await;
        if result.is_err() {
            debug!("disconnect");
            break;
        }

        start_idx = start_idx + per_send_length;
        end_idx = end_idx + per_send_length;
        if start_idx >= data.len() {
            break;
        }
    }
    access_log(pr.data, header, resp_length);
}

fn access_log(req_bytes: Vec<u8>, resp_bytes: Vec<u8>, resp_length: usize) {
    // 解析http请求
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut req = httparse::Request::new(&mut headers);
    req.parse(req_bytes.as_slice()).unwrap();

    // 解析http响应
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut parsed_resp = httparse::Response::new(&mut headers);
    parsed_resp.parse(resp_bytes.as_slice()).unwrap();
    debug!("{:?}", parsed_resp.headers);

    // 输出访问日志
    // todo 支持tcp日志
    info!("\"{} {} HTTP/1.{}\" {} {} {}", req.method.unwrap().to_string(), req.path.unwrap(), req.version.unwrap(),
            parsed_resp.code.unwrap(), parsed_resp.reason.unwrap(), resp_length);
}

// async fn proxy_dispatch(response: Response<Streaming<RequestEntry>>, tx: Sender<ProxyResponse>, target: String) {
//     let addr = target.parse().unwrap();
//
//     let mut resp_stream = response.into_inner();
//     while let Some(resp_stream_result) = resp_stream.next().await {
//         let proxy_request = resp_stream_result.unwrap(); //todo 处理连接断开的情况
//         let proxy_request_id = proxy_request.id;
//         let proxy_request_data = proxy_request.data.as_slice();
//
//         // 建立连接
//         // todo 复用连接
//         let socket = TcpSocket::new_v4().unwrap();
//         let mut stream = socket.connect(addr).await.unwrap();
//         let mut req_buf = io::BufReader::new(proxy_request_data);
//         // 发送请求
//         io::copy(&mut req_buf, &mut stream).await.unwrap();
//         // 接收响应
//         let mut resp = vec![0u8; 0];
//         io::copy(&mut stream, &mut resp).await.unwrap();
//         let resp_length = resp.len();
//         let mut header_length = 1024;
//         if resp_length < header_length {
//             header_length = resp_length;
//         }
//
//         // 分割header和body，方便服务端处理
//         let split_idx = String::from_utf8_lossy(&resp[..header_length]).find("\r\n\r\n").unwrap();
//         let header = resp[..split_idx + 2].to_owned();
//         let data = resp[split_idx + 4..].to_owned();
//
//         println!("{:?}", String::from_utf8_lossy(header.as_slice()));
//         // 将Response发送会Server端
//         tx.send(ProxyResponse { req_id: proxy_request_id, header, data }).await.unwrap();
//
//         // todo 这里每一次执行可能是同一个Request，需要进行判定，在第一次执行是解析
//         // 解析http请求
//         let mut headers = [httparse::EMPTY_HEADER; 64];
//         let mut req = httparse::Request::new(&mut headers);
//         req.parse(proxy_request_data).unwrap();
//
//         // 解析http响应
//         let mut headers = [httparse::EMPTY_HEADER; 64];
//         let mut parsed_resp = httparse::Response::new(&mut headers);
//         parsed_resp.parse(resp.as_slice()).unwrap();
//         println!("{:?}", parsed_resp.headers);
//
//         // 输出访问日志
//         // todo 支持tcp日志
//         info!("\"{} {} HTTP/1.{}\" {} {} {}", req.method.unwrap().to_string(), req.path.unwrap(), req.version.unwrap(),
//             parsed_resp.code.unwrap(), parsed_resp.reason.unwrap(), resp_length);
//     }
// }
