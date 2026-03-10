use anyhow::Result;
use owo_colors::OwoColorize;
use tnbot_core::db::connection::DatabaseManager;
use tnbot_core::db::migrations::run_migrations;
use tnbot_core::db::repository::{ConversationRepository, IdentityRepository, LibsqlRepository};

/// Run database migrations
pub async fn migrate(db_path: &std::path::Path) -> Result<()> {
    println!("{}", "Running database migrations...".cyan().bold());

    let manager = DatabaseManager::open(db_path).await?;
    run_migrations(manager.db()).await?;

    println!("{}", "✓ Migrations completed successfully".green());
    Ok(())
}

/// Show database statistics
pub async fn stats(db_path: &std::path::Path, json_output: bool) -> Result<()> {
    let manager = DatabaseManager::open(db_path).await?;
    let stats = manager.stats().await?;

    if json_output {
        let json = serde_json::json!({
            "path": stats.path,
            "conversations_count": stats.conversations_count,
            "identities_count": stats.identities_count,
            "failed_events_count": stats.failed_events_count,
            "file_size_bytes": stats.file_size_bytes,
            "last_cursor_time_us": stats.last_cursor_time_us,
        });
        println!("{}", serde_json::to_string_pretty(&json)?);
    } else {
        println!("{}", "Database Statistics:".green().bold());
        println!("  Path: {}", stats.path.cyan());
        println!("  Conversations: {}", stats.conversations_count.to_string().yellow());
        println!("  Identities: {}", stats.identities_count.to_string().yellow());
        println!("  Failed Events: {}", stats.failed_events_count.to_string().yellow());
        println!("  File Size: {} bytes", stats.file_size_bytes.to_string().yellow());
        if let Some(cursor) = stats.last_cursor_time_us {
            println!("  Last Cursor: {}", cursor.to_string().yellow());
        }
    }

    Ok(())
}

/// List recent conversation threads
pub async fn threads(db_path: &std::path::Path, limit: i64, json_output: bool) -> Result<()> {
    let manager = DatabaseManager::open(db_path).await?;
    let conn = manager.db().connect()?;
    let repo = LibsqlRepository::new(conn);

    let threads = repo.get_recent_threads(limit).await?;

    if json_output {
        let json = serde_json::json!(threads);
        println!("{}", serde_json::to_string_pretty(&json)?);
    } else {
        println!("{}", format!("Recent Threads (top {}):", limit).green().bold());
        if threads.is_empty() {
            println!("  {}", "No threads found".dimmed());
        } else {
            for (i, (root_uri, _)) in threads.iter().enumerate() {
                println!("  {}. {}", i + 1, root_uri.cyan());
            }
        }
    }

    Ok(())
}

/// Display full thread history
pub async fn thread(db_path: &std::path::Path, root_uri: &str, json_output: bool) -> Result<()> {
    let manager = DatabaseManager::open(db_path).await?;
    let conn = manager.db().connect()?;
    let repo = LibsqlRepository::new(conn);

    let messages = repo.get_thread_by_root(root_uri).await?;

    if json_output {
        let json = serde_json::json!(messages);
        println!("{}", serde_json::to_string_pretty(&json)?);
    } else {
        println!("{}", format!("Thread: {}", root_uri).green().bold());
        println!();

        if messages.is_empty() {
            println!("  {}", "No messages found in thread".dimmed());
        } else {
            for msg in messages {
                let role_str = match msg.role {
                    tnbot_core::db::models::Role::User => msg.author_did.cyan().to_string(),
                    tnbot_core::db::models::Role::Model => "🤖 Bot".blue().to_string(),
                };
                println!("[{}] {}", msg.created_at.dimmed(), role_str);
                println!("  {}", msg.content);
                println!();
            }
        }
    }

    Ok(())
}

/// List cached identity mappings
pub async fn identities(db_path: &std::path::Path, json_output: bool) -> Result<()> {
    let manager = DatabaseManager::open(db_path).await?;
    let conn = manager.db().connect()?;
    let repo = LibsqlRepository::new(conn);

    let identities = repo.list_all().await?;

    if json_output {
        let json = serde_json::json!(identities);
        println!("{}", serde_json::to_string_pretty(&json)?);
    } else {
        println!("{}", "Cached Identities:".green().bold());
        if identities.is_empty() {
            println!("  {}", "No identities cached".dimmed());
        } else {
            for identity in identities {
                let display_info = if let Some(display_name) = &identity.display_name {
                    format!("{} ({})", identity.handle, display_name)
                } else {
                    identity.handle.clone()
                };
                println!(
                    "  {} -> {} (updated: {})",
                    identity.did.cyan(),
                    display_info.yellow(),
                    identity.last_updated.dimmed()
                );
            }
        }
    }

    Ok(())
}
