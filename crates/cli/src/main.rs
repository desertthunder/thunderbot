use clap::Parser;
use owo_colors::OwoColorize;

mod cli;
mod commands;

use cli::{AiAction, BskyAction, Cli, Commands, ConfigAction, DbAction, JetstreamAction, parse_log_level};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let log_level = parse_log_level(cli.verbose);

    tracing_subscriber::fmt().with_env_filter(log_level).init();

    tracing::info!("Thunderbot starting up...");

    let settings = match tnbot_core::config::load_config(cli.config.as_deref()) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Failed to load configuration: {}", e);
            std::process::exit(1);
        }
    };

    match cli.command {
        Commands::Serve { dry_run } => commands::serve::run(&settings, dry_run).await?,
        Commands::Config { action } => match action {
            ConfigAction::Show => commands::config::show_config(&settings, cli.json)?,
            ConfigAction::Validate => commands::config::validate_config(&settings, cli.json)?,
        },
        Commands::Jetstream { action } => match action {
            JetstreamAction::Listen { filter_did, duration } => {
                commands::jetstream::listen(filter_did, settings.bot.did.clone(), duration).await?
            }
            JetstreamAction::Replay { cursor, filter_did } => {
                commands::jetstream::replay(cursor, filter_did, settings.bot.did.clone()).await?
            }
        },
        Commands::Db { action } => match action {
            DbAction::Migrate => commands::db::migrate(&settings.database.path).await?,
            DbAction::Stats => commands::db::stats(&settings.database.path, cli.json).await?,
            DbAction::Threads => commands::db::threads(&settings.database.path, 20, cli.json).await?,
            DbAction::Thread { root_uri } => commands::db::thread(&settings.database.path, &root_uri, cli.json).await?,
            DbAction::Identities => commands::db::identities(&settings.database.path, cli.json).await?,
        },
        Commands::Bsky { action } => match action {
            BskyAction::Login => commands::bsky::login(&settings, cli.json).await?,
            BskyAction::Whoami => commands::bsky::whoami(&settings, cli.json).await?,
            BskyAction::Post { text } => commands::bsky::post(&settings, text, cli.json).await?,
            BskyAction::Reply { uri, text } => commands::bsky::reply(&settings, uri, text, cli.json).await?,
            BskyAction::Resolve { handle } => commands::bsky::resolve(&settings, handle, cli.json).await?,
            BskyAction::GetPost { uri } => commands::bsky::get_post(&settings, uri, cli.json).await?,
        },
        Commands::Ai { action } => match action {
            AiAction::Prompt { text } => commands::ai::prompt(&settings.ai, text, cli.json).await?,
            AiAction::Request { text, system, base_url, api_key, model, temperature, max_tokens, repeat, delay_ms } => {
                commands::ai::request(
                    &settings.ai,
                    text,
                    system,
                    base_url,
                    api_key,
                    model,
                    temperature,
                    max_tokens,
                    repeat,
                    delay_ms,
                    cli.json,
                )
                .await?
            }
            AiAction::Chat => commands::ai::chat(&settings.ai).await?,
            AiAction::Context { root_uri } => {
                commands::ai::context(&settings.ai, &settings.database.path, root_uri, cli.json).await?
            }
            AiAction::Simulate { root_uri } => {
                commands::ai::simulate(&settings.ai, &settings.database.path, root_uri, cli.json).await?
            }
        },
        Commands::Vector { ref action } => commands::vector::handle(action.clone(), &cli, &settings).await?,
        Commands::Status => println!(
            "{}\n  Status: {}",
            "Service Status:".green().bold(),
            "Not running".yellow()
        ),
    }

    Ok(())
}
