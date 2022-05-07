use std::collections::HashMap;
use std::sync::Arc;

use log::{debug, info};
use parking_lot::Mutex;
use tokio::{io, select};
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot};
use tokio::sync::mpsc::{Receiver, Sender};
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
        let u = Url::parse(pl.entrypoint.as_str()).unwrap();
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
                    let conn_txc = conn_tx.clone();
                    let (stream, _) = listener.accept().await.unwrap();
                    tokio::spawn(async move { process(stream, conn_txc).await });
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

async fn process(mut stream: TcpStream, conn_tx: Sender<Connection>) {
    debug!("processing stream from: {:?}", stream.peer_addr());
    // 准备接收Response用的channel, 等待客户端接入
    let (tx, mut rx) = mpsc::channel(128);
    conn_tx.send(Connection { id: random_string(32), tx }).await.unwrap(); // 通知Connection已就绪
    while let Some(xd) = rx.recv().await {
        match xd {
            XData::TX(tx) => {
                // 客户端接入成功，开始接收数据并使用tx转发给客户端
                input_stream_dispatch(&mut stream, tx).await.unwrap();
            }
            XData::Data(data) => {
                debug!("received response: {:?}", data.len());
                if data.eq("EOF".as_bytes()) {
                    break;
                }
                // 接收client发回的数据并Response
                stream.write_all(&*data).await.unwrap();
            }
        }
    }
}

async fn input_stream_dispatch(stream: &mut TcpStream, tx: Sender<Vec<u8>>) -> anyhow::Result<()> {
    debug!("input_stream_dispatch: {:?}", stream.peer_addr());
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