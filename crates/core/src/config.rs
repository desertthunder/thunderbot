use super::{Settings, error::Result};
use config::{Config, Environment, File};
use std::path::Path;

pub fn load_config(config_path: Option<&Path>) -> Result<Settings> {
    let mut builder = Config::builder();

    builder = builder.set_default("bot.name", "Thunderbot")?;
    // TODO: resolve pds
    builder = builder.set_default("bluesky.pds_host", "https://bsky.social")?;
    builder = builder.set_default("database.path", "./data/thunderbot.db")?;
    builder = builder.set_default("logging.level", "info")?;
    builder = builder.set_default("logging.format", "pretty")?;

    if let Some(path) = config_path {
        builder = builder.add_source(File::from(path).required(true));
    } else if Path::new("config/default.toml").exists() {
        builder = builder.add_source(File::with_name("config/default.toml").required(false));
    }

    builder = builder.add_source(Environment::with_prefix("TNBOT").prefix_separator("_").separator("__"));

    let config = builder.build()?;
    let settings: Settings = config.try_deserialize()?;

    Ok(settings)
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
}
