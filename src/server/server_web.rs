use std::collections::HashMap;
use std::error::Error;
use std::sync::{mpsc, Mutex};
use lazy_static::lazy_static;
use tokio::io;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{Sender};
use crate::client::api::ProxyRequest;
use crate::server::api::ProxyResponse;

#[derive(Debug)]
pub struct R {
    pub otx: mpsc::Sender<ProxyResponse>,
    pub req: ProxyRequest,
}

// async fn dispatch(req: Vec<u8>) -> Vec<u8> {
//     // 获取input_tx
//     let tx = get_site_host("host".to_string());
//
//     // 发送数据
//     let (otx, orx) = oneshot::channel();
//     let warp_req: R = R { otx, req: ProxyRequest { req_id: "".to_string(), data: req } };
//     tx.send(warp_req).await.unwrap();
//
//     let resp = orx.await.unwrap();
//     resp.data
// }

lazy_static! {
    pub static ref VHOST: Mutex<HashMap<String, Sender<R>>> = Mutex::new(HashMap::new());
}

pub fn get_site_host(host: String) -> Sender<R> {
    VHOST.lock().unwrap().get(host.as_str()).unwrap().clone()
}

pub fn setup_site_host(host: String, otx: Sender<R>) {
    VHOST.lock().unwrap().insert(host, otx);
}


#[tokio::main]
pub async fn webserver() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:8080").await?;

    loop {
        let (stream, _) = listener.accept().await?;
        tokio::spawn(async move {
            if let Err(e) = process(stream).await {
                println!("failed to process connection; error = {}", e);
            }
        });
    }
}

async fn process(mut stream: TcpStream) -> Result<(), Box<dyn Error>> {
    // 获取发送Request的Sender
    let tx = get_site_host("host".to_string());

    // 准备接收Response用的channel
    let (otx, orx) = mpsc::channel();

    // 接收Request并使用tx发送给客户端
    loop {
        // Wait for the socket to be readable
        stream.readable().await?;
        println!("start read");

        // Creating the buffer **after** the `await` prevents it from
        // being stored in the async task.
        let mut buf = vec![0u8; 1024];

        // Try to read data, this may still fail with `WouldBlock`
        // if the readiness event is a false positive.
        match stream.try_read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                println!("read {} bytes", n);

                if n <= 1024 {
                    // todo 设置结束标志位，跳出循环
                }

                let warp_req: R = R { otx: otx.clone(), req: ProxyRequest { req_id: "-1".to_string(), data: buf.to_vec() } };
                tx.send(warp_req).await.unwrap();
                if n <= 1024 {
                    break;
                }
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                println!("WouldBlock");
                continue;
            }
            Err(e) => {
                return Err(e.into());
            }
        }
    }

    // 接收client发回的数据并Response
    for x in orx {
        stream.write(&x.data).await.unwrap();
        if x.req_id == "-1" {
            break;
        }
        println!("{:?}", String::from_utf8(x.data));
    }

    Ok(())
}