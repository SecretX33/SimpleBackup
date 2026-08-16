use crate::{debug_log, log};
use color_eyre::eyre::Result;
use std::path::Path;

#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AppConfig {
    pub source: Option<String>,
    pub destination: Option<String>,
    pub logs_expanded: Option<bool>,
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
        write!(f, "{}", message)
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ConfigError::IO(e) => Some(e),
            ConfigError::Parse(e) => Some(e),
            _ => None,
        }
    }
}

pub fn read_app_config(config_path: &Path) -> Result<AppConfig, ConfigError> {
    let path = config_path;

    let result = std::fs::read_to_string(path);
    let Some(file_contents) = result.as_ref().ok().filter(|c| !c.trim().is_empty()) else {
        return match result {
            Ok(_) => {
                debug_log!("Config file is empty");
                Err(ConfigError::NotFound)
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    debug_log!("Config file not found");
                    return Err(ConfigError::NotFound);
                }
                log!("Error reading config file: {}", e);
                Err(ConfigError::IO(e))
            }
        }
    };

    let parsed_config = serde_json::from_str::<AppConfig>(file_contents).map_err(ConfigError::Parse)?;
    log!("Config loaded successfully");
    Ok(parsed_config)
}
