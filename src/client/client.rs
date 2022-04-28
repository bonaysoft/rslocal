pub mod api {
    tonic::include_proto!("api");
}

use tokio::io;
use tokio::net::{TcpSocket};
use tokio::sync::mpsc::{channel};
use tonic::transport::Channel;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;
use crate::client::api::rs_locald_client::RsLocaldClient;
use crate::client::api::{LoginBody, ProxyRequest, ProxyResponse};

#[tokio::main]
pub async fn run(endpoint: String, token: String) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", endpoint);
    println!("{}", token);

    // 根据域名和Token连接服务器
    let mut client = RsLocaldClient::connect(endpoint).await?;

    // let req = Request::new(LoginBody { token: "abc".to_string() });
    // client.login(req).await.unwrap();

    proxy_dispatch(&mut client).await;

    Ok(())

    // 服务器会返回域名，将域名显示出来
}

async fn proxy_dispatch(client: &mut RsLocaldClient<Channel>) {
    let (tx, rx) = channel(128);
    let rs = ReceiverStream::new(rx);

    let response = client
        .listen(rs)
        .await
        .unwrap();

    println!("12312");
    let addr = "127.0.0.1:8000".parse().unwrap();

    let mut resp_stream = response.into_inner();
    while let Some(recived) = resp_stream.next().await {
        let recived = recived.unwrap();

        let socket = TcpSocket::new_v4().unwrap();
        let mut stream = socket.connect(addr).await.unwrap();
        let mut buf = io::BufReader::new(&*recived.data);
        io::copy(&mut buf, &mut stream).await.unwrap(); // 发送请求

        let mut resp = vec![0u8; 0];
        io::copy(&mut stream, &mut resp).await.unwrap(); //接收响应

        // 将Response发送会Server端
        tx.send(ProxyResponse { req_id: recived.req_id, data: resp.to_vec() }).await.unwrap();
        println!("resp: {:?}", String::from_utf8(resp));
    }
}
