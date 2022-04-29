use std::error::Error;
use std::fmt;
use rslocal::client;
use clap::{Args, Parser, Subcommand};
use futures::task::Spawn;
use tonic::codegen::Body;
use tonic::Status;
use rslocal::client::CustomError;

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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Cli::parse();
    println!("{:?}", args.config);

    match args.command {
        Commands::HTTP { port, subdomain } => {
            println!("expose the port {}", port);
            let source = config::File::with_name(&args.config);
            let cfg = config::Config::builder().add_source(source).build();
            let ep = cfg.as_ref().unwrap().get_string("endpoint").unwrap();
            let token = cfg.as_ref().unwrap().get_string("token").unwrap();

            client::run(ep, token, format!("127.0.0.1:{}", port), subdomain.unwrap_or_default())?;
        }
        Commands::TCP { port } => {
            println!("expose the port {}", port);
        }
    }

    Ok(())
}

// client <= config at HOME
// server <= flag and env

// client连接server，输出服务端域名地址
// server接收外部请求，将请求转发给client