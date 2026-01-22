//! Application settings and Telegram configuration.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Telegram API configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfig {
    /// Telegram API ID (obtain from <https://my.telegram.org>).
    pub api_id: i32,

    /// Telegram API hash (obtain from <https://my.telegram.org>).
    pub api_hash: String,

    /// Path to the session file.
    #[serde(default = "default_session_path")]
    pub session_path: PathBuf,
}

fn default_session_path() -> PathBuf {
    PathBuf::from("session.db")
}

impl TelegramConfig {
    /// Creates a new Telegram configuration.
    #[must_use]
    pub fn new(api_id: i32, api_hash: String) -> Self {
        Self {
            api_id,
            api_hash,
            session_path: default_session_path(),
        }
    }

    /// Creates configuration from environment variables.
    ///
    /// Expects `TG_API_ID` and `TG_API_HASH` to be set.
    ///
    /// # Errors
    ///
    /// Returns an error if environment variables are missing or invalid.
    pub fn from_env() -> Result<Self, ConfigError> {
        let api_id: i32 = std::env::var("TG_API_ID")
            .map_err(|_| ConfigError::MissingEnvVar("TG_API_ID"))?
            .parse()
            .map_err(|_| ConfigError::InvalidApiId)?;

        let api_hash = std::env::var("TG_API_HASH")
            .map_err(|_| ConfigError::MissingEnvVar("TG_API_HASH"))?;

        let session_path = std::env::var("TG_SESSION_PATH").map_or_else(|_| default_session_path(), PathBuf::from);

        Ok(Self {
            api_id,
            api_hash,
            session_path,
        })
    }
}

/// Bot-specific settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotSettings {
    /// Path to the descriptions JSON file.
    pub descriptions_path: PathBuf,

    /// Command prefix for bot commands.
    #[serde(default = "default_command_prefix")]
    pub command_prefix: String,

    /// Minimum interval between bio updates in seconds (rate limit protection).
    #[serde(default = "default_min_update_interval")]
    pub min_update_interval_secs: u64,

    /// Log level for the application.
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

fn default_command_prefix() -> String {
    "/description_bot".to_owned()
}

fn default_min_update_interval() -> u64 {
    60 // 1 minute minimum between updates
}

fn default_log_level() -> String {
    "info".to_owned()
}

impl Default for BotSettings {
    fn default() -> Self {
        Self {
            descriptions_path: PathBuf::from("descriptions.json"),
            command_prefix: default_command_prefix(),
            min_update_interval_secs: default_min_update_interval(),
            log_level: default_log_level(),
        }
    }
}

impl BotSettings {
    /// Creates bot settings from environment variables with defaults.
    #[must_use]
    pub fn from_env_with_defaults() -> Self {
        Self {
            descriptions_path: std::env::var("DESCRIPTIONS_PATH").map_or_else(|_| PathBuf::from("descriptions.json"), PathBuf::from),
            command_prefix: std::env::var("COMMAND_PREFIX")
                .unwrap_or_else(|_| default_command_prefix()),
            min_update_interval_secs: std::env::var("MIN_UPDATE_INTERVAL")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or_else(default_min_update_interval),
            log_level: std::env::var("RUST_LOG")
                .unwrap_or_else(|_| default_log_level()),
        }
    }
}

/// Configuration errors.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Missing required environment variable: {0}")]
    MissingEnvVar(&'static str),

    #[error("Invalid API ID format (must be a positive integer)")]
    InvalidApiId,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_settings() {
        let settings = BotSettings::default();
        assert_eq!(settings.command_prefix, "/description_bot");
        assert_eq!(settings.min_update_interval_secs, 60);
    }

    #[test]
    fn test_telegram_config_new() {
        let config = TelegramConfig::new(12345, "abc123".to_owned());
        assert_eq!(config.api_id, 12345);
        assert_eq!(config.api_hash, "abc123");
        assert_eq!(config.session_path, PathBuf::from("session.db"));
    }
}
