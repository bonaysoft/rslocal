pub mod api {
    tonic::include_proto!("api");
}

use tonic::{Request, Response, Status};
use tonic::transport::Channel;
use tokio_stream::StreamExt;
use crate::client::api::rs_locald_client::RsLocaldClient;
use crate::client::api::{LoginBody, ProxyResponse};

#[tokio::main]
pub async fn run(endpoint: String, token: String) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", endpoint);
    println!("{}", token);

    // 根据域名和Token连接服务器
    let mut client = RsLocaldClient::connect(endpoint).await?;

    let req = Request::new(LoginBody { token: "abc".to_string() });
    client.login(req).await.unwrap();

    proxy_dispatch(&mut client, 17).await;

    Ok(())

    // 服务器会返回域名，将域名显示出来
}

async fn proxy_dispatch(client: &mut RsLocaldClient<Channel>, num: i32) {
    // client.login(LoginBody{}).await.unwrap();
    let response = client
        .listen(())
        .await
        .unwrap();

    let mut resp_stream = response.into_inner();
    while let Some(recived) = resp_stream.next().await {
        let recived = recived.unwrap();
        let req = Request::new(ProxyResponse { req_id: recived.req_id, data: recived.data });
        client.send_response(req).await.unwrap();
    }
}