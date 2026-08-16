use crate::path_glob::PathGlob;
use crate::{debug_log, log};
use serde::de::Error;
use serde::{Deserialize, Deserializer};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub output_folder: Option<PathBuf>,
    pub targets: Vec<TargetConfig>,
    #[serde(default)]
    pub copy_empty_folders: bool,
    #[serde(default)]
    pub follow_symlinks: bool,
    pub cleanup: Option<CleanupConfig>,
    #[serde(deserialize_with = "deserialize_compressed_file_name_pattern")]
    pub compressed_file_name_pattern: String,
    #[serde(default)]
    pub compression: CompressionOptions,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TargetConfig {
    pub path: PathBuf,
    pub archive_path: Option<String>,
    pub include: Option<Vec<PathGlob>>,
    pub exclude: Option<Vec<PathGlob>>,
    pub max_depth: Option<usize>,
    pub min_depth: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CleanupConfig {
    pub keep_last: Option<usize>,
    pub delete_older_than_days: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CompressionOptions {
    #[serde(deserialize_with = "deserialize_compression_method")]
    pub method: CompressionMethod,
    pub level: u8,
}

impl Default for CompressionOptions {
    fn default() -> Self {
        Self {
            method: CompressionMethod::Deflate,
            level: 5,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub enum CompressionMethod {
    Deflate,
    LZMA2,
    PPMd,
}

fn deserialize_compression_method<'de, D>(deserializer: D) -> Result<CompressionMethod, D::Error>
where
    D: Deserializer<'de>,
{
    let pattern = String::deserialize(deserializer)?.to_lowercase();

    match pattern.as_str() {
        "deflate" => Ok(CompressionMethod::Deflate),
        "lzma2" => Ok(CompressionMethod::LZMA2),
        "ppmd" => Ok(CompressionMethod::PPMd),
        _ => Err(Error::custom(
            "compression_method must be one of 'deflate', 'lzma2', or 'ppmd'",
        ))
    }
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
