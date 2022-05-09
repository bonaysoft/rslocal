use std::fs;
use std::path::PathBuf;
use config::Config;
use inquire::Text;
use inquire::validator::StringValidator;

const DEFAULT_CLOUD_ENDPOINT: &str = "https://localtest.rs/entrypoint";

fn default() -> anyhow::Result<PathBuf> {
    let xdg_dirs = xdg::BaseDirectories::with_prefix("rslocal").unwrap();
    let cfg_path = xdg_dirs.place_config_file("config.ini")
        .expect("cannot create configuration directory");
    Ok(cfg_path)
}

pub fn load(name: &str) -> anyhow::Result<Config> {
    let path = default()?;
    let cfg = Config::builder()
        .add_source(config::File::with_name(path.to_str().unwrap()))
        .add_source(config::File::with_name(name).required(false))
        .build()?;
    Ok(cfg)
}

pub fn setup() -> anyhow::Result<()> {
    let cfg_path = default()?;

    let ep = Text::new("server endpoint?").with_default(DEFAULT_CLOUD_ENDPOINT).prompt()?;
    let token = Text::new("authorization token?")
        .with_validator(&|input| { if input.len() == 0 { Err(String::from("token required")) } else { Ok(()) } })
        .with_help_message("Please your server provider")
        .prompt()?;
    let cfg_content = format!("endpoint={}\ntoken={}", ep, token);
    fs::write(&cfg_path, cfg_content)?;
    println!("config saved at {:?}", cfg_path);
    Ok(())
}