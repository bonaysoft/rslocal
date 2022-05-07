use std::sync::Arc;
use rslocal::server::Tunnel;

#[tokio::main]
async fn main() {
    env_logger::init();

    let tunnel = Tunnel::new();
    tunnel.start().await;
}
