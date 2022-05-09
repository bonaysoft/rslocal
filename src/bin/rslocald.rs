use clap::{Parser};
use env_logger::Env;
use rslocal::server::{Config, Tunnel};

/// A fictional versioning CLI
#[derive(Debug, Parser)]
#[clap(name = "rslocald")]
#[clap(author, version, about, long_about = None)]
struct Cli {
    /// config file of rslocal
    #[clap(short, long, default_value_t = String::from("rslocald"))]
    config: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Cli::parse();
    let configfile = &args.config;

    let cfg = Config::new(configfile)?;
    let mut env = Env::default().default_filter_or("info");
    if cfg.core.debug {
        env = env.default_filter_or("rslocal=debug");
    }
    env_logger::Builder::from_env(env).init();

    let tunnel = Tunnel::new(cfg);
    tunnel.start().await
}
