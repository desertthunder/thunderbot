use crate::cli::{Cli, VectorAction};
use owo_colors::OwoColorize;
use tnbot_core::Settings;
use tnbot_core::db::connection::DatabaseManager;
use tnbot_core::db::migrations::run_migrations;
use tnbot_core::db::repository::{LibsqlRepository, MemoryRepository};

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

    let embed_config = &settings.embedding;
    let provider = embed_config.create_provider();

    println!("{} {}", "Generating embedding for query:".dimmed(), query);
    let query_embedding = provider.embed(&query).await?;

    let k = top_k.unwrap_or(5);
    println!("{} {}", "Searching top-k:".dimmed(), k);

    let results = if let Some(author_did) = author {
        repo.search_by_author(&author_did, &query_embedding, k).await?
    } else {
        repo.search_semantic(&query_embedding, k).await?
    };

    println!("\n{}", "Search Results".bold().green());
    println!("  Found {} memories\n", results.len());

    for (i, memory) in results.iter().enumerate() {
        let distance_str = memory
            .distance
            .map(|d| format!("{:.4}", d))
            .unwrap_or_else(|| "N/A".to_string());
        println!("  {}. Distance: {}", (i + 1).to_string().cyan(), distance_str.yellow());
        println!("     Content: {}", memory.content);
        println!("     Root:    {}", memory.root_uri.dimmed());
        println!();
    }

    Ok(())
}

async fn handle_embed(settings: &Settings, text: String) -> anyhow::Result<()> {
    let embed_config = &settings.embedding;
    let provider = embed_config.create_provider();

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
    let repo = LibsqlRepository::new(conn);

    println!("{}", "Vector Backfill".bold().green());
    println!("  Finding conversations without embeddings...\n");

    let mut count_rows = repo
        .conn()
        .query(
            "SELECT COUNT(*)
         FROM conversations c
         LEFT JOIN embedding_jobs ej ON c.id = ej.conversation_id
         WHERE ej.id IS NULL",
            (),
        )
        .await?;

    let pending_count: i64 = if let Ok(Some(row)) = count_rows.next().await { row.get(0).unwrap_or(0) } else { 0 };

    println!("  Found {} conversations to backfill", pending_count.to_string().cyan());

    if pending_count == 0 {
        println!("  {} All conversations have embedding jobs", "✓".green());
        return Ok(());
    }

    let limit = batch_size.unwrap_or(100) as i64;
    let mut total_created = 0;

    loop {
        let created = create_embedding_jobs_batch(&repo, limit).await?;
        total_created += created;

        if created == 0 {
            break;
        }

        println!("  Created {} embedding jobs...", created);

        if total_created >= pending_count as usize {
            break;
        }
    }

    println!("\n  {} Created {} embedding jobs total", "✓".green(), total_created);
    println!("  Jobs will be processed by the embedding pipeline worker.");

    Ok(())
}

async fn create_embedding_jobs_batch(repo: &LibsqlRepository, limit: i64) -> anyhow::Result<usize> {
    let mut rows = repo
        .conn()
        .query(
            "SELECT c.id, c.content, c.root_uri, c.author_did, c.created_at
         FROM conversations c
         LEFT JOIN embedding_jobs ej ON c.id = ej.conversation_id
         WHERE ej.id IS NULL
         LIMIT ?1",
            [limit],
        )
        .await?;

    let mut count = 0;
    while let Ok(Some(row)) = rows.next().await {
        let conversation_id: i64 = row.get(0)?;
        let _content: String = row.get(1)?;
        let _root_uri: String = row.get(2)?;
        let _author_did: String = row.get(3)?;
        let created_at: String = row.get(4)?;

        match repo.create_embedding_job(conversation_id, &created_at).await {
            Ok(job_id) if job_id > 0 => {
                count += 1;
            }
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(conversation_id, error = %e, "Failed to create embedding job");
            }
        }
    }

    Ok(count)
}

async fn handle_consolidate(_settings: &Settings) -> anyhow::Result<()> {
    println!("{}", "Consolidate Memories".bold().yellow());
    println!("  This feature is not yet implemented.");
    println!("  Future: Summarize threads older than 24h and embed summaries.");
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
