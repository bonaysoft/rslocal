use clap::{Parser, Subcommand};
use rslocal::client;
use rslocal::server::api::Protocol;

/// A fictional versioning CLI
#[derive(Debug, Parser)]
#[clap(name = "rslocal")]
#[clap(about = "tunnel local ports to public URLs and inspect traffic", long_about = None)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,

    /// config file of rslocal
    #[clap(short, long, default_value_t = String::from("rslocal"))]
    config: String,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// start an HTTP tunnel
    #[clap(arg_required_else_help = true)]
    HTTP {
        /// The local port to be exposed
        port: String,
        #[clap(short, long)]
        subdomain: Option<String>,
    },
    /// start a TCP tunnel
    #[clap(arg_required_else_help = true)]
    TCP {
        /// The local port to be exposed
        port: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let args = Cli::parse();
    let source = config::File::with_name(&args.config);
    let cfg = config::Config::builder().add_source(source).build().unwrap();
    let endpoint = cfg.get_string("endpoint").unwrap();
    let token = cfg.get_string("token").unwrap();

    match args.command {
        Commands::HTTP { port, subdomain } => {
            let sd = subdomain.unwrap_or_default();
            let target = format!("127.0.0.1:{}", port);
            let mut tunnel = client::Tunnel::connect(endpoint.as_str(), token.as_str()).await?;
            tunnel.start(Protocol::Http, target, sd.as_str()).await
        }
        Commands::TCP { port } => {
            let target = format!("127.0.0.1:{}", port);
            let mut tunnel = client::Tunnel::connect(endpoint.as_str(), token.as_str()).await?;
            tunnel.start(Protocol::Tcp, target, "").await
        }
    }
}