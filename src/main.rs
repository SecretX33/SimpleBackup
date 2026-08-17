mod log_macros;
mod config;
mod file;
mod path_glob;

use std::path;
use crate::config::read_app_config;
use color_eyre::eyre::{Context, Result};
use std::path::PathBuf;
use crate::file::run_backup;

fn main() -> Result<()> {
    color_eyre::install()?;

    let app_config_path = path::absolute(PathBuf::from(std::env::args().skip(1).next()
        .expect("No arguments provided, please provide the path to your config file as the first argument")))
        .context("Could not resolve config path into absolute path, please check the path and try again")?;
    let config = read_app_config(app_config_path.as_path())
        .context("Could not read config file, please check if the config is valid and try again")?;

    run_backup(&config);

    Ok(())
}
