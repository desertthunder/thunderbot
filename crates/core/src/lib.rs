use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub mod config;
pub mod error;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings {
    #[serde(default)]
    pub bot: BotConfig,
    #[serde(default)]
    pub bluesky: BlueskyConfig,
    #[serde(default)]
    pub database: DatabaseConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotConfig {
    #[serde(default = "default_bot_name")]
    pub name: String,
    #[serde(default)]
    pub did: String,
}

fn default_bot_name() -> String {
    "ThunderBot".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlueskyConfig {
    #[serde(default)]
    pub handle: String,
    #[serde(default)]
    pub app_password: String,
    #[serde(default = "default_pds_host")]
    pub pds_host: String,
}

fn default_pds_host() -> String {
    "https://bsky.social".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    #[serde(default = "default_db_path")]
    pub path: PathBuf,
}

fn default_db_path() -> PathBuf {
    PathBuf::from("./data/thunderbot.db")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default)]
    pub format: LogFormat,
}

fn default_log_level() -> String {
    "info".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum LogFormat {
    #[default]
    Pretty,
    Json,
}

impl Default for BotConfig {
    fn default() -> Self {
        Self { name: default_bot_name(), did: String::new() }
    }
}

impl Default for BlueskyConfig {
    fn default() -> Self {
        Self { handle: String::new(), app_password: String::new(), pds_host: default_pds_host() }
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self { path: default_db_path() }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self { level: default_log_level(), format: LogFormat::default() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_settings() {
        let settings = Settings::default();
        assert_eq!(settings.bot.name, "ThunderBot");
        assert_eq!(settings.bluesky.pds_host, "https://bsky.social");
        assert_eq!(settings.database.path, PathBuf::from("./data/thunderbot.db"));
        assert_eq!(settings.logging.level, "info");
    }

    #[test]
    fn test_log_format_default() {
        let format = LogFormat::default();
        assert!(matches!(format, LogFormat::Pretty));
    }

    #[test]
    fn test_settings_serialization() {
        let settings = Settings::default();
        let json = serde_json::to_string(&settings).unwrap();
        let deserialized: Settings = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.bot.name, settings.bot.name);
        assert_eq!(deserialized.bluesky.pds_host, settings.bluesky.pds_host);
    }

    #[test]
    fn test_bot_config_default() {
        let bot = BotConfig::default();
        assert_eq!(bot.name, "ThunderBot");
        assert_eq!(bot.did, "");
    }

    #[test]
    fn test_bluesky_config_default() {
        let bsky = BlueskyConfig::default();
        assert_eq!(bsky.handle, "");
        assert_eq!(bsky.app_password, "");
        assert_eq!(bsky.pds_host, "https://bsky.social");
    }

    #[test]
    fn test_database_config_default() {
        let db = DatabaseConfig::default();
        assert_eq!(db.path, PathBuf::from("./data/thunderbot.db"));
    }

    #[test]
    fn test_logging_config_default() {
        let logging = LoggingConfig::default();
        assert_eq!(logging.level, "info");
        assert!(matches!(logging.format, LogFormat::Pretty));
    }
}
