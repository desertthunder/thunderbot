use owo_colors::OwoColorize;
use std::sync::Arc;
use tnbot_core::Settings;
use tnbot_core::ai::{DEFAULT_CONSTITUTION, Glm5Client, Glm5Config, PromptBuilder};
use tnbot_core::bsky::BskyClient;
use tnbot_core::db::connection::DatabaseManager;
use tnbot_core::db::migrations::run_migrations;
use tnbot_core::db::repository::LibsqlRepository;
use tnbot_core::embedding::{EmbeddingPipeline, EmbeddingPipelineConfig};
use tnbot_core::jetstream::{EventProcessor, FilteredEvent, ProcessedEvent};
use tnbot_core::services::ActionPipeline;

struct ActionEventProcessor {
    pipeline: ActionPipeline<LibsqlRepository>,
}

impl ActionEventProcessor {
    fn new(pipeline: ActionPipeline<LibsqlRepository>) -> Self {
        Self { pipeline }
    }
}

#[async_trait::async_trait]
impl EventProcessor for ActionEventProcessor {
    async fn process(
        &self, mut event: FilteredEvent,
    ) -> Result<ProcessedEvent, Box<dyn std::error::Error + Send + Sync>> {
        match self.pipeline.process_mention(&event).await {
            Ok(result) => {
                if result.loop_prevented {
                    tracing::debug!(post_uri = %result.post_uri, "Loop prevention triggered");
                } else if result.silent {
                    tracing::info!(post_uri = %result.post_uri, "Model returned silent mode");
                } else if let Some(reply_uri) = result.posted_reply_uri {
                    tracing::info!(post_uri = %result.post_uri, reply_uri = %reply_uri, "Reply posted");
                } else if let Some(response_text) = result.response_text {
                    tracing::info!(
                        post_uri = %result.post_uri,
                        response = %response_text,
                        "Dry run: generated response without posting"
                    );
                }

                event.acknowledge();
                Ok(ProcessedEvent { event, success: true, error: None })
            }
            Err(e) => Ok(ProcessedEvent { event, success: false, error: Some(e.to_string()) }),
        }
    }
}

fn create_ai_client(settings: &Settings) -> anyhow::Result<Glm5Client> {
    if settings.ai.api_key.is_empty() {
        Glm5Client::from_env().map_err(Into::into)
    } else {
        let config = Glm5Config {
            api_key: settings.ai.api_key.clone(),
            base_url: settings.ai.base_url.clone(),
            model: settings.ai.model.clone(),
            temperature: settings.ai.temperature,
            max_tokens: settings.ai.max_tokens,
        };
        Ok(Glm5Client::with_config(config))
    }
}

fn create_bsky_client(settings: &Settings, dry_run: bool) -> anyhow::Result<BskyClient> {
    if dry_run {
        return Ok(BskyClient::new(&settings.bluesky.pds_host));
    }

    if settings.bluesky.handle.trim().is_empty() || settings.bluesky.app_password.trim().is_empty() {
        anyhow::bail!(
            "Bluesky credentials are not configured. Set TNBOT_BLUESKY__HANDLE and TNBOT_BLUESKY__APP_PASSWORD."
        );
    }

    Ok(BskyClient::with_credentials(
        &settings.bluesky.pds_host,
        &settings.bluesky.handle,
        &settings.bluesky.app_password,
    ))
}

pub async fn run(settings: &Settings, dry_run: bool) -> anyhow::Result<()> {
    let mode_banner = if dry_run {
        "Starting daemon in cognitive dry-run mode (generates AI replies, does not post)..."
    } else {
        "Starting daemon with cognitive pipeline..."
    };
    println!("{}", mode_banner.green().bold());

    let ai_client = create_ai_client(settings)?;
    let bsky_client = create_bsky_client(settings, dry_run)?;
    let prompt_builder = PromptBuilder::new(DEFAULT_CONSTITUTION);

    let db_manager = DatabaseManager::open(&settings.database.path).await?;
    run_migrations(db_manager.db()).await?;

    let conn = db_manager.db().connect()?;
    let repo = Arc::new(LibsqlRepository::new(conn));

    let embedding_sender = if settings.memory.enabled {
        tracing::info!("Initializing embedding pipeline...");
        let provider = Arc::from(settings.embedding.create_provider());
        let pipeline_config = EmbeddingPipelineConfig {
            poll_interval_secs: 30,
            batch_size: settings.embedding.batch_size,
            dedup_threshold: settings.memory.dedup_threshold,
            max_retries: 3,
        };

        let (pipeline, rx) = EmbeddingPipeline::new(repo.clone(), provider, pipeline_config);

        let sender = pipeline.sender();

        tokio::spawn(async move { pipeline.run(rx).await });

        tracing::info!("Embedding pipeline started");
        Some(sender)
    } else {
        tracing::info!("Embedding pipeline disabled");
        None
    };

    let mut action_pipeline = ActionPipeline::new(
        ai_client,
        bsky_client,
        (*repo).clone(),
        prompt_builder,
        settings.bot.did.clone(),
    );
    if dry_run {
        action_pipeline = action_pipeline.with_dry_run();
    }
    if let Some(sender) = embedding_sender {
        action_pipeline = action_pipeline.with_embedding_sender(sender);
    }

    let processor = ActionEventProcessor::new(action_pipeline);

    super::jetstream::listen_with_processor(processor, None, settings.bot.did.clone(), None).await
}
