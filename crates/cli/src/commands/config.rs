use owo_colors::OwoColorize;
use tnbot_core::Settings;

pub fn show_config(settings: &Settings, json: bool) -> anyhow::Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(settings)?);
    } else {
        println!("{}", "Configuration:".green().bold());
        println!("  Bot Name: {}", settings.bot.name.cyan());
        println!("  Bot DID: {}", settings.bot.did.cyan());
        println!("  Bluesky Handle: {}", settings.bluesky.handle.cyan());
        println!("  PDS Host: {}", settings.bluesky.pds_host.cyan());
        println!(
            "  Database Path: {}",
            settings.database.path.display().to_string().cyan()
        );
        println!("  Log Level: {}", settings.logging.level.cyan());
    }
    Ok(())
}

pub fn validate_config(settings: &Settings, json: bool) -> anyhow::Result<()> {
    match tnbot_core::config::validate_settings(settings) {
        Ok(()) => {
            if json {
                println!("{}", serde_json::json!({ "valid": true }));
            } else {
                println!("{}", "Configuration is valid!".green().bold());
            }
            Ok(())
        }
        Err(err) => {
            if json {
                println!("{}", serde_json::json!({ "valid": false, "error": err.to_string() }));
            } else {
                eprintln!("{} {}", "Configuration is invalid:".red().bold(), err);
            }
            Err(anyhow::anyhow!(err))
        }
    }
}
