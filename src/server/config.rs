use std::collections::HashMap;
use config::{ConfigError, Environment, File};
use serde_derive::Deserialize;
use log::info;

#[derive(Debug, Clone, Deserialize)]
#[allow(unused)]
pub struct Core {
    pub debug: bool,
    pub bind_addr: String,
    pub auth_method: String,
    pub allow_ports: String,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(unused)]
pub struct HTTPConfig {
    pub bind_addr: String,
    pub default_domain: String,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(unused)]
pub struct Config {
    pub core: Core,
    pub http: HTTPConfig,
    pub tokens: HashMap<String, String>,
}

impl Config {
    pub fn new() -> Result<Self, ConfigError> {
        // let run_mode = env::var("RUN_MODE").unwrap_or_else(|_| "development".into());

        let s = config::Config::builder()
            .add_source(File::with_name("/etc/rslocal/rslocald").required(false))
            .add_source(File::with_name("rslocald").required(false))
            .add_source(Environment::with_prefix("app"))
            .build()?;

        // Now that we're done, let's access our configuration
        info!("debug: {:?}", s.get_bool("core.debug"));
        info!("core_bind_addr: {:?}", s.get::<String>("core.bind_addr"));

        // You can deserialize (and thus freeze) the entire configuration as
        s.try_deserialize()
    }
}