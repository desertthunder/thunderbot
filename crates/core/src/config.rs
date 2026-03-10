use super::{
    Settings,
    error::{BotError, Result},
};
use config::{Config, Environment, File, FileFormat};
use std::path::Path;

const EMBEDDED_DEFAULT_CONFIG: &str = include_str!("../config/default.toml");
const LOCAL_CONFIG_CANDIDATES: [&str; 2] = ["tnbot.toml", "config/default.toml"];

pub fn load_config(config_path: Option<&Path>) -> Result<Settings> {
    let _ = dotenvy::dotenv();

    let mut builder = Config::builder().add_source(File::from_str(EMBEDDED_DEFAULT_CONFIG, FileFormat::Toml));

    builder = builder.set_default("bot.name", "ThunderBot")?;
    builder = builder.set_default("bluesky.pds_host", "https://bsky.social")?;
    builder = builder.set_default("database.path", "./data/thunderbot.db")?;
    builder = builder.set_default("logging.level", "info")?;
    builder = builder.set_default("logging.format", "pretty")?;

    if let Some(path) = config_path {
        builder = builder.add_source(File::from(path).required(true));
    } else {
        for candidate in LOCAL_CONFIG_CANDIDATES {
            if Path::new(candidate).exists() {
                builder = builder.add_source(File::from(Path::new(candidate)).required(false));
                break;
            }
        }
    }

    builder = builder.add_source(Environment::with_prefix("TNBOT").prefix_separator("_").separator("__"));

    let config = builder.build()?;
    let settings: Settings = config.try_deserialize()?;
    validate_settings(&settings)?;

    Ok(settings)
}

pub fn validate_settings(settings: &Settings) -> Result<()> {
    let mut errors = Vec::new();

    if settings.bot.name.trim().is_empty() {
        errors.push("bot.name must not be empty");
    }

    if !settings.bot.did.trim().is_empty() && !settings.bot.did.starts_with("did:") {
        errors.push("bot.did must start with `did:` when provided");
    }

    let pds_host = settings.bluesky.pds_host.trim();
    if !(pds_host.starts_with("https://") || pds_host.starts_with("http://")) {
        errors.push("bluesky.pds_host must start with http:// or https://");
    }

    if settings.database.path.as_os_str().is_empty() {
        errors.push("database.path must not be empty");
    }

    if errors.is_empty() { Ok(()) } else { Err(BotError::Validation(errors.join("; "))) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile;

    #[test]
    fn test_load_config_defaults() {
        let settings = load_config(None).unwrap();
        assert_eq!(settings.bot.name, "ThunderBot");
        assert_eq!(settings.bluesky.pds_host, "https://bsky.social");
    }

    #[test]
    fn test_load_config_from_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("test_config.toml");

        fs::write(
            &config_path,
            r#"
[bot]
name = "TestBot"
did = "did:plc:test123"

[bluesky]
handle = "test.bsky.social"
pds_host = "https://test.bsky.social"
"#,
        )
        .unwrap();

        let settings = load_config(Some(&config_path)).unwrap();
        assert_eq!(settings.bot.name, "TestBot");
        assert_eq!(settings.bot.did, "did:plc:test123");
        assert_eq!(settings.bluesky.handle, "test.bsky.social");
        assert_eq!(settings.bluesky.pds_host, "https://test.bsky.social");
    }

    #[test]
    fn test_config_file_not_found() {
        let result = load_config(Some(Path::new("/nonexistent/path.toml")));
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_settings_rejects_invalid_did() {
        let mut settings = Settings::default();
        settings.bot.did = "not-a-did".to_string();

        let result = validate_settings(&settings);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_settings_rejects_invalid_pds() {
        let mut settings = Settings::default();
        settings.bluesky.pds_host = "ftp://example.org".to_string();

        let result = validate_settings(&settings);
        assert!(result.is_err());
    }
}
