use super::{
    Settings,
    error::{BotError, Result},
};
use config::{Config, Environment, File, FileFormat};
use std::path::{Path, PathBuf};

const EMBEDDED_DEFAULT_CONFIG: &str = include_str!("../config/default.toml");
const LOCAL_CONFIG_CANDIDATES: [&str; 2] = ["tnbot.toml", "config/default.toml"];
const APP_CONFIG_SUBDIR: &str = "thunderbot";
const APP_CONFIG_FILE: &str = "config.toml";
const APP_DATABASE_FILE: &str = "thunderbot.db";

fn os_config_candidates() -> Vec<PathBuf> {
    os_config_candidates_from_env(
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
        std::env::var_os("APPDATA"),
    )
}

fn os_config_candidates_from_env(
    xdg_config_home: Option<std::ffi::OsString>, home: Option<std::ffi::OsString>, appdata: Option<std::ffi::OsString>,
) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(appdata) = appdata {
        candidates.push(PathBuf::from(appdata).join(APP_CONFIG_SUBDIR).join(APP_CONFIG_FILE));
    }

    if let Some(xdg) = xdg_config_home {
        candidates.push(PathBuf::from(xdg).join(APP_CONFIG_SUBDIR).join(APP_CONFIG_FILE));
    } else if let Some(home) = home.clone() {
        candidates.push(
            PathBuf::from(home)
                .join(".config")
                .join(APP_CONFIG_SUBDIR)
                .join(APP_CONFIG_FILE),
        );
    }

    if let Some(home) = home {
        candidates.push(
            PathBuf::from(home)
                .join(".local")
                .join("share")
                .join(APP_CONFIG_SUBDIR)
                .join(APP_CONFIG_FILE),
        );
    }

    candidates
}

fn discover_config_candidates() -> Vec<PathBuf> {
    let mut candidates: Vec<PathBuf> = LOCAL_CONFIG_CANDIDATES.iter().map(PathBuf::from).collect();
    candidates.extend(os_config_candidates());
    candidates
}

pub fn default_database_path() -> PathBuf {
    default_database_path_from_env(
        std::env::var_os("XDG_DATA_HOME"),
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
        std::env::var_os("APPDATA"),
    )
}

fn default_database_path_from_env(
    xdg_data_home: Option<std::ffi::OsString>, xdg_config_home: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>, appdata: Option<std::ffi::OsString>,
) -> PathBuf {
    if let Some(appdata) = appdata {
        return PathBuf::from(appdata).join(APP_CONFIG_SUBDIR).join(APP_DATABASE_FILE);
    }

    if let Some(xdg_data_home) = xdg_data_home {
        return PathBuf::from(xdg_data_home)
            .join(APP_CONFIG_SUBDIR)
            .join(APP_DATABASE_FILE);
    }

    if let Some(home) = home.clone() {
        return PathBuf::from(home)
            .join(".local")
            .join("share")
            .join(APP_CONFIG_SUBDIR)
            .join(APP_DATABASE_FILE);
    }

    if let Some(xdg_config_home) = xdg_config_home {
        return PathBuf::from(xdg_config_home)
            .join(APP_CONFIG_SUBDIR)
            .join(APP_DATABASE_FILE);
    }

    PathBuf::from("./data/thunderbot.db")
}

pub fn load_config(config_path: Option<&Path>) -> Result<Settings> {
    let _ = dotenvy::dotenv();

    let mut builder = Config::builder().add_source(File::from_str(EMBEDDED_DEFAULT_CONFIG, FileFormat::Toml));

    builder = builder.set_default("bot.name", "ThunderBot")?;
    builder = builder.set_default("bluesky.pds_host", "https://bsky.social")?;
    builder = builder.set_default("database.path", default_database_path().to_string_lossy().to_string())?;
    builder = builder.set_default("logging.level", "info")?;
    builder = builder.set_default("logging.format", "pretty")?;

    if let Some(path) = config_path {
        builder = builder.add_source(File::from(path).required(true));
    } else {
        for candidate in discover_config_candidates() {
            if candidate.exists() {
                builder = builder.add_source(File::from(candidate.as_path()).required(false));
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

    if !settings.bot.did.trim().is_empty() && !settings.bot.did.starts_with("did:") {
        errors.push("bot.did must start with `did:` when provided");
    }

    for did in &settings.access.allowed_dids {
        if !did.trim().is_empty() && !did.starts_with("did:") {
            errors.push("access.allowed_dids values must start with `did:`");
            break;
        }
    }

    let pds_host = settings.bluesky.pds_host.trim();
    if !(pds_host.is_empty() || pds_host.starts_with("https://") || pds_host.starts_with("http://")) {
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
        assert!(settings.database.path.ends_with("thunderbot.db"));
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

[access]
allowed_dids = ["did:plc:allowed"]
allowed_handles = ["alice.bsky.social"]
"#,
        )
        .unwrap();

        let settings = load_config(Some(&config_path)).unwrap();
        let expected_bot_name = std::env::var("TNBOT_BOT__NAME").unwrap_or_else(|_| "TestBot".to_string());
        let expected_bot_did = std::env::var("TNBOT_BOT__DID").unwrap_or_else(|_| "did:plc:test123".to_string());
        let expected_handle = std::env::var("TNBOT_BLUESKY__HANDLE").unwrap_or_else(|_| "test.bsky.social".to_string());
        let expected_pds_host =
            std::env::var("TNBOT_BLUESKY__PDS_HOST").unwrap_or_else(|_| "https://test.bsky.social".to_string());

        assert_eq!(settings.bot.name, expected_bot_name);
        assert_eq!(settings.bot.did, expected_bot_did);
        assert_eq!(settings.bluesky.handle, expected_handle);
        assert_eq!(settings.bluesky.pds_host, expected_pds_host);
        assert_eq!(settings.access.allowed_dids, vec!["did:plc:allowed"]);
        assert_eq!(settings.access.allowed_handles, vec!["alice.bsky.social"]);
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

    #[test]
    fn test_validate_settings_allows_empty_pds_host() {
        let mut settings = Settings::default();
        settings.bluesky.pds_host.clear();

        let result = validate_settings(&settings);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_settings_rejects_invalid_allowed_did() {
        let mut settings = Settings::default();
        settings.access.allowed_dids = vec!["alice.bsky.social".to_string()];

        let result = validate_settings(&settings);
        assert!(result.is_err());
    }

    #[test]
    fn test_os_config_candidates_prefers_xdg_and_includes_local_share() {
        let candidates = os_config_candidates_from_env(
            Some("/tmp/xdg".into()),
            Some("/tmp/home".into()),
            Some("/tmp/appdata".into()),
        );

        assert_eq!(candidates[0], PathBuf::from("/tmp/appdata/thunderbot/config.toml"));
        assert_eq!(candidates[1], PathBuf::from("/tmp/xdg/thunderbot/config.toml"));
        assert_eq!(
            candidates[2],
            PathBuf::from("/tmp/home/.local/share/thunderbot/config.toml")
        );
    }

    #[test]
    fn test_os_config_candidates_falls_back_to_dot_config_without_xdg() {
        let candidates = os_config_candidates_from_env(None, Some("/tmp/home".into()), None);

        assert_eq!(candidates[0], PathBuf::from("/tmp/home/.config/thunderbot/config.toml"));
        assert_eq!(
            candidates[1],
            PathBuf::from("/tmp/home/.local/share/thunderbot/config.toml")
        );
    }

    #[test]
    fn test_default_database_path_prefers_appdata() {
        let path = default_database_path_from_env(
            Some("/tmp/xdg-data".into()),
            Some("/tmp/xdg-config".into()),
            Some("/tmp/home".into()),
            Some("/tmp/appdata".into()),
        );

        assert_eq!(path, PathBuf::from("/tmp/appdata/thunderbot/thunderbot.db"));
    }

    #[test]
    fn test_default_database_path_uses_local_share() {
        let path = default_database_path_from_env(None, Some("/tmp/xdg-config".into()), Some("/tmp/home".into()), None);

        assert_eq!(path, PathBuf::from("/tmp/home/.local/share/thunderbot/thunderbot.db"));
    }

    #[test]
    fn test_default_database_path_falls_back_to_xdg_config_then_relative() {
        let path = default_database_path_from_env(None, Some("/tmp/xdg-config".into()), None, None);
        assert_eq!(path, PathBuf::from("/tmp/xdg-config/thunderbot/thunderbot.db"));

        let relative = default_database_path_from_env(None, None, None, None);
        assert_eq!(relative, PathBuf::from("./data/thunderbot.db"));
    }
}
