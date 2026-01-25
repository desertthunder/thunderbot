use crate::cli::{self, Cli, Commands, JetstreamCommands};
use crate::echo;
use anyhow::Context;

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
    use std::sync::Arc;
    use thunderbot_core::BskyClient;

    let pds_host = std::env::var("PDS_HOST").unwrap_or_else(|_| "https://bsky.social".to_string());
    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "file:bot.db".to_string());
    let repo = Arc::new(thunderbot_core::LibsqlRepository::new(&db_url).await?);

    let client = BskyClient::new(&pds_host, Some(repo));

    client.load_from_database().await;

    match command {
        cli::BskyCommands::Login => {
            let handle = std::env::var("BSKY_HANDLE").context("BSKY_HANDLE not set")?;
            let password = std::env::var("BSKY_APP_PASSWORD").context("BSKY_APP_PASSWORD not set")?;

            let session = client.create_session(&handle, &password).await?;

            echo::success("Successfully logged in!");
            println!("  Handle: {}", session.handle);
            println!("  DID: {}", session.did);
            Ok(())
        }
        cli::BskyCommands::Whoami => {
            let session = client
                .get_session()
                .await
                .ok_or_else(|| anyhow::anyhow!("No active session. Run 'bsky login' first."))?;

            echo::header("Current session");
            println!("  Handle: {}", session.handle);
            println!("  DID: {}", session.did);
            Ok(())
        }
        cli::BskyCommands::Post { text } => {
            let result = client.create_post(text).await?;

            echo::success("Post created successfully!");
            println!("  URI: {}", result.uri);
            println!("  CID: {}", result.cid);
            Ok(())
        }
        cli::BskyCommands::Reply { uri, text } => {
            let post = client.get_post(uri).await?;
            let post_record = post.value;

            let parent_cid = post_record
                .get("cid")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Post missing cid"))?;

            let root_uri = post_record
                .get("reply")
                .and_then(|r| r.get("root"))
                .and_then(|r| r.get("uri"))
                .and_then(|u| u.as_str())
                .unwrap_or(uri);

            let root_cid = post_record
                .get("reply")
                .and_then(|r| r.get("root"))
                .and_then(|r| r.get("cid"))
                .and_then(|c| c.as_str())
                .unwrap_or(parent_cid);

            let result = client.reply_to_post(text, uri, parent_cid, root_uri, root_cid).await?;

            echo::success("Reply created successfully!");
            println!("  URI: {}", result.uri);
            println!("  CID: {}", result.cid);
            Ok(())
        }
        cli::BskyCommands::Resolve { handle } => {
            let did = client.resolve_handle(handle).await?;

            echo::success("Resolved handle");
            println!("  {} -> {}", handle, did);
            Ok(())
        }
        cli::BskyCommands::GetPost { uri } => {
            let post = client.get_post(uri).await?;
            let post_record = post.value;

            let text = post_record.get("text").and_then(|t| t.as_str()).unwrap_or("");

            let author_did = post_record
                .get("author")
                .and_then(|a| a.get("did"))
                .and_then(|d| d.as_str())
                .unwrap_or("unknown");

            let created_at = post_record.get("createdAt").and_then(|t| t.as_str()).unwrap_or("");

            echo::header("Post details");
            println!("  URI: {}", post.uri);
            println!("  CID: {}", post.cid);
            println!("  Author DID: {}", author_did);
            println!("  Created at: {}", created_at);
            println!("  Text: {}", text);

            if let Some(reply) = post_record.get("reply")
                && let Some(parent) = reply.get("parent")
                && let Some(parent_uri) = parent.get("uri").and_then(|u| u.as_str())
            {
                println!("  Reply to: {}", parent_uri);
            }

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
            echo::success("Database migrations completed successfully");
            Ok(())
        }
        cli::DbCommands::Stats => {
            let stats = repo.get_stats().await?;
            echo::header("Database Statistics");
            println!("  Conversations: {}", stats.conversation_count);
            println!("  Threads: {}", stats.thread_count);
            println!("  Identities: {}", stats.identity_count);
            Ok(())
        }
        cli::DbCommands::Threads { limit } => {
            let threads = repo.get_all_threads(*limit).await?;
            echo::info(&format!("Recent Threads ({}):", limit));
            for (i, thread) in threads.iter().enumerate() {
                println!("  {}. {}", i + 1, thread);
            }
            Ok(())
        }
        cli::DbCommands::Thread { root_uri } => {
            let context_builder = ThreadContextBuilder::new(repo.clone());
            let context = context_builder.build(root_uri).await?;

            echo::header(&format!("Thread: {}", context.root_uri));
            echo::info("Messages:");
            for msg in context.messages {
                println!("  [{}] {}: {}", msg.role, msg.author_did, msg.content);
            }
            Ok(())
        }
        cli::DbCommands::Identities => {
            let identities = repo.get_all_identities().await?;
            echo::info(&format!("Cached Identities ({}):", identities.len()));
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
            echo::warn(&format!("Prompt command not yet implemented: {}", text));
            Ok(())
        }
        cli::AiCommands::Chat => {
            echo::warn("Chat command not yet implemented");
            Ok(())
        }
        cli::AiCommands::Context { root_uri } => {
            echo::warn(&format!("Context command not yet implemented: {}", root_uri));
            Ok(())
        }
        cli::AiCommands::Simulate { root_uri } => {
            echo::warn(&format!("Simulate command not yet implemented: {}", root_uri));
            Ok(())
        }
    }
}

async fn handle_serve() -> anyhow::Result<()> {
    echo::warn("Serve command not yet implemented");
    Ok(())
}

async fn handle_status() -> anyhow::Result<()> {
    echo::warn("Status command not yet implemented");
    Ok(())
}

async fn handle_config(command: &cli::ConfigCommands) -> anyhow::Result<()> {
    match command {
        cli::ConfigCommands::Show => {
            echo::warn("Config show command not yet implemented");
            Ok(())
        }
        cli::ConfigCommands::Validate => {
            echo::warn("Config validate command not yet implemented");
            Ok(())
        }
    }
}
