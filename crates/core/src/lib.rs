use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub mod ai;
pub mod bsky;
pub mod config;
pub mod db;
pub mod embedding;
pub mod error;
pub mod jetstream;
pub mod processor;
pub mod services;

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
    #[serde(default)]
    pub ai: AiConfig,
    #[serde(default)]
    pub embedding: EmbeddingConfig,
    #[serde(default)]
    pub memory: MemoryConfig,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_ai_base_url")]
    pub base_url: String,
    #[serde(default = "default_ai_model")]
    pub model: String,
    #[serde(default = "default_ai_temperature")]
    pub temperature: f64,
    #[serde(default = "default_ai_max_tokens")]
    pub max_tokens: u32,
}

fn default_ai_base_url() -> String {
    "https://api.z.ai/api/paas/v4".to_string()
}

fn default_ai_model() -> String {
    "glm-5".to_string()
}

fn default_ai_temperature() -> f64 {
    0.7
}

fn default_ai_max_tokens() -> u32 {
    300
}

pub use embedding::EmbeddingConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    #[serde(default = "default_memory_enabled")]
    pub enabled: bool,
    #[serde(default = "default_ttl_days")]
    pub ttl_days: u32,
    #[serde(default = "default_consolidation_ttl_days")]
    pub consolidation_ttl_days: u32,
    #[serde(default = "default_dedup_threshold")]
    pub dedup_threshold: f64,
    #[serde(default = "default_consolidation_delay_hours")]
    pub consolidation_delay_hours: u32,
}

fn default_memory_enabled() -> bool {
    true
}

fn default_ttl_days() -> u32 {
    90
}

fn default_consolidation_ttl_days() -> u32 {
    365
}

fn default_dedup_threshold() -> f64 {
    0.05
}

fn default_consolidation_delay_hours() -> u32 {
    24
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            enabled: default_memory_enabled(),
            ttl_days: default_ttl_days(),
            consolidation_ttl_days: default_consolidation_ttl_days(),
            dedup_threshold: default_dedup_threshold(),
            consolidation_delay_hours: default_consolidation_delay_hours(),
        }
    }
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

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: default_ai_base_url(),
            model: default_ai_model(),
            temperature: default_ai_temperature(),
            max_tokens: default_ai_max_tokens(),
        }
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
