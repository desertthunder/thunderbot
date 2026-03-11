use owo_colors::OwoColorize;
use std::path::PathBuf;
use tnbot_core::Settings;
use tnbot_core::bsky::BskyClient;
use tnbot_core::db::connection::DatabaseManager;
use tnbot_core::db::migrations::run_migrations;
use tnbot_core::db::repository::LibsqlRepository;
use tnbot_core::services::IdentityResolver;

fn session_cache_path(settings: &Settings) -> PathBuf {
    let base_dir = settings
        .database
        .path
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base_dir.join("bsky_session.json")
}

fn ensure_credentials(settings: &Settings) -> anyhow::Result<()> {
    if settings.bluesky.handle.trim().is_empty() || settings.bluesky.app_password.trim().is_empty() {
        anyhow::bail!(
            "Bluesky credentials are not configured. Set TNBOT_BLUESKY__HANDLE and TNBOT_BLUESKY__APP_PASSWORD."
        );
    }
    Ok(())
}

async fn build_authenticated_client(settings: &Settings) -> anyhow::Result<(BskyClient, PathBuf)> {
    ensure_credentials(settings)?;
    let client = BskyClient::with_credentials(
        &settings.bluesky.pds_host,
        &settings.bluesky.handle,
        &settings.bluesky.app_password,
    );
    let session_path = session_cache_path(settings);

    if let Err(e) = client.load_session_from_file(&session_path).await {
        tracing::warn!("Failed to load cached session from {}: {}", session_path.display(), e);
    }

    Ok((client, session_path))
}

pub async fn login(settings: &Settings, json_output: bool) -> anyhow::Result<()> {
    if let Err(e) = ensure_credentials(settings) {
        if json_output {
            println!(
                "{}",
                serde_json::to_string(&serde_json::json!({
                    "success": false,
                    "error": e.to_string(),
                }))?
            );
        } else {
            eprintln!("{}", "Bluesky credentials not configured".red().bold());
            eprintln!(
                "Set {} and {} in environment/.env",
                "TNBOT_BLUESKY__HANDLE".cyan(),
                "TNBOT_BLUESKY__APP_PASSWORD".cyan()
            );
        }
        return Ok(());
    }

    let client = BskyClient::new(&settings.bluesky.pds_host);
    let session_path = session_cache_path(settings);

    match client
        .login(&settings.bluesky.handle, &settings.bluesky.app_password)
        .await
    {
        Ok(session) => {
            if let Err(e) = client.save_session_to_file(&session_path).await {
                tracing::warn!("Failed to persist session to {}: {}", session_path.display(), e);
            }

            if json_output {
                println!(
                    "{}",
                    serde_json::to_string(&serde_json::json!({
                        "success": true,
                        "did": session.did,
                        "handle": session.handle,
                        "session_path": session_path,
                    }))?
                );
            } else {
                println!("{}", "Bot account binding successful!".green().bold());
                println!("  {}: {}", "DID".cyan(), session.did);
                println!("  {}: {}", "Handle".cyan(), session.handle);
                println!("  {}: {}", "Session Cache".cyan(), session_path.display());
                println!(
                    "  {}: {} seconds",
                    "Token expires in".cyan(),
                    session.seconds_until_expiry()
                );
            }
        }
        Err(e) => {
            if json_output {
                println!(
                    "{}",
                    serde_json::to_string(&serde_json::json!({
                        "success": false,
                        "error": e.to_string(),
                    }))?
                );
            } else {
                eprintln!("{} {}", "Bot account binding failed:".red(), e);
            }
        }
    }

    Ok(())
}

pub async fn whoami(settings: &Settings, json_output: bool) -> anyhow::Result<()> {
    let (client, session_path) = build_authenticated_client(settings).await?;

    match client.ensure_valid_session().await {
        Ok(session) => {
            if let Err(e) = client.save_session_to_file(&session_path).await {
                tracing::warn!("Failed to persist session to {}: {}", session_path.display(), e);
            }

            if json_output {
                println!(
                    "{}",
                    serde_json::to_string(&serde_json::json!({
                        "did": session.did,
                        "handle": session.handle,
                        "pds": settings.bluesky.pds_host,
                        "session_path": session_path,
                    }))?
                );
            } else {
                println!("{}", "Current Session:".green().bold());
                println!("  {}: {}", "DID".cyan(), session.did);
                println!("  {}: {}", "Handle".cyan(), session.handle);
                println!("  {}: {}", "PDS".cyan(), settings.bluesky.pds_host);
                println!("  {}: {}", "Session Cache".cyan(), session_path.display());
                println!(
                    "  {}: {} seconds",
                    "Token expires in".cyan(),
                    session.seconds_until_expiry()
                );
            }
        }
        Err(e) => {
            if json_output {
                println!(
                    "{}",
                    serde_json::to_string(&serde_json::json!({ "error": e.to_string() }))?
                );
            } else {
                eprintln!("{} {}", "Not authenticated:".red(), e);
                eprintln!("Run {} to authenticate", "tnbot bsky login".cyan());
            }
        }
    }

    Ok(())
}

pub async fn post(settings: &Settings, text: String, json_output: bool) -> anyhow::Result<()> {
    let (client, session_path) = build_authenticated_client(settings).await?;

    match client.create_post(&text).await {
        Ok(response) => {
            if let Err(e) = client.save_session_to_file(&session_path).await {
                tracing::warn!("Failed to persist session to {}: {}", session_path.display(), e);
            }

            if json_output {
                println!(
                    "{}",
                    serde_json::to_string(&serde_json::json!({
                        "success": true,
                        "uri": response.uri,
                        "cid": response.cid,
                    }))?
                );
            } else {
                println!("{}", "Post created successfully!".green().bold());
                println!("  {}: {}", "URI".cyan(), response.uri);
                println!("  {}: {}", "CID".cyan(), response.cid);
            }
        }
        Err(e) => {
            if json_output {
                println!(
                    "{}",
                    serde_json::to_string(&serde_json::json!({
                        "success": false,
                        "error": e.to_string(),
                    }))?
                );
            } else {
                eprintln!("{} {}", "Failed to create post:".red(), e);
            }
        }
    }

    Ok(())
}

pub async fn reply(settings: &Settings, uri: String, text: String, json_output: bool) -> anyhow::Result<()> {
    let (client, session_path) = build_authenticated_client(settings).await?;

    match client.reply_to(&uri, &text).await {
        Ok(response) => {
            if let Err(e) = client.save_session_to_file(&session_path).await {
                tracing::warn!("Failed to persist session to {}: {}", session_path.display(), e);
            }

            if json_output {
                println!(
                    "{}",
                    serde_json::to_string(&serde_json::json!({
                        "success": true,
                        "uri": response.uri,
                        "cid": response.cid,
                    }))?
                );
            } else {
                println!("{}", "Reply created successfully!".green().bold());
                println!("  {}: {}", "URI".cyan(), response.uri);
                println!("  {}: {}", "CID".cyan(), response.cid);
            }
        }
        Err(e) => {
            if json_output {
                println!(
                    "{}",
                    serde_json::to_string(&serde_json::json!({
                        "success": false,
                        "error": e.to_string(),
                    }))?
                );
            } else {
                eprintln!("{} {}", "Failed to create reply:".red(), e);
            }
        }
    }

    Ok(())
}

pub async fn resolve(settings: &Settings, handle: String, json_output: bool) -> anyhow::Result<()> {
    let manager = DatabaseManager::open(&settings.database.path).await?;
    run_migrations(manager.db()).await?;
    let conn = manager.db().connect()?;
    let repo = LibsqlRepository::new(conn);
    let resolver = IdentityResolver::new(repo, settings.bluesky.pds_host.clone());

    match resolver.resolve_handle_to_did(&handle).await {
        Ok(did) => {
            if json_output {
                println!(
                    "{}",
                    serde_json::to_string(&serde_json::json!({ "handle": handle, "did": did }))?
                );
            } else {
                println!("{}", "Handle resolved:".green().bold());
                println!("  {}: {}", "Handle".cyan(), handle);
                println!("  {}: {}", "DID".cyan(), did);
            }
        }
        Err(e) => {
            if json_output {
                println!(
                    "{}",
                    serde_json::to_string(&serde_json::json!({
                        "handle": handle,
                        "error": e.to_string(),
                    }))?
                );
            } else {
                eprintln!("{} {}", format!("Failed to resolve {}:", handle).red(), e);
            }
        }
    }

    Ok(())
}

pub async fn get_post(settings: &Settings, uri: String, json_output: bool) -> anyhow::Result<()> {
    let client = BskyClient::new(&settings.bluesky.pds_host);

    match client.get_record_by_uri(&uri).await {
        Ok(record) => {
            if json_output {
                println!("{}", serde_json::to_string_pretty(&record)?);
            } else {
                println!("{}", "Post Record:".green().bold());
                println!("  {}: {}", "URI".cyan(), record.uri);
                println!("  {}: {}", "CID".cyan(), record.cid);
                if let Some(text) = record.value.get("text").and_then(|v| v.as_str()) {
                    println!("  {}: {}", "Text".cyan(), text);
                }
                if let Some(created_at) = record.value.get("createdAt").and_then(|v| v.as_str()) {
                    println!("  {}: {}", "Created At".cyan(), created_at);
                }
            }
        }
        Err(e) => {
            if json_output {
                println!(
                    "{}",
                    serde_json::to_string(&serde_json::json!({
                        "uri": uri,
                        "error": e.to_string(),
                    }))?
                );
            } else {
                eprintln!("{} {}", format!("Failed to fetch post {}:", uri).red(), e);
            }
        }
    }

    Ok(())
}
