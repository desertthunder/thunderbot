use clap::Parser;
use owo_colors::OwoColorize;
use tracing::{error, info};

mod cli;
mod commands;

use cli::{Cli, Commands, ConfigAction, JetstreamAction, parse_log_level};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let log_level = parse_log_level(cli.verbose);

    tracing_subscriber::fmt().with_env_filter(log_level).init();

    info!("{} starting up...", "Thunderbot".cyan().bold());

    let settings = match tnbot_core::config::load_config(cli.config.as_deref()) {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to load configuration: {}", e.red());
            std::process::exit(1);
        }
    };

    match cli.command {
        Commands::Serve { dry_run } => commands::serve::run(dry_run).await,
        Commands::Config { action } => match action {
            ConfigAction::Show => commands::config::show_config(&settings, cli.json)?,
            ConfigAction::Validate => commands::config::validate_config(),
        },
        Commands::Jetstream { action } => match action {
            JetstreamAction::Listen { filter_did, duration } => {
                commands::jetstream::listen(filter_did, duration).await;
            }
            JetstreamAction::Replay { cursor, filter_did } => {
                commands::jetstream::replay(cursor, filter_did).await;
            }
        },
        Commands::Status => {
            println!("{}", "Service Status:".green().bold());
            println!("  Status: {}", "Not running".yellow());
        }
        _ => println!("{}", "Command not yet implemented".yellow()),
    }

    Ok(())
}
