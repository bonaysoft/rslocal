use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;

use log::{debug, error, info};
use parking_lot::Mutex;
use futures::FutureExt;
use tokio::{io};
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot};
use tokio::sync::mpsc::{Sender};
use tokio_util::sync::PollSender;
use url::Url;
use crate::{random_string, RxReader, TxWriter};
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
            info!("tcp server listening on {}", addr.clone());
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
                debug!("tcp server terminating");
            }
        }
        });
    }

    fn stop(&self, addr: String) {
        let mut mg = self.listeners.lock();
        let tx = mg.remove(addr.as_str()).unwrap();
        tx.send(()).unwrap();
        info!("tcp server {} closed.", addr);
    }
}

async fn process(stream: TcpStream, conn_tx: Sender<Connection>) {
    info!("processing stream from: {:?}", stream.peer_addr());
    // 准备接收Response用的channel, 等待客户端接入
    let conn_id = random_string(32);
    let (tx, mut rx) = mpsc::channel(128);
    conn_tx.send(Connection { id: conn_id.clone(), tx: tx.clone() }).await.unwrap(); // 通知Connection已就绪
    if let XData::TX(dtx) = rx.recv().await.unwrap() {
        let rx_reader = RxReader { rx };
        let tx_writer = TxWriter { conn_id, tx: PollSender::new(dtx) };
        tokio::spawn(transfer(stream, rx_reader, tx_writer).map(|r| {
            debug!("transfer map: {:?}", r);
            if let Err(e) = r {
                error!("Failed to transfer; error={}", e);
            }
        }));
    }
}

async fn transfer(mut inbound: TcpStream, mut ro: RxReader<XData>, mut wo: TxWriter<Vec<u8>>) -> Result<(), Box<dyn Error>> {
    debug!("transfer");
    let (mut ri, mut wi) = inbound.split();

    // 从socket复制到grpc
    let client_to_server = async {
        debug!("client_to_server");
        io::copy(&mut ri, &mut wo).await?;
        wo.shutdown().await
    };

    // 从grpc复制到socket
    let server_to_client = async {
        debug!("server_to_client");
        io::copy(&mut ro, &mut wi).await?;
        wi.shutdown().await
    };

    tokio::try_join!(client_to_server, server_to_client)?;
    Ok(())
}