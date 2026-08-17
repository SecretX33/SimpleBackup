use crate::path_glob::PathGlobSet;
use crate::{debug_log, log};
use color_eyre::eyre::{bail, eyre};
use color_eyre::Result;
use serde::de::Error;
use serde::{Deserialize, Deserializer};
use std::path;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
struct RawAppConfig {
    pub output_folder: PathBuf,
    pub targets: Vec<RawTargetConfig>,
    pub follow_symlinks: Option<bool>,
    pub cleanup: Option<CleanupConfig>,
    pub compressed_file_name_prefix: Option<String>,
    pub compression: Option<CompressionOptions>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawTargetConfig {
    pub path: PathBuf,
    pub archive_path: Option<String>,
    pub include: Option<PathGlobSet>,
    pub exclude: Option<PathGlobSet>,
    pub min_depth: Option<usize>,
    pub max_depth: Option<usize>,
    pub follow_symlinks: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub output_folder: PathBuf,
    pub targets: Vec<TargetConfig>,
    pub cleanup: Option<CleanupConfig>,
    pub compressed_file_name_prefix: String,
    pub compression: CompressionOptions,
}

#[derive(Debug, Clone)]
pub struct TargetConfig {
    pub path: PathBuf,
    pub archive_path: Option<String>,
    pub include: Option<PathGlobSet>,
    pub exclude: Option<PathGlobSet>,
    pub max_depth: Option<usize>,
    pub min_depth: Option<usize>,
    pub follow_symlinks: bool,
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

impl TryFrom<RawAppConfig> for AppConfig {
    type Error = color_eyre::Report;

    fn try_from(value: RawAppConfig) -> Result<Self> {
        let targets = value.targets.into_iter()
            .map(|raw_target| parse_target(raw_target, value.follow_symlinks))
            .collect::<Result<_>>()?;

        if value.compressed_file_name_prefix.as_ref().is_some_and(|e| e.is_empty()) {
            bail!("'compressed_file_name_prefix' must not be empty");
        }

        Ok(Self {
            output_folder: value.output_folder,
            targets,
            cleanup: value.cleanup,
            compressed_file_name_prefix: value.compressed_file_name_prefix.unwrap_or_else(|| "backup_".to_string()),
            compression: value.compression.unwrap_or_default(),
        })
    }
}

fn parse_target(
    raw_target: RawTargetConfig,
    global_follow_symlinks: Option<bool>,
) -> Result<TargetConfig> {
    let follow_symlinks = raw_target.follow_symlinks.or(global_follow_symlinks).unwrap_or(false);
    if let (Some(min_depth), Some(max_depth)) = (raw_target.min_depth, raw_target.max_depth) {
        if min_depth > max_depth {
            bail!("Invalid config: 'min_depth' must be less than or equal to 'max_depth'");
        }
    }

    Ok(TargetConfig {
        path: path::absolute(&raw_target.path)?,
        archive_path: raw_target.archive_path.clone(),
        include: raw_target.include,
        exclude: raw_target.exclude,
        max_depth: raw_target.max_depth,
        min_depth: raw_target.min_depth,
        follow_symlinks,
    })
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

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error("Error parsing config file: {0:?}")]
    Parse(color_eyre::Report),
    #[error("Config file not found")]
    NotFound,
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

    let parsed_config = serde_json::from_str::<RawAppConfig>(file_contents)
        .map_err(|e| eyre!(e))
        .and_then(|raw_config| raw_config.try_into())
        .map_err(ConfigError::Parse)?;
    log!("Config loaded successfully");
    Ok(parsed_config)
}
