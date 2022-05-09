use std::collections::HashMap;
use std::{env, fs};
use anyhow::anyhow;
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
    pub fn new(name: &str) -> Result<Self, ConfigError> {
        let s = config::Config::builder()
            .add_source(File::with_name("/etc/rslocal/rslocald"))
            .add_source(File::with_name(name).required(false))
            .add_source(Environment::with_prefix("rslocal"))
            .build()?;
        if s.get_bool("core.debug").is_err() {
            return Err(ConfigError::NotFound("configfile is required".parse().unwrap()));
        }

        // Now that we're done, let's access our configuration
        info!("debug: {:?}", s.get_bool("core.debug"));
        info!("core_bind_addr: {:?}", s.get::<String>("core.bind_addr"));

        // You can deserialize (and thus freeze) the entire configuration as
        s.try_deserialize()
    }
}