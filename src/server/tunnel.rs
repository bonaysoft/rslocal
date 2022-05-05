use std::sync::Arc;

use tokio::sync::mpsc;
use tokio::sync::mpsc::{Sender};
use tonic::transport::Server;
use crate::server::{check_auth, Config, HttpServer, Payload, RSLServer, RSLUser, TcpServer};
use crate::server::api::tunnel_server::TunnelServer;
use crate::server::api::user_server::UserServer;

#[derive(Clone)]
pub struct Tunnel {
    cfg: Config,

    tcp_server: TcpServer,
    http_server: HttpServer,
}

impl Tunnel {
    pub fn new() -> Self {
        let cfg = Config::new().unwrap();
        let http_cfg = cfg.http.clone();
        Tunnel {
            cfg,
            tcp_server: TcpServer::new(),
            http_server: HttpServer::new(http_cfg),
        }
    }

    pub async fn start(self: Arc<Self>) {
        let that = self.clone();
        let tx1 = event_loop(move |msg| {
            that.http_server.event_handler(msg);
        }).await;

        let that2 = self.clone();
        let tx2 = event_loop(move |msg| {
            that2.tcp_server.event_handler(msg)
        }).await;

        self.http_server.start();
        self.tcp_server.start();
        self.run(tx1, tx2).await.unwrap();
    }

    pub async fn run(&self, tx_http: Sender<Payload>, tx_tcp: Sender<Payload>) -> Result<(), Box<dyn std::error::Error>> {
        println!("grpc_server");
        let addr = self.cfg.core.bind_addr.parse()?;
        let user = RSLUser::new(self.cfg.clone());
        let tunnel = RSLServer::new(self.cfg.clone(), tx_tcp, tx_http);

        Server::builder()
            .add_service(UserServer::new(user))
            .add_service(TunnelServer::with_interceptor(tunnel, check_auth))
            .serve(addr)
            .await?;
        Ok(())
    }
}


async fn event_loop<T>(callback: impl Fn(T) + Send + 'static) -> Sender<T>
    where
        T: Send + 'static
{
    let (tx, mut rx) = mpsc::channel(128);
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            callback(msg)
        }
    });
    tx
}
// async fn key_recv<V>(key: String) -> V {
//     let (tx, mut rx) = mpsc::channel(128);
//     VHOST.lock().unwrap().insert(key, tx);
//     rx.recv().await.unwrap()
// }
//
// async fn key_send<V>(key: String, v: V) {
//     let tx = VHOST.lock().unwrap().get(key.as_str()).unwrap();
//     tx.send(v).await;
// }

