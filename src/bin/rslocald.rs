use env_logger::Env;
use rslocal::server::{Config, Tunnel};

#[tokio::main]
async fn main() {
    let cfg = Config::new().unwrap();
    let mut env = Env::default().default_filter_or("info");
    if cfg.core.debug {
        env = env.default_filter_or("rslocal=debug");
    }
    env_logger::Builder::from_env(env).init();

    let tunnel = Tunnel::new(cfg);
    tunnel.start().await;
}
