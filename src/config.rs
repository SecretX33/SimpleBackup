use crate::{debug_log, log};
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::de::Error;
use serde::{Deserialize, Deserializer};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub output_folder: PathBuf,
    pub date_format: String,
    #[serde(deserialize_with = "deserialize_compressed_file_name_pattern")]
    pub compressed_file_name_pattern: String,
    pub targets: Vec<TargetConfig>,
    pub cleanup: Option<CleanupConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TargetConfig {
    pub path: PathBuf,
    pub archive_path: Option<String>,
    #[serde(deserialize_with = "deserialize_globset")]
    pub include: Option<GlobSet>,
    #[serde(deserialize_with = "deserialize_globset")]
    pub exclude: Option<GlobSet>,
    pub max_depth: Option<i32>,
    pub min_depth: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CleanupConfig {
    pub keep_last: Option<usize>,
    pub delete_older_than_days: Option<u64>,
}

fn deserialize_compressed_file_name_pattern<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let pattern = String::deserialize(deserializer)?;

    if !pattern.contains("{date}") {
        return Err(Error::custom(
            "compressed_file_name_pattern must contain the {date} placeholder",
        ));
    }
    Ok(pattern)
}

fn deserialize_globset<'de, D>(deserializer: D) -> Result<Option<GlobSet>, D::Error>
where
    D: Deserializer<'de>,
{
    let Some(patterns) = Option::<HashSet<String>>::deserialize(deserializer)? else {
        return Ok(None);
    };

    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(Glob::new(&pattern).map_err(D::Error::custom)?);
    }
    let globset = builder.build().map_err(D::Error::custom)?;

    Ok(Some(globset))
}

#[derive(Debug)]
pub enum ConfigError {
    IO(std::io::Error),
    Parse(serde_json::Error),
    NotFound,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            ConfigError::IO(e) => e.to_string(),
            ConfigError::Parse(e) => e.to_string(),
            ConfigError::NotFound => "Config file not found".to_string(),
        };
        write!(f, "{message}")
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ConfigError::IO(e) => Some(e),
            ConfigError::Parse(e) => Some(e),
            ConfigError::NotFound => None,
        }
    }
}

pub fn read_app_config(config_path: &Path) -> Result<AppConfig, ConfigError> {
    let result = std::fs::read_to_string(config_path);
    let Some(file_contents) = result
        .as_ref()
        .ok()
        .filter(|contents| !contents.trim().is_empty())
    else {
        return match result {
            Ok(_) => {
                debug_log!("Config file is empty");
                Err(ConfigError::NotFound)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                debug_log!("Config file not found");
                Err(ConfigError::NotFound)
            }
            Err(e) => {
                log!("Error reading config file: {}", e);
                Err(ConfigError::IO(e))
            }
        };
    };

    let parsed_config = serde_json::from_str(file_contents).map_err(ConfigError::Parse)?;
    log!("Config loaded successfully");
    Ok(parsed_config)
}
