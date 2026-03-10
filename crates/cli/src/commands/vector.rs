use crate::cli::{Cli, VectorAction};
use owo_colors::OwoColorize;
use std::sync::Arc;
use tnbot_core::Settings;
use tnbot_core::ai::{Glm5Client, Glm5Config, Message};
use tnbot_core::db::connection::DatabaseManager;
use tnbot_core::db::migrations::run_migrations;
use tnbot_core::db::models::{Conversation, CreateMemoryParams, MemorySearchFilters, SearchSource};
use tnbot_core::db::repository::{ConversationRepository, LibsqlRepository, MemoryRepository};
use tnbot_core::embedding::{EmbeddingPipeline, EmbeddingPipelineConfig};
use tnbot_core::services::{MemoryRetriever, MemoryRetrieverConfig};

pub async fn handle(action: VectorAction, _cli: &Cli, settings: &Settings) -> anyhow::Result<()> {
    match action {
        VectorAction::Stats => handle_stats(settings).await,
        VectorAction::Search { query, top_k, author } => handle_search(settings, query, top_k, author).await,
        VectorAction::Embed { text } => handle_embed(settings, text).await,
        VectorAction::Backfill { batch_size } => handle_backfill(settings, batch_size).await,
        VectorAction::Consolidate => handle_consolidate(settings).await,
        VectorAction::Expire => handle_expire(settings).await,
    }
}

async fn handle_stats(settings: &Settings) -> anyhow::Result<()> {
    let db_manager = DatabaseManager::open(&settings.database.path).await?;
    run_migrations(db_manager.db()).await?;

    let conn = db_manager.db().connect()?;
    let repo = LibsqlRepository::new(conn);

    let memory_count = repo.count_memories().await?;
    let vector_bytes = query_i64(
        repo.conn(),
        "SELECT COALESCE(SUM(length(embedding)), 0) FROM memories",
        (),
    )
    .await?;

    let page_count = query_i64(repo.conn(), "PRAGMA page_count", ()).await?;
    let page_size = query_i64(repo.conn(), "PRAGMA page_size", ()).await?;
    let db_size_bytes = page_count.saturating_mul(page_size);
    let vector_index_bytes = query_i64(
        repo.conn(),
        "SELECT COALESCE(SUM(pgsize), 0) FROM dbstat WHERE name = 'libsql_vector_idx'",
        (),
    )
    .await
    .unwrap_or(vector_bytes);

    let mut rows = repo
        .conn()
        .query(
            "SELECT
                COUNT(*) as total_jobs,
                SUM(CASE WHEN status = 'pending' THEN 1 ELSE 0 END) as pending_jobs,
                SUM(CASE WHEN status = 'complete' THEN 1 ELSE 0 END) as complete_jobs,
                SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END) as failed_jobs
             FROM embedding_jobs",
            (),
        )
        .await?;

    let mut job_stats = (0i64, 0i64, 0i64, 0i64);
    if let Ok(Some(row)) = rows.next().await {
        job_stats.0 = row.get::<i64>(0).unwrap_or(0);
        job_stats.1 = row.get::<i64>(1).unwrap_or(0);
        job_stats.2 = row.get::<i64>(2).unwrap_or(0);
        job_stats.3 = row.get::<i64>(3).unwrap_or(0);
    }

    println!("{}", "Vector Store Statistics".bold().green());
    println!("  Memories:       {}", memory_count.to_string().cyan());
    println!("  Vector bytes:   {}", format_bytes(vector_bytes as u64).cyan());
    println!("  Vector index:   {}", format_bytes(vector_index_bytes as u64).cyan());
    println!("  DB size:        {}", format_bytes(db_size_bytes as u64).cyan());
    println!("\n{}", "Embedding Jobs".bold());
    println!("  Total:          {}", job_stats.0.to_string().cyan());
    println!("  Pending:        {}", job_stats.1.to_string().yellow());
    println!("  Complete:       {}", job_stats.2.to_string().green());
    println!("  Failed:         {}", job_stats.3.to_string().red());

    Ok(())
}

async fn handle_search(
    settings: &Settings, query: String, top_k: Option<usize>, author: Option<String>,
) -> anyhow::Result<()> {
    let db_manager = DatabaseManager::open(&settings.database.path).await?;
    run_migrations(db_manager.db()).await?;

    let conn = db_manager.db().connect()?;
    let repo = LibsqlRepository::new(conn);

    let retriever = MemoryRetriever::new(
        repo,
        Arc::from(settings.embedding.create_provider()),
        MemoryRetrieverConfig::default(),
    );

    let limit = top_k.unwrap_or(5);
    let filters = MemorySearchFilters { author_did: author, ..Default::default() };

    println!("{} {}", "Running hybrid search for:".dimmed(), query);
    println!("{} {}", "Top-k:".dimmed(), limit);

    let results = retriever.retrieve_hybrid(&query, filters, Some(limit)).await?;

    println!("\n{}", "Search Results".bold().green());
    println!("  Found {} memories\n", results.len());

    for (idx, result) in results.iter().enumerate() {
        let source = match result.source {
            SearchSource::Semantic => "semantic".cyan().to_string(),
            SearchSource::Keyword => "keyword".yellow().to_string(),
            SearchSource::Hybrid => "hybrid".green().to_string(),
        };
        println!(
            "  {}. Score: {:.4} [{}]",
            (idx + 1).to_string().cyan(),
            result.score,
            source
        );
        println!("     Content: {}", result.memory.content);
        println!("     Root:    {}", result.memory.root_uri.dimmed());
        println!("     Author:  {}", result.memory.author_did.dimmed());
        println!();
    }

    Ok(())
}

async fn handle_embed(settings: &Settings, text: String) -> anyhow::Result<()> {
    let provider = settings.embedding.create_provider();

    println!("{} {}", "Generating embedding for:".dimmed(), text);
    let embedding = provider.embed(&text).await?;

    println!("\n{}", "Embedding Vector".bold().green());
    println!("  Dimensions: {}", embedding.len());
    println!("\n  First 10 dimensions:");
    for (i, value) in embedding.iter().take(10).enumerate() {
        print!("    [{:3}]: {:>10.6}", i, value);
        if (i + 1) % 2 == 0 {
            println!();
        }
    }
    if embedding.len() > 10 {
        println!("    ... and {} more dimensions", embedding.len() - 10);
    }

    Ok(())
}

async fn handle_backfill(settings: &Settings, batch_size: Option<usize>) -> anyhow::Result<()> {
    let db_manager = DatabaseManager::open(&settings.database.path).await?;
    run_migrations(db_manager.db()).await?;

    let conn = db_manager.db().connect()?;
    let repo = Arc::new(LibsqlRepository::new(conn));

    let provider = Arc::from(settings.embedding.create_provider());
    let config = EmbeddingPipelineConfig {
        poll_interval_secs: 30,
        batch_size: settings.embedding.batch_size,
        dedup_threshold: settings.memory.dedup_threshold,
        max_retries: 3,
        post_ttl_days: settings.memory.ttl_days,
    };
    let (pipeline, _rx) = EmbeddingPipeline::new(repo.clone(), provider, config);

    println!("{}", "Vector Backfill".bold().green());
    let created = pipeline.backfill(batch_size).await?;
    println!("  Created {} pending embedding jobs", created.to_string().cyan());

    let mut completed_total = 0usize;
    loop {
        let completed = pipeline.process_pending_once().await?;
        if completed == 0 {
            break;
        }
        completed_total += completed;
        println!("  Processed {} jobs...", completed);
    }

    println!(
        "\n  {} Backfill complete (created {}, processed {})",
        "✓".green(),
        created,
        completed_total
    );
    Ok(())
}

async fn handle_consolidate(settings: &Settings) -> anyhow::Result<()> {
    let db_manager = DatabaseManager::open(&settings.database.path).await?;
    run_migrations(db_manager.db()).await?;

    let conn = db_manager.db().connect()?;
    let repo = LibsqlRepository::new(conn);
    let provider = settings.embedding.create_provider();
    let ai_client = maybe_create_ai_client(settings);

    println!("{}", "Consolidate Memories".bold().green());

    let cutoff_modifier = format!("-{} hours", settings.memory.consolidation_delay_hours);
    let mut stale_roots_rows = repo
        .conn()
        .query(
            "SELECT root_uri
             FROM conversations
             GROUP BY root_uri
             HAVING julianday(MAX(created_at)) <= julianday('now', ?1)
             ORDER BY MAX(created_at) ASC",
            [cutoff_modifier.as_str()],
        )
        .await?;

    let mut stale_roots = Vec::new();
    while let Ok(Some(row)) = stale_roots_rows.next().await {
        let root_uri: String = row.get(0)?;
        stale_roots.push(root_uri);
    }

    if stale_roots.is_empty() {
        println!("  {} No stale threads eligible for consolidation", "✓".green());
        return Ok(());
    }

    let mut consolidated = 0usize;
    let mut deleted = 0u64;

    for root_uri in stale_roots {
        let existing_summary = query_i64(
            repo.conn(),
            "SELECT COUNT(*) FROM memories
             WHERE root_uri = ?1
               AND json_extract(metadata, '$.kind') = 'summary'",
            [root_uri.as_str()],
        )
        .await?;

        if existing_summary > 0 {
            continue;
        }

        let thread = repo.get_thread_by_root(&root_uri).await?;
        if thread.len() < 2 {
            continue;
        }

        let summary = summarize_thread(&thread, ai_client.as_ref()).await;
        let embedding = provider.embed(&summary).await?;

        let last = match thread.last() {
            Some(v) => v,
            None => continue,
        };
        let expires_at = add_days_sql(repo.conn(), &last.created_at, settings.memory.consolidation_ttl_days).await?;
        let metadata = serde_json::json!({
            "kind": "summary",
            "consolidated_from_messages": thread.len(),
        });

        let summary_id = repo
            .create_memory_with_params(CreateMemoryParams {
                conversation_id: last.id,
                root_uri: root_uri.clone(),
                content: summary,
                embedding,
                author_did: settings.bot.did.clone(),
                metadata: Some(metadata),
                created_at: last.created_at.clone(),
                expires_at: Some(expires_at),
                content_hash: None,
            })
            .await?;

        let deleted_count = repo
            .conn()
            .execute(
                "DELETE FROM memories
                 WHERE root_uri = ?1 AND id != ?2",
                (root_uri.as_str(), summary_id),
            )
            .await? as u64;

        consolidated += 1;
        deleted += deleted_count;
    }

    println!(
        "  {} Consolidated {} thread(s), removed {} post-level memories",
        "✓".green(),
        consolidated.to_string().cyan(),
        deleted.to_string().cyan()
    );
    Ok(())
}

async fn handle_expire(settings: &Settings) -> anyhow::Result<()> {
    let db_manager = DatabaseManager::open(&settings.database.path).await?;
    run_migrations(db_manager.db()).await?;

    let conn = db_manager.db().connect()?;
    let repo = LibsqlRepository::new(conn);

    println!("{}", "Expire Old Memories".bold().green());
    let deleted = repo.delete_expired().await?;
    println!("  {} Deleted {} expired memories", "✓".green(), deleted);

    Ok(())
}

fn maybe_create_ai_client(settings: &Settings) -> Option<Glm5Client> {
    if settings.ai.api_key.trim().is_empty() {
        Glm5Client::from_env().ok()
    } else {
        Some(Glm5Client::with_config(Glm5Config {
            api_key: settings.ai.api_key.clone(),
            base_url: settings.ai.base_url.clone(),
            model: settings.ai.model.clone(),
            temperature: settings.ai.temperature,
            max_tokens: settings.ai.max_tokens,
        }))
    }
}

async fn summarize_thread(thread: &[Conversation], ai_client: Option<&Glm5Client>) -> String {
    if let Some(client) = ai_client {
        let mut transcript = String::new();
        for row in thread.iter().take(40) {
            transcript.push_str(&format!("[{} {}]: {}\n", row.created_at, row.author_did, row.content));
            if transcript.len() > 6000 {
                break;
            }
        }

        let messages = vec![
            Message::system(
                "Summarize this thread for future retrieval. Focus on user intent, key facts, and resolutions.",
            ),
            Message::user(transcript),
        ];

        if let Ok(summary) = client.chat(messages).await {
            let trimmed = summary.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }

    let mut parts = Vec::new();
    for msg in thread.iter().take(8) {
        parts.push(format!("{}: {}", msg.author_did, msg.content));
    }
    format!("Thread summary ({} messages): {}", thread.len(), parts.join(" | "))
}

async fn add_days_sql(conn: &libsql::Connection, base_ts: &str, days: u32) -> anyhow::Result<String> {
    let modifier = format!("+{} days", days);
    let mut rows = conn
        .query("SELECT datetime(?1, ?2)", (base_ts, modifier.as_str()))
        .await?;
    if let Ok(Some(row)) = rows.next().await {
        Ok(row.get::<String>(0)?)
    } else {
        Ok(base_ts.to_string())
    }
}

async fn query_i64<P>(conn: &libsql::Connection, sql: &str, params: P) -> anyhow::Result<i64>
where
    P: libsql::params::IntoParams,
{
    let mut rows = conn.query(sql, params).await?;
    if let Ok(Some(row)) = rows.next().await { Ok(row.get::<i64>(0).unwrap_or(0)) } else { Ok(0) }
}

fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.2} MB", b / MB)
    } else if b >= KB {
        format!("{:.2} KB", b / KB)
    } else {
        format!("{} B", bytes)
    }
}
