use owo_colors::OwoColorize;
use std::sync::Arc;
use tnbot_core::Settings;
use tnbot_core::db::connection::DatabaseManager;
use tnbot_core::db::migrations::run_migrations;
use tnbot_core::db::repository::LibsqlRepository;
use tnbot_core::processor::DatabaseEventProcessor;

pub async fn run(settings: &Settings, dry_run: bool) -> anyhow::Result<()> {
    if dry_run {
        println!("{}", "Running in dry-run mode (no posts will be made)".yellow());
        println!("{}", "Starting daemon in log-only mode...".green().bold());
        return super::jetstream::listen(None, settings.bot.did.clone(), None).await;
    }

    println!("{}", "Starting daemon with database persistence...".green().bold());

    let db_manager = DatabaseManager::open(&settings.database.path).await?;
    run_migrations(db_manager.db()).await?;

    let conn = db_manager.db().connect()?;
    let repo = Arc::new(LibsqlRepository::new(conn));
    let processor = DatabaseEventProcessor::new(repo);

    super::jetstream::listen_with_processor(processor, None, settings.bot.did.clone(), None).await
}
