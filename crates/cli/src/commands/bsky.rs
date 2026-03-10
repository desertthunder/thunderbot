use owo_colors::OwoColorize;
use tnbot_core::Settings;
use tnbot_core::bsky::BskyClient;

pub async fn login(settings: &Settings, json_output: bool) -> anyhow::Result<()> {
    if settings.bluesky.handle.is_empty() || settings.bluesky.app_password.is_empty() {
        if json_output {
            println!(
                "{{\"success\":false,\"error\":\"Bluesky credentials not configured\",\"hint\":\"Set BSKY_HANDLE and BSKY_APP_PASSWORD in environment or .env\"}}"
            );
        } else {
            eprintln!("{}", "Bluesky credentials not configured".red());
            eprintln!(
                "Set {} and {} in your environment or .env file",
                "BSKY_HANDLE".cyan(),
                "BSKY_APP_PASSWORD".cyan()
            );
        }
        return Ok(());
    }

    let client = BskyClient::new(&settings.bluesky.pds_host);

    match client
        .login(&settings.bluesky.handle, &settings.bluesky.app_password)
        .await
    {
        Ok(session) => {
            if json_output {
                println!(
                    "{{\"success\":true,\"did\":\"{}\",\"handle\":\"{}\"}}",
                    session.did, session.handle
                );
            } else {
                println!("{}", "Login successful!".green().bold());
                println!("  {}: {}", "DID".cyan(), session.did);
                println!("  {}: {}", "Handle".cyan(), session.handle);
                println!(
                    "  {}: {} seconds",
                    "Token expires in".cyan(),
                    session.seconds_until_expiry()
                );
            }
        }
        Err(e) => {
            if json_output {
                println!("{{\"success\":false,\"error\":\"{}\"}}", e);
            } else {
                eprintln!("{} {}", "Login failed:".red(), e);
            }
        }
    }

    Ok(())
}

pub async fn whoami(settings: &Settings, json_output: bool) -> anyhow::Result<()> {
    let client = BskyClient::with_credentials(
        &settings.bluesky.pds_host,
        &settings.bluesky.handle,
        &settings.bluesky.app_password,
    );

    match client.ensure_valid_session().await {
        Ok(session) => {
            if json_output {
                println!(
                    "{{\"did\":\"{}\",\"handle\":\"{}\",\"pds\":\"{}\"}}",
                    session.did, session.handle, settings.bluesky.pds_host
                );
            } else {
                println!("{}", "Current Session:".green().bold());
                println!("  {}: {}", "DID".cyan(), session.did);
                println!("  {}: {}", "Handle".cyan(), session.handle);
                println!("  {}: {}", "PDS".cyan(), settings.bluesky.pds_host);
                println!(
                    "  {}: {} seconds",
                    "Token expires in".cyan(),
                    session.seconds_until_expiry()
                );
            }
        }
        Err(e) => {
            if json_output {
                println!("{{\"error\":\"{}\"}}", e);
            } else {
                eprintln!("{} {}", "Not authenticated:".red(), e);
                eprintln!("Run {} to authenticate", "tnbot bsky login".cyan());
            }
        }
    }

    Ok(())
}

pub async fn post(settings: &Settings, text: String, json_output: bool) -> anyhow::Result<()> {
    let client = BskyClient::with_credentials(
        &settings.bluesky.pds_host,
        &settings.bluesky.handle,
        &settings.bluesky.app_password,
    );

    match client.create_post(&text).await {
        Ok(response) => {
            if json_output {
                println!(
                    "{{\"success\":true,\"uri\":\"{}\",\"cid\":\"{}\"}}",
                    response.uri, response.cid
                );
            } else {
                println!("{}", "Post created successfully!".green().bold());
                println!("  {}: {}", "URI".cyan(), response.uri);
                println!("  {}: {}", "CID".cyan(), response.cid);
            }
        }
        Err(e) => match json_output {
            true => println!("{{\"success\":false,\"error\":\"{}\"}}", e),
            false => eprintln!("{} {}", "Failed to create post:".red(), e),
        },
    }

    Ok(())
}

pub async fn reply(settings: &Settings, uri: String, text: String, json_output: bool) -> anyhow::Result<()> {
    let client = BskyClient::with_credentials(
        &settings.bluesky.pds_host,
        &settings.bluesky.handle,
        &settings.bluesky.app_password,
    );

    match client.reply_to(&uri, &text).await {
        Ok(response) => {
            if json_output {
                println!(
                    "{{\"success\":true,\"uri\":\"{}\",\"cid\":\"{}\"}}",
                    response.uri, response.cid
                );
            } else {
                println!("{}", "Reply created successfully!".green().bold());
                println!("  {}: {}", "URI".cyan(), response.uri);
                println!("  {}: {}", "CID".cyan(), response.cid);
            }
        }
        Err(e) => match json_output {
            true => println!("{{\"success\":false,\"error\":\"{}\"}}", e),
            false => eprintln!("{} {}", "Failed to create reply:".red(), e),
        },
    }

    Ok(())
}

pub async fn resolve(settings: &Settings, handle: String, json_output: bool) -> anyhow::Result<()> {
    let client = BskyClient::new(&settings.bluesky.pds_host);

    match client.resolve_handle(&handle).await {
        Ok(did) => {
            if json_output {
                println!("{{\"handle\":\"{}\",\"did\":\"{}\"}}", handle, did);
            } else {
                println!("{}", "Handle resolved:".green().bold());
                println!("  {}: {}", "Handle".cyan(), handle);
                println!("  {}: {}", "DID".cyan(), did);
            }
        }
        Err(e) => match json_output {
            true => println!("{{\"handle\":\"{}\",\"error\":\"{}\"}}", handle, e),
            false => eprintln!("{} {}", format!("Failed to resolve {}:", handle).red(), e),
        },
    }

    Ok(())
}

pub async fn get_post(settings: &Settings, uri: String, json_output: bool) -> anyhow::Result<()> {
    let client = BskyClient::new(&settings.bluesky.pds_host);

    match client.get_record_by_uri(&uri).await {
        Ok(record) => {
            if json_output {
                println!("{}", serde_json::to_string_pretty(&record.value)?);
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
        Err(e) => match json_output {
            true => println!("{{\"uri\":\"{}\",\"error\":\"{}\"}}", uri, e),
            false => eprintln!("{} {}", format!("Failed to fetch post {}:", uri).red(), e),
        },
    }

    Ok(())
}

use tnbot_core::bsky::Session;

#[async_trait::async_trait]
trait BskyClientExt {
    async fn ensure_valid_session(&self) -> Result<Session, tnbot_core::error::BotError>;
}

#[async_trait::async_trait]
impl BskyClientExt for BskyClient {
    async fn ensure_valid_session(&self) -> Result<Session, tnbot_core::error::BotError> {
        if let Some(session) = self.get_session().await
            && !session.is_expired()
        {
            Ok(session)
        } else {
            Err(tnbot_core::error::BotError::XrpcAuthentication(
                "No active session. Run 'tnbot bsky login' first.".to_string(),
            ))
        }
    }
}
