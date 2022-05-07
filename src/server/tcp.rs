use std::collections::HashMap;
use std::sync::Arc;

use log::{debug, info};
use parking_lot::Mutex;
use tokio::{io, select};
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot};
use tokio::sync::mpsc::Sender;
use url::Url;
use crate::random_string;
use crate::server::{Connection, Payload, XData};

#[derive(Clone)]
pub struct TcpServer {
    listeners: Arc<Mutex<HashMap<String, oneshot::Sender<()>>>>,
}

impl TcpServer {
    pub fn new() -> Self {
        TcpServer { listeners: Default::default() }
    }

    pub async fn event_handler(&mut self, pl: Payload) {
        let u = Url::parse(pl.bind_addr.as_str()).unwrap();
        let mut addr = u.host_str().unwrap().to_string();
        if let Some(port) = u.port() {
            addr = format!("{}:{}", addr, port);
        }

        if pl.tx.is_closed() {
            debug!("stop tcp-server");
            self.stop(addr);
            return;
        }

        debug!("start tcp-server");
        self.start(addr, pl.tx).await;
    }

    async fn start(&mut self, addr: String, conn_tx: Sender<Connection>) {
        let (tx, rx) = oneshot::channel();
        self.listeners.lock().insert(addr.clone(), tx); // 存储tx供stop调用

        tokio::spawn(async move {
            info!("TCP Server Listening on {}", addr.clone());
            let listener = TcpListener::bind(addr).await.unwrap();
            tokio::select! {
            _ = async {
                loop {
                    let (stream, _) = listener.accept().await.unwrap();
                    let conn_txc = conn_tx.clone();
                    tokio::spawn(async move {
                        debug!("processing stream from: {:?}", stream.peer_addr());
                        let tx = stream_handler(stream).await; // 准备接收器
                        conn_txc.send(Connection { id: random_string(32), tx }).await.unwrap(); // 通知Connection已就绪
                    });
                }

                // Help the rust type inferencer out
                Ok::<_, io::Error>(())
            } => {}
            _ = rx => {
                info!("TCP Server terminating");
            }
        }
        });
    }

    fn stop(&self, addr: String) {
        let mut mg = self.listeners.lock();
        let tx = mg.remove(addr.as_str()).unwrap();
        tx.send(()).unwrap();
    }
}

async fn stream_handler(mut stream: TcpStream) -> Sender<XData> {
    // 准备接收Response用的channel, 等待客户端接入
    let (tx, mut rx) = mpsc::channel(128);
    tokio::spawn(async move {
        while let Some(xd) = rx.recv().await {
            match xd {
                XData::TX(tx) => {
                    // 客户端接入成功，开始接收数据并使用tx转发给客户端
                    input_stream_dispatch(&mut stream, tx).await.unwrap();
                }
                XData::Data(data) => {
                    if data.eq("EOF".as_bytes()) {
                        break;
                    }
                    // 接收client发回的数据并Response
                    stream.write_all(&*data).await.unwrap();
                }
            }
        }
    });
    tx
}

async fn input_stream_dispatch(stream: &mut TcpStream, tx: Sender<Vec<u8>>) -> anyhow::Result<()> {
    debug!("input_stream_dispatch");
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
                tx.send(buf[..n].to_vec()).await.unwrap();
                if n < 48 * 1024 {
                    break;
                }
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

    debug!("input_stream_dispatch end");
    Ok(())
}