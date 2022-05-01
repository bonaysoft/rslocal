use std::collections::HashMap;
use std::sync::Mutex;
use anyhow::anyhow;
use lazy_static::lazy_static;
use log::info;
use tokio::{io, sync};
use tokio::sync::{mpsc, oneshot};
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use crate::client::api::ProxyRequest;
use crate::server::api::ProxyResponse;
use crate::server::Config;

#[derive(Debug)]
pub struct R {
    pub otx: mpsc::Sender<ProxyResponse>,
    pub req: ProxyRequest,
}

lazy_static! {
    pub static ref VHOST: Mutex<HashMap<String, mpsc::Sender<R>>> = Mutex::new(HashMap::new());
}

pub fn get_site_host(host: String) -> Option<mpsc::Sender<R>> {
    if let Some(site) = VHOST.lock().unwrap().get(host.as_str()) {
        Some(site.clone())
    } else {
        None
    }
}

pub fn setup_site_host(host: String, otx: mpsc::Sender<R>) -> bool {
    if get_site_host(host.clone()).is_some() {
        return false;
    }

    VHOST.lock().unwrap().insert(host, otx);
    return true;
}

pub fn remove_site_host(host: String) {
    VHOST.lock().unwrap().remove(host.as_str());
}


#[tokio::main]
pub async fn webserver() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = Config::new().unwrap();
    let listener = TcpListener::bind(cfg.http.bind_addr).await?;

    loop {
        let (stream, _) = listener.accept().await?;
        tokio::spawn(async move {
            println!("processing stream from: {:?}", stream.peer_addr());
            if let Err(e) = process(stream).await {
                println!("failed to process connection; error = {}", e);
            }
        });
    }
}

async fn process(mut stream: TcpStream) -> anyhow::Result<()> {
    // 准备接收Response用的channel
    let (otx, mut orx) = mpsc::channel(128);

    // 接收Request并使用tx发送给客户端
    stream_handler(&mut stream, otx).await.unwrap();

    // 接收client发回的数据并Response
    loop {
        let x = orx.recv().await;
        if let Some(pr) = x {
            stream.write_all(&pr.data).await.unwrap();
            if pr.req_id == "-1" {
                break;
            }
        } else {
            return Err(anyhow!("otx closed"));
        }
    }

    // todo 完善访问日志，可能需要先重构这里的代码
    info!("{}", stream.peer_addr().unwrap());

    Ok(())
}

async fn stream_handler(stream: &mut TcpStream, otx: mpsc::Sender<ProxyResponse>) -> anyhow::Result<()> {
    println!("stream_handler");
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

                let mut req = ProxyRequest { req_id: "".to_string(), data: buf.to_vec() };
                if n <= 1024 {
                    // todo 设置结束标志位，跳出循环
                    req.req_id = "-1".to_string();
                }

                // todo 判断是否为http请求，如果是解析buf获取host
                let mut headers = [httparse::EMPTY_HEADER; 64];
                httparse::Request::new(&mut headers).parse(&*buf).unwrap();
                let mut host: String = "".to_string();
                for x in headers {
                    if x.name == "Host" {
                        host = String::from_utf8(x.value.clone().to_vec()).unwrap()
                    }
                }

                let warp_req: R = R { otx: otx.clone(), req: req };
                // 获取发送Request的Sender
                let tx = get_site_host(host);
                if tx.is_none() {
                    stream.write("HTTP/1.1 404 Not Found\nServer: Rslocal\n".as_bytes()).await.unwrap();
                    return Ok(());
                }

                tx.unwrap().send(warp_req).await.unwrap();
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

    Ok(())
}