use std::sync::Arc;
use rslocal::server::Tunnel;

#[tokio::main]
async fn main() {
    env_logger::init();

    let tunnel = Arc::new(Tunnel::new());
    tunnel.start().await;
}
