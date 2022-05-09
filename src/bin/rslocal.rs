use std::error::Error;
use anyhow::anyhow;
use clap::{Parser, Subcommand};
use env_logger::Env;
use rslocal::client;
use rslocal::client::{ClientError};
use rslocal::server::api::Protocol;

/// A fictional versioning CLI
#[derive(Debug, Parser)]
#[clap(name = "rslocal")]
#[clap(author, version, about, long_about = None)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,

    /// config file of rslocal
    #[clap(short, long, default_value_t = String::from("rslocal"))]
    config: String,

    /// logging level: 'trace', 'debug', 'info', 'warn', 'error'
    #[clap(long, default_value_t = String::from("info"))]
    log_level: String,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// config the client
    Config {},

    /// start an HTTP tunnel
    #[clap(arg_required_else_help = true)]
    Http {
        /// The local port to be exposed
        port: String,
        #[clap(short, long)]
        subdomain: Option<String>,
    },
    /// start a TCP tunnel
    #[clap(arg_required_else_help = true)]
    Tcp {
        /// The local port to be exposed
        port: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Cli::parse();
    if let Commands::Config {} = args.command {
        return client::config::setup();
    }

    let cfg = client::config::load(&args.config)?;
    let env = Env::default().default_filter_or(format!("rslocal={}", &args.log_level));
    env_logger::Builder::from_env(env).init();

    let endpoint = cfg.get_string("endpoint").unwrap();
    let token = cfg.get_string("token").unwrap();
    match args.command {
        Commands::Http { port, subdomain } => {
            let sd = subdomain.unwrap_or_default();
            let target = format!("127.0.0.1:{}", port);
            build_tunnel(endpoint, token, Protocol::Http, target, sd).await
        }
        Commands::Tcp { port } => {
            let target = format!("127.0.0.1:{}", port);
            build_tunnel(endpoint, token, Protocol::Tcp, target, String::default()).await
        }
        _ => { Ok(()) }
    }
}

async fn build_tunnel(endpoint: String, token: String, protocol: Protocol, target: String, subdomain: String) -> anyhow::Result<()> {
    let tunnel = client::Tunnel::connect(endpoint.as_str(), token.as_str()).await;
    if let Err(ClientError::Connect(err)) = tunnel {
        return Err(anyhow!("{}", err.source().unwrap().to_string()));
    }

    let result = tunnel.unwrap().start(protocol, target, subdomain.as_str()).await;
    if let Err(err) = result {
        return match err {
            ClientError::Connect(err) => { Err(anyhow!("{}", err.source().unwrap().to_string())) }
            ClientError::Disconnect(_err) => {
                Err(anyhow!("remote server disconnect"))
                // todo 增加断线重连机制
            }
            ClientError::Status(status) => { Err(anyhow!("{}: {}", status.code(), status.message())) }
            ClientError::Other(err) => { Err(err) }
        };
    }

    anyhow::Ok(result?)
}
