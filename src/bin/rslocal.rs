use std::error::Error;
use anyhow::anyhow;
use rslocal::client;
use clap::{Parser, Subcommand};
use rslocal::client::{ClientError};

/// A fictional versioning CLI
#[derive(Debug, Parser)]
#[clap(name = "rslocal")]
#[clap(about = "tunnel local ports to public URLs and inspect traffic", long_about = None)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,

    /// config file of rslocal
    #[clap(short, long, default_value_t = String::from("config"))]
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
    match args.command {
        Commands::HTTP { port, subdomain } => {
            let source = config::File::with_name(&args.config);
            let cfg = config::Config::builder().add_source(source).build();
            let ep = cfg.as_ref().unwrap().get_string("endpoint").unwrap();
            let token = cfg.as_ref().unwrap().get_string("token").unwrap();

            if let Err(err) = client::run(ep, token, format!("127.0.0.1:{}", port), subdomain.unwrap_or_default()).await {
                return match err {
                    ClientError::Connect(err) => { Err(anyhow!("{}", err.source().unwrap().to_string())) }
                    ClientError::Status(status) => { Err(anyhow!("{}", status.message())) }
                    ClientError::Other(err) => { Err(err) }
                };
            }
        }
        Commands::TCP { port } => {
            println!("expose the port {}", port);
        }
    }

    Ok(())
}