use crate::path_glob::PathGlobSet;
use crate::util::{find_common_path_denominator, normalize_path};
use crate::{debug_log, log};
use color_eyre::Result;
use color_eyre::eyre::{Context, bail, eyre};
use serde::de::Error;
use serde::{Deserialize, Deserializer};
use std::path;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
struct RawAppConfig {
    pub output_folder: PathBuf,
    pub sources: Vec<RawSourceConfig>,
    pub follow_symlinks: Option<bool>,
    pub skip_recompression_for_known_formats: Option<bool>,
    pub retention: Option<RetentionConfig>,
    pub archive_name_prefix: Option<String>,
    pub compression: Option<CompressionOptions>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawSourceConfig {
    pub path: String,
    pub path_in_archive: Option<String>,
    pub include: Option<PathGlobSet>,
    pub exclude: Option<PathGlobSet>,
    pub min_depth: Option<usize>,
    pub max_depth: Option<usize>,
    pub follow_symlinks: Option<bool>,
    pub skip_recompression_for_known_formats: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub output_folder: PathBuf,
    pub sources: Vec<SourceConfig>,
    pub retention: Option<RetentionConfig>,
    pub archive_name_prefix: String,
    pub compression: CompressionOptions,
}

#[derive(Debug, Clone)]
pub struct SourceConfig {
    pub path: PathBuf,
    pub path_in_archive: PathBuf,
    pub include: Option<PathGlobSet>,
    pub exclude: Option<PathGlobSet>,
    pub max_depth: Option<usize>,
    pub min_depth: Option<usize>,
    pub follow_symlinks: bool,
    pub skip_recompression_for_known_formats: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RetentionConfig {
    pub keep_last: Option<usize>,
    #[serde(default, deserialize_with = "deserialize_humantime_duration")]
    pub max_age: Option<std::time::Duration>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CompressionOptions {
    #[serde(deserialize_with = "deserialize_compression_algorithm")]
    pub algorithm: CompressionAlgorithm,
    pub level: u8,
}

impl Default for CompressionOptions {
    fn default() -> Self {
        Self {
            algorithm: CompressionAlgorithm::Deflate,
            level: 5,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub enum CompressionAlgorithm {
    Deflate,
    LZMA2,
    PPMd,
}

impl CompressionAlgorithm {
    pub const ALL_EXTENSIONS: [&str; 2] = ["7z", "zip"];

    pub fn extension(&self) -> &'static str {
        match self {
            CompressionAlgorithm::Deflate => "zip",
            CompressionAlgorithm::LZMA2 => "7z",
            CompressionAlgorithm::PPMd => "7z",
        }
    }
}

impl TryFrom<RawAppConfig> for AppConfig {
    type Error = color_eyre::Report;

    fn try_from(value: RawAppConfig) -> Result<Self> {
        if value.sources.is_empty() {
            bail!("Invalid config: 'sources' must not be empty");
        }

        let absolute_source_paths = value
            .sources
            .iter()
            .map(|source| path::absolute(normalize_path(&source.path).as_ref()))
            .collect::<std::io::Result<Vec<_>>>()?;

        let source_paths = absolute_source_paths
            .iter()
            .map(PathBuf::as_path)
            .collect::<Vec<_>>();
        let common_source_denominator = find_common_path_denominator(&source_paths)
            .context("Could not find common denominator for source paths")?;

        let sources: Vec<SourceConfig> = value
            .sources
            .into_iter()
            .zip(absolute_source_paths)
            .map(|(raw_source, absolute_path)| {
                parse_source(
                    raw_source,
                    absolute_path,
                    value.follow_symlinks,
                    value.skip_recompression_for_known_formats,
                    common_source_denominator.as_deref(),
                )
            })
            .collect::<Result<_>>()?;

        if value
            .archive_name_prefix
            .as_ref()
            .is_some_and(|e| e.is_empty())
        {
            bail!("'archive_name_prefix' must not be empty");
        }

        if let Some(level) = value.compression.as_ref().map(|e| e.level)
            && level > 9
        {
            bail!("'compression.level' must be a value between 0 and 9");
        }

        Ok(Self {
            output_folder: value.output_folder,
            sources,
            retention: value.retention.filter(|it| it.max_age.is_some() || it.keep_last.is_some()),
            archive_name_prefix: value
                .archive_name_prefix
                .unwrap_or_else(|| "backup_".to_string()),
            compression: value.compression.unwrap_or_default(),
        })
    }
}

fn parse_source(
    raw_source: RawSourceConfig,
    path: PathBuf,
    global_follow_symlinks: Option<bool>,
    global_skip_recompression_for_known_formats: Option<bool>,
    common_source_denominator: Option<&Path>,
) -> Result<SourceConfig> {
    if !path.is_absolute() {
        bail!("Source path must be absolute");
    }

    let follow_symlinks = raw_source
        .follow_symlinks
        .or(global_follow_symlinks)
        .unwrap_or(false);
    let skip_recompression_for_known_formats = raw_source
        .skip_recompression_for_known_formats
        .or(global_skip_recompression_for_known_formats)
        .unwrap_or(false);

    if let (Some(min_depth), Some(max_depth)) = (raw_source.min_depth, raw_source.max_depth)
        && min_depth > max_depth
    {
        bail!("Invalid config: 'min_depth' must be less than or equal to 'max_depth'");
    }

    let final_path_in_archive = raw_source
        .path_in_archive
        .or_else(|| {
            common_source_denominator
                .map(|common_path| {
                    path.strip_prefix(common_path)
                        .expect("Could not make source path relative to the common denominator")
                        .to_string_lossy()
                        .into_owned()
                })
                .filter(|e| !e.is_empty())
        })
        .unwrap_or_else(|| path.file_name().unwrap().to_string_lossy().into_owned());

    Ok(SourceConfig {
        path,
        path_in_archive: PathBuf::from(final_path_in_archive),
        include: raw_source.include,
        exclude: raw_source.exclude,
        max_depth: raw_source.max_depth,
        min_depth: raw_source.min_depth,
        follow_symlinks,
        skip_recompression_for_known_formats,
    })
}

fn deserialize_compression_algorithm<'de, D>(
    deserializer: D,
) -> core::result::Result<CompressionAlgorithm, D::Error>
where
    D: Deserializer<'de>,
{
    let pattern = String::deserialize(deserializer)?.to_lowercase();

    match pattern.as_str() {
        "deflate" => Ok(CompressionAlgorithm::Deflate),
        "lzma2" => Ok(CompressionAlgorithm::LZMA2),
        "ppmd" => Ok(CompressionAlgorithm::PPMd),
        _ => Err(Error::custom(
            "compression.algorithm must be one of 'deflate', 'lzma2', or 'ppmd'",
        )),
    }
}

fn deserialize_humantime_duration<'de, D>(
    deserializer: D,
) -> core::result::Result<Option<std::time::Duration>, D::Error>
where
    D: Deserializer<'de>,
{
    let pattern = Option::<String>::deserialize(deserializer)?;
    pattern
        .map(|pattern| {
            humantime::parse_duration(&pattern)
                .map_err(|e| Error::custom(eyre!("Error deserializing duration '{pattern}': {e}")))
        })
        .transpose()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_renamed_configuration_fields() {
        let config: RawAppConfig = serde_json::from_str(
            r#"{
                "output_folder": "backups",
                "sources": [{
                    "path": ".",
                    "path_in_archive": "project",
                    "skip_recompression_for_known_formats": false
                }],
                "skip_recompression_for_known_formats": true,
                "archive_name_prefix": "snapshot_",
                "retention": {
                    "keep_last": 5,
                    "max_age": "30 days"
                },
                "compression": {
                    "algorithm": "lzma2",
                    "level": 7
                }
            }"#,
        )
        .unwrap();

        assert_eq!(config.sources.len(), 1);
        assert_eq!(
            config.sources[0].path_in_archive.as_deref(),
            Some("project")
        );
        assert_eq!(
            config.sources[0].skip_recompression_for_known_formats,
            Some(false)
        );
        assert_eq!(config.skip_recompression_for_known_formats, Some(true));
        assert_eq!(config.archive_name_prefix.as_deref(), Some("snapshot_"));
        assert_eq!(config.retention.as_ref().unwrap().keep_last, Some(5));
        assert_eq!(
            config.retention.as_ref().unwrap().max_age,
            Some(std::time::Duration::from_secs(30 * 24 * 60 * 60))
        );
        assert!(matches!(
            config.compression.unwrap().algorithm,
            CompressionAlgorithm::LZMA2
        ));
    }
}
