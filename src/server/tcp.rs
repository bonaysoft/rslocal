use log::debug;
use tokio::io;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;
use url::Url;
use crate::random_string;
use crate::server::{Connection, Payload, XData};

#[derive(Clone, Copy)]
pub struct TcpServer {}

impl TcpServer {
    pub fn new() -> Self {
        TcpServer {}
    }
    pub fn event_handler(&self, pl: Payload) {
        debug!("start tcp-server");
        tokio::spawn(async move {
            let u = Url::parse(pl.bind_addr.as_str()).unwrap();
            let mut host = u.host_str().unwrap().to_string();
            if let Some(port) = u.port() {
                host = format!("{}:{}", host, port);
            }

            debug!("TCP Server Listening on {}", host);
            let listener = TcpListener::bind(host).await.unwrap();

            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let plc = pl.clone();
                tokio::spawn(async move {
                    debug!("processing stream from: {:?}", stream.peer_addr());
                    let tx = stream_handler(stream).await; // 准备接收器
                    plc.tx.send(Connection { id: random_string(32), tx }).await.unwrap(); // 通知Connection已就绪
                });
            }
        });
    }

    pub fn start(&self) {}
}

async fn stream_handler(mut stream: TcpStream) -> Sender<XData> {
    // 准备接收Response用的channel, 等待客户端接入
    let (tx, mut rx) = mpsc::channel(128);
    tokio::spawn(async move {
        while let Some(xd) = rx.recv().await {
            match xd {
                XData::TX(tx) => {
                    // 客户端接入成功，开始接收数据并使用tx转发给客户端
                    stream_dispatch(&mut stream, tx).await.unwrap();
                }
                XData::Data(data) => {
                    if data.eq("EOF".as_bytes()) {
                        break;
                    }
                    // 接收client发回的数据并Response
                    println!("response data: {:?}", data);
                    stream.write(data.as_slice()).await.unwrap();
                }
            }
        }
    });
    tx
}

async fn stream_dispatch(stream: &mut TcpStream, tx: Sender<Vec<u8>>) -> anyhow::Result<()> {
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
                tx.send(buf).await.unwrap();
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

    tx.send(vec![]).await.unwrap();
    println!("stream_handler end");
    Ok(())
}