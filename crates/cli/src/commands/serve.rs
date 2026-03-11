use anyhow::Context;
use owo_colors::OwoColorize;
use std::collections::HashSet;
use std::future::Future;
use std::sync::Arc;
use std::time::Instant;
use tnbot_core::Settings;
use tnbot_core::ai::{DEFAULT_CONSTITUTION, Glm5Client, Glm5Config, PromptBuilder};
use tnbot_core::bsky::BskyClient;
use tnbot_core::db::connection::DatabaseManager;
use tnbot_core::db::migrations::run_migrations;
use tnbot_core::db::repository::LibsqlRepository;
use tnbot_core::embedding::{EmbeddingPipeline, EmbeddingPipelineConfig, EmbeddingProvider};
use tnbot_core::jetstream::{EventProcessor, FilteredEvent, ProcessedEvent};
use tnbot_core::services::{AccessPolicy, ActionPipeline, MemoryRetriever, MemoryRetrieverConfig};
use tnbot_web::runtime::SharedRuntimeState;

const DEFAULT_BOT_NAME: &str = "ThunderBot";

struct ActionEventProcessor {
    pipeline: ActionPipeline<LibsqlRepository>,
    runtime: SharedRuntimeState,
}

impl ActionEventProcessor {
    fn new(pipeline: ActionPipeline<LibsqlRepository>, runtime: SharedRuntimeState) -> Self {
        Self { pipeline, runtime }
    }
}

#[async_trait::async_trait]
impl EventProcessor for ActionEventProcessor {
    async fn process(
        &self, mut event: FilteredEvent,
    ) -> Result<ProcessedEvent, Box<dyn std::error::Error + Send + Sync>> {
        self.runtime.record_jetstream_event(event.cursor());
        self.runtime.begin_processing();

        if self.runtime.is_paused() {
            tracing::debug!(
                cursor = event.cursor(),
                "Bot paused; acknowledging event without processing"
            );
            event.acknowledge();
            self.runtime.finish_processing(true, None);
            return Ok(ProcessedEvent { event, success: true, error: None });
        }

        let started = Instant::now();
        match self.pipeline.process_mention(&event).await {
            Ok(result) => {
                if result.loop_prevented {
                    tracing::debug!(post_uri = %result.post_uri, "Loop prevention triggered");
                } else if result.blocked_by_access_policy {
                    tracing::info!(post_uri = %result.post_uri, "Author blocked by access policy");
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
                self.runtime
                    .finish_processing(true, Some(started.elapsed().as_millis() as u64));
                Ok(ProcessedEvent { event, success: true, error: None })
            }
            Err(e) => {
                self.runtime.finish_processing(false, None);
                Ok(ProcessedEvent { event, success: false, error: Some(e.to_string()) })
            }
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

async fn create_bsky_client(settings: &Settings, dry_run: bool) -> anyhow::Result<BskyClient> {
    let handle = settings.bluesky.handle.trim();
    if handle.is_empty() {
        if dry_run {
            let fallback = settings.bluesky.pds_host.trim();
            let fallback = if fallback.is_empty() { "https://bsky.social" } else { fallback };
            return Ok(BskyClient::new(fallback));
        }
        anyhow::bail!("Bluesky handle is required. Set TNBOT_BLUESKY__HANDLE.");
    }

    let pds_host = BskyClient::determine_pds_host(handle, &settings.bluesky.pds_host)
        .await
        .with_context(|| format!("Failed to resolve PDS host for `{}`", handle))?;
    tracing::info!(handle = %handle, pds_host = %pds_host, "Resolved PDS host from Bluesky API");

    if dry_run {
        return Ok(BskyClient::new(&pds_host));
    }

    if settings.bluesky.app_password.trim().is_empty() {
        anyhow::bail!("Bluesky app password is required. Set TNBOT_BLUESKY__APP_PASSWORD.");
    }

    Ok(BskyClient::with_credentials(
        &pds_host,
        handle,
        &settings.bluesky.app_password,
    ))
}

async fn resolve_bot_identity(settings: &Settings, client: &BskyClient) -> anyhow::Result<(String, String)> {
    let handle = settings.bluesky.handle.trim();
    if handle.is_empty() {
        if settings.bot.did.trim().is_empty() {
            anyhow::bail!("Cannot resolve bot identity without a Bluesky handle. Set TNBOT_BLUESKY__HANDLE.");
        }
        let fallback_name = if settings.bot.name.trim().is_empty() {
            DEFAULT_BOT_NAME.to_string()
        } else {
            settings.bot.name.clone()
        };
        return Ok((settings.bot.did.clone(), fallback_name));
    }

    let discovered_did = client
        .resolve_handle(handle)
        .await
        .with_context(|| format!("Failed to resolve DID for handle `{}`", handle))?;
    let did_override = settings.bot.did.trim();
    let did = if did_override.is_empty() {
        discovered_did.clone()
    } else {
        if did_override != discovered_did {
            tracing::warn!(
                configured_did = %did_override,
                discovered_did = %discovered_did,
                handle = %handle,
                "Configured bot DID overrides the DID discovered from Bluesky APIs"
            );
        }
        did_override.to_string()
    };

    let profile = client
        .get_profile(handle)
        .await
        .with_context(|| format!("Failed to resolve profile for handle `{}`", handle))?;
    let discovered_name = profile
        .display_name
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(profile.handle);
    let name_override = settings.bot.name.trim();
    let name = if name_override.is_empty() || name_override == DEFAULT_BOT_NAME {
        discovered_name
    } else {
        name_override.to_string()
    };

    tracing::info!(
        handle = %handle,
        pds_host = %client.pds_host(),
        did = %did,
        bot_name = %name,
        "Resolved bot identity from Bluesky APIs"
    );

    Ok((did, name))
}

async fn verify_authenticated_identity(client: &BskyClient, expected_did: &str, dry_run: bool) -> anyhow::Result<()> {
    if dry_run {
        return Ok(());
    }

    let session = client.ensure_valid_session().await?;
    if session.did != expected_did {
        anyhow::bail!(
            "Authenticated DID `{}` does not match resolved bot DID `{}`",
            session.did,
            expected_did
        );
    }

    Ok(())
}

async fn resolve_allowed_dids(settings: &Settings, bsky_client: &BskyClient) -> anyhow::Result<HashSet<String>> {
    resolve_allowed_dids_with(settings, |handle| async move {
        bsky_client
            .resolve_handle(&handle)
            .await
            .with_context(|| format!("Failed to resolve allowlisted handle `{}` to DID", handle))
    })
    .await
}

async fn resolve_allowed_dids_with<F, Fut>(
    settings: &Settings, mut resolve_handle: F,
) -> anyhow::Result<HashSet<String>>
where
    F: FnMut(String) -> Fut,
    Fut: Future<Output = anyhow::Result<String>>,
{
    let mut allowed = HashSet::new();

    for did in &settings.access.allowed_dids {
        let trimmed = did.trim();
        if !trimmed.is_empty() {
            allowed.insert(trimmed.to_string());
        }
    }

    for handle in &settings.access.allowed_handles {
        let trimmed = handle.trim();
        if trimmed.is_empty() {
            continue;
        }
        let did = resolve_handle(trimmed.to_string()).await?;
        allowed.insert(did);
    }

    Ok(allowed)
}

async fn build_access_policy(settings: &Settings, bsky_client: &BskyClient) -> anyhow::Result<AccessPolicy> {
    let allowed_dids = resolve_allowed_dids(settings, bsky_client).await?;
    let policy = AccessPolicy::new(allowed_dids, settings.access.unauthorized_policy);
    if policy.allowed_did_count() > 0 {
        tracing::info!(
            allowed_did_count = policy.allowed_did_count(),
            policy = ?settings.access.unauthorized_policy,
            "Access whitelist enabled for mention interactions"
        );
    } else {
        tracing::info!("Access whitelist disabled; all mentions are eligible for processing");
    }
    Ok(policy)
}

async fn update_presence_status(client: &BskyClient, emoji: &str, dry_run: bool) {
    if dry_run {
        return;
    }

    if let Err(e) = client.update_profile_status_prefix(emoji).await {
        tracing::warn!(error = %e, status = %emoji, "Failed to update bot profile status");
    }
}

async fn verify_embedding_provider(settings: &Settings, provider: &Arc<dyn EmbeddingProvider>) -> anyhow::Result<()> {
    let probe_text = "thunderbot startup embedding probe";
    let embedding = provider.embed(probe_text).await.with_context(|| {
        format!(
            "Embedding provider preflight failed (provider=`{}`, model=`{}`, base_url=`{}`)",
            settings.embedding.provider, settings.embedding.model, settings.embedding.base_url
        )
    })?;

    if embedding.len() != settings.embedding.dimensions {
        anyhow::bail!(
            "Embedding provider dimension mismatch: got {}, expected {} for model `{}`",
            embedding.len(),
            settings.embedding.dimensions,
            settings.embedding.model
        );
    }

    tracing::info!(
        provider = %settings.embedding.provider,
        model = %settings.embedding.model,
        base_url = %settings.embedding.base_url,
        dimensions = embedding.len(),
        "Embedding provider preflight succeeded"
    );
    Ok(())
}

pub async fn run(settings: &Settings, dry_run: bool) -> anyhow::Result<()> {
    let mode_banner = if dry_run {
        "Starting daemon in cognitive dry-run mode (generates AI replies, does not post)..."
    } else {
        "Starting daemon with cognitive pipeline..."
    };
    println!("{}", mode_banner.green().bold());

    let ai_client = create_ai_client(settings)?;
    let bsky_client = create_bsky_client(settings, dry_run).await?;
    let (resolved_bot_did, resolved_bot_name) = resolve_bot_identity(settings, &bsky_client).await?;
    verify_authenticated_identity(&bsky_client, &resolved_bot_did, dry_run).await?;
    let access_policy = build_access_policy(settings, &bsky_client).await?;
    let prompt_builder = PromptBuilder::new(DEFAULT_CONSTITUTION);
    let mut runtime_settings = settings.clone();
    runtime_settings.bluesky.pds_host = bsky_client.pds_host().to_string();
    runtime_settings.bot.did = resolved_bot_did.clone();
    runtime_settings.bot.name = resolved_bot_name;

    let db_manager = DatabaseManager::open(&settings.database.path).await?;
    run_migrations(db_manager.db()).await?;

    let conn = db_manager.db().connect()?;
    let repo = Arc::new(LibsqlRepository::new(conn));

    let mut memory_retriever = None;
    let embedding_sender = if settings.memory.enabled {
        tracing::info!("Initializing embedding pipeline...");
        let provider: Arc<dyn EmbeddingProvider> = Arc::from(settings.embedding.create_provider());
        verify_embedding_provider(settings, &provider).await?;
        let pipeline_config = EmbeddingPipelineConfig {
            poll_interval_secs: 30,
            batch_size: settings.embedding.batch_size,
            dedup_threshold: settings.memory.dedup_threshold,
            max_retries: 3,
            post_ttl_days: settings.memory.ttl_days,
        };

        let (pipeline, rx) = EmbeddingPipeline::new(repo.clone(), provider.clone(), pipeline_config);

        let sender = pipeline.sender();

        tokio::spawn(async move { pipeline.run(rx).await });

        memory_retriever = Some(MemoryRetriever::new(
            (*repo).clone(),
            provider.clone(),
            MemoryRetrieverConfig::default(),
        ));

        tracing::info!("Embedding pipeline started");
        Some(sender)
    } else {
        tracing::info!("Embedding pipeline disabled");
        None
    };

    let mut action_pipeline = ActionPipeline::new(
        ai_client,
        bsky_client.clone(),
        (*repo).clone(),
        prompt_builder,
        runtime_settings.bot.did.clone(),
    );
    if dry_run {
        action_pipeline = action_pipeline.with_dry_run();
    }
    if let Some(sender) = embedding_sender {
        action_pipeline = action_pipeline.with_embedding_sender(sender);
    }
    if let Some(retriever) = memory_retriever {
        action_pipeline = action_pipeline.with_memory_retriever(retriever);
    }
    action_pipeline = action_pipeline.with_access_policy(access_policy);

    let runtime = tnbot_web::runtime::new_shared_runtime();
    let processor = ActionEventProcessor::new(action_pipeline, runtime.clone());
    let presence_client = bsky_client.clone();

    update_presence_status(&presence_client, "🟢", dry_run).await;

    let web_settings = runtime_settings.clone();
    let web_handle = tokio::spawn(async move {
        if let Err(e) = tnbot_web::run(web_settings, runtime, dry_run).await {
            tracing::error!(error = %e, "Web dashboard stopped");
        }
    });

    let daemon_result =
        super::jetstream::listen_with_processor(processor, None, runtime_settings.bot.did.clone(), None).await;

    web_handle.abort();
    let _ = web_handle.await;

    update_presence_status(&presence_client, "🔴", dry_run).await;

    daemon_result
}

#[cfg(test)]
mod tests {
    use super::{resolve_allowed_dids_with, verify_authenticated_identity};
    use tnbot_core::Settings;
    use tnbot_core::bsky::{BskyClient, CreateSessionResponse, Session};

    #[test]
    fn test_bind_validation_allows_empty_configured_bot_did() {
        let mut settings = Settings::default();
        settings.bot.did.clear();
        assert!(settings.bot.did.is_empty());
    }

    #[tokio::test]
    async fn test_bind_validation_fails_on_mismatched_did() {
        let mut settings = Settings::default();
        settings.bot.did = "did:plc:configured".to_string();

        let client = BskyClient::new("https://bsky.social");
        let session = Session::from_create_response(CreateSessionResponse {
            access_jwt: "invalid-jwt".to_string(),
            refresh_jwt: "refresh".to_string(),
            handle: "bot.bsky.social".to_string(),
            did: "did:plc:runtime".to_string(),
            did_doc: None,
        })
        .unwrap();
        client.set_session(session).await;

        let result = verify_authenticated_identity(&client, &settings.bot.did, false).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_bind_validation_skips_in_dry_run() {
        let settings = Settings::default();
        let client = BskyClient::new("https://bsky.social");
        assert!(
            verify_authenticated_identity(&client, &settings.bot.did, true)
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn test_resolve_allowed_dids_merges_dids_and_handles() {
        let mut settings = Settings::default();
        settings.access.allowed_dids = vec!["did:plc:explicit".to_string()];
        settings.access.allowed_handles = vec!["alice.bsky.social".to_string(), "bob.bsky.social".to_string()];

        let resolved = resolve_allowed_dids_with(&settings, |handle| async move {
            match handle.as_str() {
                "alice.bsky.social" => Ok("did:plc:alice".to_string()),
                "bob.bsky.social" => Ok("did:plc:bob".to_string()),
                other => anyhow::bail!("unexpected handle {}", other),
            }
        })
        .await
        .unwrap();

        assert_eq!(resolved.len(), 3);
        assert!(resolved.contains("did:plc:explicit"));
        assert!(resolved.contains("did:plc:alice"));
        assert!(resolved.contains("did:plc:bob"));
    }
}
