mod log_macros;
mod config;

use crate::config::read_app_config;
use color_eyre::eyre::{Context, Result};
use std::path::PathBuf;

fn main() -> Result<()> {
    color_eyre::install()?;

    let app_config_path = PathBuf::from(std::env::args().skip(1).next()
        .expect("No arguments provided, please provide the path to your config file as the first argument"))
        .canonicalize()
        .context("Could not resolve config path into absolute path, please check the path and try again")?;
    let config = read_app_config(app_config_path.as_path())?;

    for target in config.targets {
        todo!("Implement target backup");
    }

    Ok(())
}
