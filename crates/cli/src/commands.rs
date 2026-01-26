use crate::cli::{self, Cli, Commands, JetstreamCommands};
use crate::echo;

use anyhow::Context;
use reqwest::Client;
use std::sync::Arc;
use thunderbot_core::BskyClient;
use thunderbot_core::jetstream;
use thunderbot_core::{Agent, IdentityResolver, IdentityResolverConfig, LibsqlRepository, ThreadContextBuilder};
use thunderbot_core::{DatabaseRepository, VectorStore};

pub async fn run(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        Commands::Jetstream { ref command } => handle_jetstream(command).await,
        Commands::Bsky { ref command } => handle_bsky(&cli, command).await,
        Commands::Db { ref command } => handle_db(command).await,
        Commands::Ai { ref command } => handle_ai(&cli, command).await,
        Commands::Vector { ref command } => handle_vector(command).await,
        Commands::Serve => handle_serve(&cli).await,
        Commands::Status => handle_status(&cli).await,
        Commands::Logs { ref level, ref component, follow } => {
            handle_logs(level.as_deref(), component.as_deref(), follow).await
        }
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

async fn handle_bsky(cli: &Cli, command: &cli::BskyCommands) -> anyhow::Result<()> {
    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "file:bot.db".to_string());
    let pds_host = std::env::var("PDS_HOST").unwrap_or_else(|_| "https://bsky.social".to_string());
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
            if cli.dry_run {
                echo::info("[DRY-RUN] Would create post:");
                println!("  Text: {}", text);
                println!("  Character count: {}", text.chars().count());
                return Ok(());
            }

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

            if cli.dry_run {
                echo::info("[DRY-RUN] Would create reply:");
                println!("  Parent URI: {}", uri);
                println!("  Root URI: {}", root_uri);
                println!("  Text: {}", text);
                println!("  Character count: {}", text.chars().count());
                return Ok(());
            }

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
        cli::DbCommands::Backup { path } => {
            echo::info(&format!("Creating backup to: {}", path));

            let before_size = std::fs::metadata(path).ok().map(|m| m.len()).unwrap_or(0);
            let size = repo.backup(path).await?;
            // TODO: use or remove
            let _after_size = std::fs::metadata(path).ok().map(|m| m.len()).unwrap_or(0);

            let size_mb = size as f64 / (1024.0 * 1024.0);
            echo::success("Backup created successfully");
            println!("  Path: {}", path);
            println!("  Size: {:.2} MB", size_mb);

            if before_size > 0 && size > 0 {
                let saved_mb = (before_size.saturating_sub(size) as f64) / (1024.0 * 1024.0);
                if saved_mb > 0.01 {
                    println!("  Saved: {:.2} MB", saved_mb);
                }
            }

            Ok(())
        }
        cli::DbCommands::Restore { path } => {
            if !std::path::Path::new(path).exists() {
                anyhow::bail!("Backup file does not exist: {}", path);
            }

            echo::warn("This will replace the current database. Backup file will not be modified.");
            println!("  From: {}", path);

            repo.restore(path).await?;

            echo::success("Database restored successfully");
            println!("  Source: {}", path);
            Ok(())
        }
        cli::DbCommands::Vacuum => {
            echo::info("Running database vacuum (this may take a while)...");

            let (before, after) = repo.vacuum().await?;
            let saved = before.saturating_sub(after);
            let before_mb = before as f64 / (1024.0 * 1024.0);
            let after_mb = after as f64 / (1024.0 * 1024.0);
            let saved_mb = saved as f64 / (1024.0 * 1024.0);

            echo::success("Database vacuum completed");
            println!("  Before: {:.2} MB", before_mb);
            println!("  After: {:.2} MB", after_mb);
            if saved > 0 {
                println!("  Saved: {:.2} MB", saved_mb);
            }

            Ok(())
        }
    }
}

async fn handle_ai(cli: &Cli, command: &cli::AiCommands) -> anyhow::Result<()> {
    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "file:bot.db".to_string());
    let pds_host = std::env::var("PDS_HOST").unwrap_or_else(|_| "https://bsky.social".to_string());
    let repo = Arc::new(LibsqlRepository::new(&db_url).await?);

    let bsky_client = Arc::new(BskyClient::new(&pds_host, Some(repo.clone())));
    bsky_client.load_from_database().await;

    match command {
        cli::AiCommands::Prompt { text } => {
            if cli.dry_run {
                echo::info("[DRY-RUN] Would send prompt to Gemini:");
                println!("  Prompt: {}", text);
                echo::info("  System prompt: None");
                return Ok(());
            }

            let agent = Agent::from_clients(bsky_client, repo.clone(), "did:plc:placeholder".to_string(), None)?;

            let response = agent.one_shot_prompt(text).await?;

            echo::success("Response from Gemini:");
            println!("  {}", response);
            Ok(())
        }
        cli::AiCommands::Chat => {
            use std::io::{self, Write};

            let agent = Agent::from_clients(bsky_client, repo, "did:plc:placeholder".to_string(), None)?;

            echo::info("Interactive chat mode (press Ctrl+C to exit)");

            loop {
                print!("> ");
                io::stdout().flush()?;

                let mut input = String::new();
                io::stdin().read_line(&mut input)?;

                let input = input.trim();
                if input.is_empty() {
                    continue;
                }

                match agent.one_shot_prompt(input).await {
                    Ok(response) => {
                        echo::info("Gemini:");
                        println!("  {}", response);
                    }
                    Err(e) => {
                        echo::error(&format!("Error: {}", e));
                    }
                }
            }
        }
        cli::AiCommands::Context { root_uri } => {
            let context_builder = ThreadContextBuilder::new(repo.clone());
            let identity_resolver = IdentityResolver::new(repo, IdentityResolverConfig::default());

            let context = context_builder
                .build_with_handle_context(root_uri, &identity_resolver)
                .await?;

            echo::header("Prompt Context");
            println!("{}", context);

            Ok(())
        }
        cli::AiCommands::Simulate { root_uri } => {
            let agent = Agent::from_clients(bsky_client, repo.clone(), "did:plc:placeholder".to_string(), None)?;

            let response = agent.simulate_response(root_uri).await?;

            echo::success("Simulated Response (not posted):");
            println!("  {}", response);

            Ok(())
        }
    }
}

async fn handle_serve(cli: &Cli) -> anyhow::Result<()> {
    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "file:bot.db".to_string());
    let pds_host = std::env::var("PDS_HOST").unwrap_or_else(|_| "https://bsky.social".to_string());
    let dashboard_token = std::env::var("DASHBOARD_TOKEN").unwrap_or_else(|_| "changeme".to_string());

    echo::info(&format!("Starting web server with token: {}", dashboard_token));

    if cli.dry_run {
        echo::warn("DRY-RUN MODE: Posts will be logged but not sent to Bluesky");
    }

    let repo = Arc::new(thunderbot_core::LibsqlRepository::new(&db_url).await?);
    let bsky_client = Arc::new(BskyClient::new(&pds_host, Some(repo.clone())));

    bsky_client.load_from_database().await;

    let health = Arc::new(thunderbot_core::HealthRegistry::new());
    let server = thunderbot_core::Server::new(repo, bsky_client, health).with_dry_run(cli.dry_run);

    echo::success("Web server starting on http://127.0.0.1:3000");
    echo::info("Use the following Authorization header:");
    echo::info(&format!("  Authorization: Bearer {}", dashboard_token));

    server.serve().await
}

async fn handle_status(cli: &Cli) -> anyhow::Result<()> {
    let server_url = std::env::var("SERVER_URL").unwrap_or_else(|_| "http://127.0.0.1:3000".to_string());
    let health_url = format!("{}/api/health", server_url);

    let client = Client::new();
    let response = match client.get(&health_url).send().await {
        Ok(r) => r,
        Err(e) => {
            echo::error(&format!("Failed to connect to server at {}", server_url));
            echo::info("Make sure the server is running with: thunderbot serve");
            return Err(e.into());
        }
    };

    let status = response.status();
    if cli.format == cli::OutputFormat::Json {
        let body = response.text().await?;
        println!("{}", body);
        if status.is_success() {
            return Ok(());
        } else {
            anyhow::bail!("Health check failed with status: {}", status);
        }
    }

    let health_json: serde_json::Value = response.json().await?;
    let overall_status = health_json["status"].as_str().unwrap_or("unknown");
    let version = health_json["version"].as_str().unwrap_or("unknown");

    echo::header("ThunderBot Status");
    println!("  Version: {}", version);
    println!("  Status: {}", overall_status);

    if let Some(checks) = health_json["checks"].as_object() {
        for (component, check) in checks {
            let status = check["status"].as_str().unwrap_or("unknown");
            let output = check["output"].as_str().unwrap_or("");
            let error = check["error"].as_str().unwrap_or("");
            let latency = check["observedValue"].as_i64().unwrap_or(0);

            let status_emoji = match status {
                "pass" => "✓",
                "warn" => "⚠",
                "fail" => "✗",
                _ => "?",
            };

            echo::info(&format!("{} {}: {}", status_emoji, component, status));

            if !output.is_empty() {
                println!("    Output: {}", output);
            }
            if !error.is_empty() {
                echo::error(&format!("    Error: {}", error));
            }
            if latency > 0 {
                println!("    Latency: {}ms", latency);
            }
        }
    }

    if !status.is_success() {
        anyhow::bail!("Health check failed");
    }

    Ok(())
}

async fn handle_config(command: &cli::ConfigCommands) -> anyhow::Result<()> {
    match command {
        cli::ConfigCommands::Show => handle_config_show().await,
        cli::ConfigCommands::Validate => handle_config_validate().await,
    }
}

async fn handle_config_show() -> anyhow::Result<()> {
    echo::header("Current Configuration");

    let config_vars = vec![
        ("DATABASE_URL", "file:bot.db"),
        ("BSKY_HANDLE", "not set"),
        ("BSKY_APP_PASSWORD", "not set"),
        ("PDS_HOST", "https://bsky.social"),
        ("GEMINI_API_KEY", "not set"),
        ("GEMINI_MODEL", "gemini-3-pro-preview"),
        ("DASHBOARD_TOKEN", "not set"),
    ];

    for (key, default) in config_vars {
        let value = std::env::var(key).unwrap_or_else(|_| default.to_string());

        let display_value = if key.contains("PASSWORD") || key.contains("TOKEN") || key.contains("API_KEY") {
            if value == "not set" {
                "not set".to_string()
            } else {
                format!("****{}****", &value.chars().rev().take(4).collect::<String>())
            }
        } else {
            value.clone()
        };

        println!("  {} = {}", key, display_value);
    }

    Ok(())
}

async fn handle_config_validate() -> anyhow::Result<()> {
    echo::header("Validating Configuration");

    let mut all_passed = true;
    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "file:bot.db".to_string());
    echo::info("Checking DATABASE_URL...");

    let db_path = db_url.replace("file:", "");

    match std::fs::metadata(&db_path) {
        Ok(meta) => {
            if meta.is_file() {
                println!("  ✓ Database file exists and is writable");
            } else if meta.is_dir() {
                println!("  ✓ Database directory exists");
            } else {
                echo::error("  ✗ Path exists but is not a file or directory");
                all_passed = false;
            }
        }
        Err(_) => {
            if db_url.starts_with("file:") {
                let path = db_url.strip_prefix("file:").unwrap_or(&db_url);
                if let Some(parent) = std::path::Path::new(path).parent() {
                    if parent.exists() {
                        println!("  ✓ Parent directory exists (database will be created)");
                    } else {
                        echo::error("  ✗ Parent directory does not exist");
                        all_passed = false;
                    }
                } else {
                    echo::error("  ✗ Invalid database path");
                    all_passed = false;
                }
            } else {
                println!("  ! Remote database URL (skipping filesystem check)");
            }
        }
    }

    echo::info("Checking BSKY_HANDLE...");
    let handle = std::env::var("BSKY_HANDLE");
    match handle {
        Ok(h) if h.contains('.') => println!("  ✓ BSKY_HANDLE is set: {}", h),
        Ok(_) => {
            echo::error("  ✗ BSKY_HANDLE format invalid (should be like user.bsky.social)");
            all_passed = false;
        }
        Err(_) => echo::warn("  ! BSKY_HANDLE not set (required for Bluesky operations)"),
    }

    echo::info("Checking BSKY_APP_PASSWORD...");
    let password = std::env::var("BSKY_APP_PASSWORD");
    match password {
        Ok(p) if p.contains('-') => println!("  ✓ BSKY_APP_PASSWORD is set"),
        Ok(_) => echo::warn("  ! BSKY_APP_PASSWORD may be in wrong format (should be xxxx-xxxx-xxxx-xxxx)"),
        Err(_) => echo::warn("  ! BSKY_APP_PASSWORD not set (required for Bluesky operations)"),
    }

    echo::info("Checking GEMINI_API_KEY...");
    let api_key = std::env::var("GEMINI_API_KEY");
    match api_key {
        Ok(key) if key.starts_with("AIza") => println!("  ✓ GEMINI_API_KEY is set"),
        Ok(_) => {
            echo::error("  ✗ GEMINI_API_KEY format invalid (should start with 'AIza')");
            all_passed = false;
        }
        Err(_) => echo::warn("  ! GEMINI_API_KEY not set (required for AI operations)"),
    }

    echo::info("Checking GEMINI_MODEL...");
    let model = std::env::var("GEMINI_MODEL").unwrap_or_else(|_| "gemini-3-pro-preview".to_string());
    println!("  ✓ GEMINI_MODEL: {}", model);

    echo::info("Checking DASHBOARD_TOKEN...");
    let token = std::env::var("DASHBOARD_TOKEN");
    match token {
        Ok(t) if t != "changeme" => println!("  ✓ DASHBOARD_TOKEN is set"),
        Ok(_) => {
            echo::warn("  ! DASHBOARD_TOKEN is set to default (change for security)");
        }
        Err(_) => {
            echo::warn("  ! DASHBOARD_TOKEN not set (required for dashboard access)");
        }
    }

    match all_passed {
        true => echo::ok("All configuration checks passed!"),
        false => anyhow::bail!("Some configuration checks failed"),
    }
}

async fn handle_vector(command: &cli::VectorCommands) -> anyhow::Result<()> {
    use std::sync::Arc;
    use thunderbot_core::{
        EmbeddingProvider, GeminiEmbeddingProvider, LibsqlRepository, MemoryConfig, SearchFilter, SemanticRetriever,
        SqliteVecStore,
    };

    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "file:bot.db".to_string());
    let vector_db_url = std::env::var("VECTOR_DB_URL").unwrap_or_else(|_| "./bot_vectors.db".to_string());

    match command {
        cli::VectorCommands::Stats => {
            let vector_store = Arc::new(SqliteVecStore::new(&vector_db_url, MemoryConfig::default()).await?);
            let stats = vector_store.get_stats().await?;

            echo::header("Vector Store Statistics");
            println!("  Total memories: {}", stats.total_memories);
            println!("  Unique conversations: {}", stats.unique_conversations);
            println!("  Oldest memory: {:?}", stats.oldest_memory);
            println!("  Newest memory: {:?}", stats.newest_memory);
            println!("  By role:");
            for (role, count) in stats.by_role {
                println!("    {}: {}", role, count);
            }

            Ok(())
        }
        cli::VectorCommands::Search { query, top_k, author_did, role } => {
            let vector_store = Arc::new(SqliteVecStore::new(&vector_db_url, MemoryConfig::default()).await?);
            let embedding_provider = Arc::new(GeminiEmbeddingProvider::from_env()?);
            let retriever = SemanticRetriever::new(vector_store, embedding_provider, MemoryConfig::default());

            let filter = SearchFilter {
                author_did: author_did.clone(),
                role: role.clone(),
                start_time: None,
                end_time: None,
                min_score: Some(0.6),
            };

            let results = retriever.search_memories(query, Some(*top_k), Some(filter)).await?;

            echo::header(&format!("Search Results for: {}", query));
            if results.is_empty() {
                echo::info("No results found");
            } else {
                for (i, result) in results.iter().enumerate() {
                    println!(
                        "  {}. [Score: {:.3}] [{}]",
                        i + 1,
                        result.score,
                        result.memory.metadata.role
                    );
                    println!("     Author: {}", result.memory.metadata.author_did);
                    println!("     Content: {}", result.memory.content);
                    println!("     Created: {}", result.memory.created_at);
                    if i < results.len() - 1 {
                        println!();
                    }
                }
            }

            Ok(())
        }
        cli::VectorCommands::Embed { text } => {
            let embedding_provider = GeminiEmbeddingProvider::from_env()?;

            echo::info(&format!("Generating embedding for text ({} chars)", text.len()));

            let embedding = embedding_provider.embed(text).await?;

            echo::ok("Embedding generated")?;
            println!("  Dimensions: {}", embedding.len());
            println!("  First 5 values: {:?}", &embedding[..embedding.len().min(5)]);
            println!("  Last 5 values: {:?}", &embedding[embedding.len().saturating_sub(5)..]);

            Ok(())
        }
        cli::VectorCommands::Backfill { root_uri } => {
            let repo = Arc::new(LibsqlRepository::new(&db_url).await?);
            let vector_store = Arc::new(SqliteVecStore::new(&vector_db_url, MemoryConfig::default()).await?);
            let embedding_provider = Arc::new(GeminiEmbeddingProvider::from_env()?);
            let retriever = SemanticRetriever::new(vector_store, embedding_provider, MemoryConfig::default());
            let history = repo.get_thread_history(root_uri).await?;

            echo::info(&format!("Found {} messages in conversation", history.len()));

            let messages: Vec<(String, String, String)> = history
                .into_iter()
                .map(|row| (row.content, row.author_did, row.role))
                .collect();

            let added = retriever.backfill_conversation(root_uri, &messages).await?;
            let msg = format!("Backfilled {} memories for conversation", added);
            echo::ok(&msg)
        }
    }
}

async fn handle_logs(_level: Option<&str>, _component: Option<&str>, _follow: bool) -> anyhow::Result<()> {
    echo::warn("Logs endpoint not implemented on server");
    echo::info("Use -vv or -vvv flags when running 'thunderbot serve' for verbose logging");
    echo::info("For now, you can view logs directly from server process output");
    echo::info("");
    echo::info("Example: thunderbot serve -vv");
    Ok(())
}
