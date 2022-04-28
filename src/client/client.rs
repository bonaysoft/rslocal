pub mod api {
    tonic::include_proto!("api");
}

use tokio::sync::mpsc::{channel, Receiver};
use tonic::{Request, Response, Status};
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
    let mut resp_stream = response.into_inner();
    while let Some(recived) = resp_stream.next().await {
        let recived = recived.unwrap();
        println!("data: {:?}", String::from_utf8(recived.data));

        // todo 转发请求到本地的一个地址，并拿到Response
        let resp = "HTTP/1.1 200 OK\nServer: Test\nDate:Thu, 28 Apr 2022 10:10:25 GMT";
        println!("resp: {:?}", resp);

        // 将Response发送会Server端
        tx.send(ProxyResponse { req_id: recived.req_id, data: resp.as_bytes().to_vec() }).await.unwrap();
    }
}