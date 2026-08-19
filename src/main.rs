mod backup;
mod config;
mod file;
mod log_macros;
mod path_glob;
mod util;

use crate::backup::run_backup;
use crate::config::read_app_config;
use crate::file::cleanup_old_backups;
use color_eyre::eyre::{Context, Result};
use std::path;
use std::path::PathBuf;

fn main() -> Result<()> {
    color_eyre::install()?;

    let app_config_path = path::absolute(PathBuf::from(std::env::args().nth(1).expect(
        "No arguments provided, please provide the path to your config file as the first argument",
    )))
    .context(
        "Could not resolve config path into absolute path, please check the path and try again",
    )?;
    let config = read_app_config(app_config_path.as_path())
        .context("Could not read config file, please check if the config is valid and try again")?;

    run_backup(&config);
    cleanup_old_backups(&config);

    Ok(())
}
