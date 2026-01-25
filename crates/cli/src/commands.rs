use crate::cli::{self, Cli, Commands, JetstreamCommands};

use thunderbot_core::jetstream;

pub async fn run(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        Commands::Jetstream { ref command } => handle_jetstream(command).await,
        Commands::Bsky { ref command } => handle_bsky(command).await,
        Commands::Db { ref command } => handle_db(command).await,
        Commands::Ai { ref command } => handle_ai(command).await,
        Commands::Serve => handle_serve().await,
        Commands::Status => handle_status().await,
        Commands::Config { ref command } => handle_config(command).await,
    }
}

async fn handle_jetstream(command: &JetstreamCommands) -> anyhow::Result<()> {
    match command {
        JetstreamCommands::Listen { filter_did, duration } => {
            tracing::info!("Starting Jetstream listener");
            tracing::info!("Filter DID: {:?}", filter_did);
            tracing::info!("Duration: {:?}", duration);
            jetstream::listen(filter_did.clone(), *duration).await
        }
        JetstreamCommands::Replay { cursor } => {
            tracing::info!("Replaying Jetstream from cursor: {}", cursor);
            jetstream::replay(*cursor).await
        }
    }
}

async fn handle_bsky(command: &cli::BskyCommands) -> anyhow::Result<()> {
    match command {
        cli::BskyCommands::Login => {
            tracing::info!("Login command not yet implemented");
            Ok(())
        }
        cli::BskyCommands::Whoami => {
            tracing::info!("Whoami command not yet implemented");
            Ok(())
        }
        cli::BskyCommands::Post { text } => {
            tracing::info!("Post command not yet implemented: {}", text);
            Ok(())
        }
        cli::BskyCommands::Reply { uri, text } => {
            tracing::info!("Reply command not yet implemented: {} -> {}", uri, text);
            Ok(())
        }
        cli::BskyCommands::Resolve { handle } => {
            tracing::info!("Resolve command not yet implemented: {}", handle);
            Ok(())
        }
        cli::BskyCommands::GetPost { uri } => {
            tracing::info!("GetPost command not yet implemented: {}", uri);
            Ok(())
        }
    }
}

async fn handle_db(command: &cli::DbCommands) -> anyhow::Result<()> {
    use std::sync::Arc;
    use thunderbot_core::{DatabaseRepository, LibsqlRepository, ThreadContextBuilder};

    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "file:bot.db".to_string());
    let repo = Arc::new(LibsqlRepository::new(&db_url).await?);

    match command {
        cli::DbCommands::Migrate => {
            tracing::info!("Running database migrations...");
            repo.run_migration().await?;
            println!("Database migrations completed successfully");
            Ok(())
        }
        cli::DbCommands::Stats => {
            let stats = repo.get_stats().await?;
            println!("Database Statistics:");
            println!("  Conversations: {}", stats.conversation_count);
            println!("  Threads: {}", stats.thread_count);
            println!("  Identities: {}", stats.identity_count);
            Ok(())
        }
        cli::DbCommands::Threads { limit } => {
            let threads = repo.get_all_threads(*limit).await?;
            println!("Recent Threads ({}):", limit);
            for (i, thread) in threads.iter().enumerate() {
                println!("  {}. {}", i + 1, thread);
            }
            Ok(())
        }
        cli::DbCommands::Thread { root_uri } => {
            let context_builder = ThreadContextBuilder::new(repo.clone());
            let context = context_builder.build(root_uri).await?;

            println!("Thread: {}", context.root_uri);
            println!("Messages:");
            for msg in context.messages {
                println!("  [{}] {}: {}", msg.role, msg.author_did, msg.content);
            }
            Ok(())
        }
        cli::DbCommands::Identities => {
            let identities = repo.get_all_identities().await?;
            println!("Cached Identities ({}):", identities.len());
            for identity in identities {
                println!("  {} -> {} ({})", identity.did, identity.handle, identity.last_updated);
            }
            Ok(())
        }
    }
}

async fn handle_ai(command: &cli::AiCommands) -> anyhow::Result<()> {
    match command {
        cli::AiCommands::Prompt { text } => {
            tracing::info!("Prompt command not yet implemented: {}", text);
            Ok(())
        }
        cli::AiCommands::Chat => {
            tracing::info!("Chat command not yet implemented");
            Ok(())
        }
        cli::AiCommands::Context { root_uri } => {
            tracing::info!("Context command not yet implemented: {}", root_uri);
            Ok(())
        }
        cli::AiCommands::Simulate { root_uri } => {
            tracing::info!("Simulate command not yet implemented: {}", root_uri);
            Ok(())
        }
    }
}

async fn handle_serve() -> anyhow::Result<()> {
    tracing::info!("Serve command not yet implemented");
    Ok(())
}

async fn handle_status() -> anyhow::Result<()> {
    tracing::info!("Status command not yet implemented");
    Ok(())
}

async fn handle_config(command: &cli::ConfigCommands) -> anyhow::Result<()> {
    match command {
        cli::ConfigCommands::Show => {
            tracing::info!("Config show command not yet implemented");
            Ok(())
        }
        cli::ConfigCommands::Validate => {
            tracing::info!("Config validate command not yet implemented");
            Ok(())
        }
    }
}
