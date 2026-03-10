//! Action pipeline for processing mentions and generating responses
//!
//! Orchestrates the full flow:
//! 1. Loop prevention check (skip if author_did == bot_did)
//! 2. Store incoming mention
//! 3. Build context from thread history
//! 4. Generate AI response via GLM-5
//! 5. Check for silent mode (<SILENT_THOUGHT>)
//! 6. Post reply with proper threading (Root/Parent refs)
//! 7. Store bot's reply in database
//! 8. Handle rate limits with exponential backoff

use crate::ai::client::Glm5Client;
use crate::ai::prompt::PromptBuilder;
use crate::ai::types::ChatCompletionRequest;
use crate::bsky::{BskyClient, CreateRecordResponse, StrongRef};
use crate::db::models::MemorySearchFilters;
use crate::db::models::{CreateConversationParams, Role};
use crate::db::repository::{ConversationRepository, IdentityRepository, MemoryRepository};
use crate::embedding::EmbeddingPipelineMessage;
use crate::error::{BotError, Result};
use crate::jetstream::filter::FilteredEvent;
use crate::jetstream::types::JetstreamEvent;
use crate::services::memory::MemoryRetriever;
use crate::services::thread::{
    extract_created_at, extract_parent_uri, extract_root_cid, extract_root_uri, extract_text,
};
use rand::RngExt;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::mpsc;

/// Action pipeline for processing mentions
pub struct ActionPipeline<R: ConversationRepository + IdentityRepository + MemoryRepository + Clone + Send + Sync> {
    ai_client: Glm5Client,
    bsky_client: BskyClient,
    repo: R,
    prompt_builder: PromptBuilder,
    bot_did: String,
    dry_run: bool,
    embedding_sender: Option<mpsc::Sender<EmbeddingPipelineMessage>>,
    memory_retriever: Option<MemoryRetriever<R>>,
}

/// Result of processing a mention
#[derive(Debug, Clone)]
pub struct ActionResult {
    pub post_uri: String,
    pub root_uri: String,
    pub response_text: Option<String>,
    pub posted_reply_uri: Option<String>,
    pub silent: bool,
    pub loop_prevented: bool,
    pub error: Option<String>,
}

impl<R: ConversationRepository + IdentityRepository + MemoryRepository + Clone + Send + Sync> ActionPipeline<R> {
    /// Create a new action pipeline
    pub fn new(
        ai_client: Glm5Client, bsky_client: BskyClient, repo: R, prompt_builder: PromptBuilder, bot_did: String,
    ) -> Self {
        Self {
            ai_client,
            bsky_client,
            repo,
            prompt_builder,
            bot_did,
            dry_run: false,
            embedding_sender: None,
            memory_retriever: None,
        }
    }

    /// Enable dry-run mode (process but don't post)
    pub fn with_dry_run(mut self) -> Self {
        self.dry_run = true;
        self
    }

    /// Set the embedding pipeline sender for creating embedding jobs
    pub fn with_embedding_sender(mut self, sender: mpsc::Sender<EmbeddingPipelineMessage>) -> Self {
        self.embedding_sender = Some(sender);
        self
    }

    /// Set memory retriever for hybrid RAG context injection.
    pub fn with_memory_retriever(mut self, retriever: MemoryRetriever<R>) -> Self {
        self.memory_retriever = Some(retriever);
        self
    }

    /// Process a mention event through the full pipeline
    pub async fn process_mention(&self, event: &FilteredEvent) -> Result<ActionResult> {
        let (author_did, commit) = match &event.event {
            JetstreamEvent::Commit { did, commit, .. } => (did.clone(), commit),
            _ => {
                return Err(BotError::Validation("Non-commit event received".to_string()));
            }
        };

        let post_uri = format!("at://{}/app.bsky.feed.post/{}", author_did, commit.rkey);
        let cid = commit.cid.clone();

        let record = match &commit.record {
            Some(r) => r,
            None => {
                return Err(BotError::Validation("No record in commit".to_string()));
            }
        };

        let root_uri = extract_root_uri(&post_uri, record);
        let parent_uri = extract_parent_uri(record);
        let root_cid = extract_root_cid(record).or_else(|| if root_uri == post_uri { cid.clone() } else { None });
        let content = extract_text(record);
        let created_at = extract_created_at(record).unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

        if author_did == self.bot_did {
            tracing::debug!(
                post_uri = %post_uri,
                "Skipping own post (loop prevention)"
            );
            return Ok(ActionResult {
                post_uri,
                root_uri,
                response_text: None,
                posted_reply_uri: None,
                silent: false,
                loop_prevented: true,
                error: None,
            });
        }

        let mention_params = CreateConversationParams {
            root_uri: root_uri.clone(),
            post_uri: post_uri.clone(),
            parent_uri: parent_uri.clone(),
            author_did: author_did.clone(),
            role: Role::User,
            content: content.clone(),
            cid: cid.clone(),
            created_at: created_at.clone(),
        };

        let was_inserted = self.repo.create_conversation(mention_params).await?;
        tracing::info!(post_uri = %post_uri, was_inserted = was_inserted, "Stored incoming mention");

        if was_inserted
            && let Some(ref sender) = self.embedding_sender
            && let Ok(Some(conversation)) = self.repo.get_by_post_uri(&post_uri).await
        {
            let msg = EmbeddingPipelineMessage::CreateJob {
                conversation_id: conversation.id,
                content: content.clone(),
                root_uri: root_uri.clone(),
                author_did: author_did.clone(),
            };
            match sender.try_send(msg) {
                Ok(_) => tracing::debug!(conversation_id = conversation.id, "Queued embedding job"),
                Err(e) => {
                    tracing::warn!(conversation_id = conversation.id, error = %e, "Failed to queue embedding job")
                }
            }
        }

        let thread = self.repo.get_thread_by_root(&root_uri).await?;
        tracing::debug!(
            root_uri = %root_uri,
            thread_length = thread.len(),
            "Fetched thread context"
        );

        let mut identity_map = self.build_identity_map(&thread).await?;

        let memory_lines = if let Some(retriever) = &self.memory_retriever {
            let filters = MemorySearchFilters { exclude_root_uri: Some(root_uri.clone()), ..Default::default() };
            match retriever.retrieve_hybrid(&content, filters, Some(5)).await {
                Ok(memories) => {
                    let mut lines = Vec::with_capacity(memories.len());
                    for result in memories {
                        let did = result.memory.author_did.clone();
                        let handle = if let Some(existing) = identity_map.get(&did) {
                            existing.clone()
                        } else {
                            let resolved = match self.repo.get_by_did(&did).await? {
                                Some(identity) => identity.handle,
                                None => did.clone(),
                            };
                            identity_map.insert(did.clone(), resolved.clone());
                            resolved
                        };

                        let date = result
                            .memory
                            .created_at
                            .split('T')
                            .next()
                            .unwrap_or(result.memory.created_at.as_str());
                        lines.push(format!(
                            "[{} @{} | {}] {}",
                            date,
                            handle,
                            format!("{:?}", result.source).to_lowercase(),
                            result.memory.content
                        ));
                    }
                    lines
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to retrieve memory context for RAG");
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };

        let resolve_handle = |did: &str| identity_map.get(did).map(|s| s.as_str()).unwrap_or(did).to_string();

        let messages = if memory_lines.is_empty() {
            self.prompt_builder.build(&thread, resolve_handle)
        } else {
            self.prompt_builder
                .build_with_memories(&thread, &memory_lines, resolve_handle)
        };
        let request = ChatCompletionRequest::new(self.ai_client.model(), messages).with_thinking();

        let ai_response = self.ai_client.chat_completion(request).await?;

        let response_text = ai_response
            .content()
            .map(|s| s.to_string())
            .ok_or_else(|| BotError::AiResponse("Empty response from model".to_string()))?;

        if response_text.contains("<SILENT_THOUGHT>") {
            tracing::info!(
                post_uri = %post_uri,
                "Bot chose silent mode, not replying"
            );
            return Ok(ActionResult {
                post_uri,
                root_uri,
                response_text: Some(response_text),
                posted_reply_uri: None,
                silent: true,
                loop_prevented: false,
                error: None,
            });
        }

        let reply_record = if self.dry_run {
            tracing::info!(
                post_uri = %post_uri,
                response = %response_text,
                "[DRY RUN] Would post reply"
            );
            None
        } else {
            match self
                .post_reply_with_retry(
                    &post_uri,
                    &root_uri,
                    root_cid.as_deref(),
                    cid.as_deref(),
                    &response_text,
                )
                .await
            {
                Ok(record) => {
                    tracing::info!(
                        post_uri = %post_uri,
                        reply_uri = %record.uri,
                        "Posted reply"
                    );
                    Some(record)
                }
                Err(e) => {
                    tracing::error!(
                        post_uri = %post_uri,
                        error = %e,
                        "Failed to post reply after retries"
                    );
                    return Err(e);
                }
            }
        };

        if let Some(ref reply) = reply_record {
            let bot_reply_params = CreateConversationParams {
                root_uri: root_uri.clone(),
                post_uri: reply.uri.clone(),
                parent_uri: Some(post_uri.clone()),
                author_did: self.bot_did.clone(),
                role: Role::Model,
                content: response_text.clone(),
                cid: Some(reply.cid.clone()),
                created_at: chrono::Utc::now().to_rfc3339(),
            };

            let was_inserted = self.repo.create_conversation(bot_reply_params).await?;
            tracing::debug!(
                reply_uri = %reply.uri,
                was_inserted = was_inserted,
                "Stored bot reply in database"
            );

            if was_inserted
                && let Some(ref sender) = self.embedding_sender
                && let Ok(Some(conversation)) = self.repo.get_by_post_uri(&reply.uri).await
            {
                let msg = EmbeddingPipelineMessage::CreateJob {
                    conversation_id: conversation.id,
                    content: response_text.clone(),
                    root_uri: root_uri.clone(),
                    author_did: self.bot_did.clone(),
                };
                if let Err(e) = sender.try_send(msg) {
                    tracing::warn!(conversation_id = conversation.id, error = %e, "Failed to queue bot reply embedding job");
                } else {
                    tracing::debug!(conversation_id = conversation.id, "Queued bot reply embedding job");
                }
            }
        }

        Ok(ActionResult {
            post_uri,
            root_uri,
            response_text: Some(response_text),
            posted_reply_uri: reply_record.map(|r| r.uri),
            silent: false,
            loop_prevented: false,
            error: None,
        })
    }

    /// Post a reply with exponential backoff on rate limits
    async fn post_reply_with_retry(
        &self, parent_uri: &str, root_uri: &str, root_cid: Option<&str>, parent_cid: Option<&str>, text: &str,
    ) -> Result<CreateRecordResponse> {
        let max_retries = 3;
        let initial_backoff_ms = 1000;
        let max_backoff_ms = 60000;

        for attempt in 0..max_retries {
            let result = if let (Some(root_cid), Some(parent_cid)) = (root_cid, parent_cid) {
                let root_ref = StrongRef { uri: root_uri.to_string(), cid: root_cid.to_string() };
                let parent_ref = StrongRef { uri: parent_uri.to_string(), cid: parent_cid.to_string() };
                self.bsky_client.reply_with_refs(root_ref, parent_ref, text).await
            } else {
                self.bsky_client.reply_to(parent_uri, text).await
            };

            match result {
                Ok(response) => {
                    return Ok(response);
                }
                Err(BotError::XrpcRateLimit(_)) if attempt + 1 < max_retries => {
                    let backoff = calculate_backoff(attempt, initial_backoff_ms, max_backoff_ms);
                    tracing::warn!(
                        attempt = attempt + 1,
                        max_attempts = max_retries,
                        backoff_ms = backoff,
                        "Rate limited, retrying with backoff"
                    );
                    tokio::time::sleep(Duration::from_millis(backoff)).await;
                }
                Err(BotError::XrpcServerError(_)) if attempt + 1 < max_retries => {
                    let backoff = calculate_backoff(attempt, initial_backoff_ms, max_backoff_ms);
                    tracing::warn!(
                        attempt = attempt + 1,
                        max_attempts = max_retries,
                        backoff_ms = backoff,
                        "Server error, retrying with backoff"
                    );
                    tokio::time::sleep(Duration::from_millis(backoff)).await;
                }
                Err(e) => return Err(e),
            }
        }

        Err(BotError::XrpcRateLimit(
            "Max retries exceeded for posting reply".to_string(),
        ))
    }

    /// Check if the pipeline is in dry-run mode
    pub fn is_dry_run(&self) -> bool {
        self.dry_run
    }

    /// Build a map of DIDs to handles from the thread and cache
    async fn build_identity_map(&self, thread: &[crate::db::models::Conversation]) -> Result<HashMap<String, String>> {
        let mut map = HashMap::new();

        for msg in thread {
            if !map.contains_key(&msg.author_did) {
                match self.repo.get_by_did(&msg.author_did).await? {
                    Some(identity) => map.insert(msg.author_did.clone(), identity.handle),
                    None => map.insert(msg.author_did.clone(), msg.author_did.clone()),
                };
            }
        }

        Ok(map)
    }
}

/// Calculate exponential backoff with jitter
fn calculate_backoff(attempt: usize, initial_ms: u64, max_ms: u64) -> u64 {
    let factor = 2_u64.saturating_pow(attempt as u32);
    let base = initial_ms.saturating_mul(factor);
    let capped = base.min(max_ms);
    let jitter = rand::rng().random_range(0..=1000_u64);
    capped.saturating_add(jitter)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backoff_calculation() {
        let b0 = calculate_backoff(0, 1000, 60000);
        assert!((1000..=2000).contains(&b0));

        let b1 = calculate_backoff(1, 1000, 60000);
        assert!((2000..=3000).contains(&b1));

        let b5 = calculate_backoff(5, 1000, 60000);
        assert!(b5 <= 60000 + 1000);
    }

    #[test]
    fn test_action_result_fields() {
        let result = ActionResult {
            post_uri: "at://did:plc:test/app.bsky.feed.post/123".to_string(),
            root_uri: "at://did:plc:test/app.bsky.feed.post/root".to_string(),
            response_text: Some("Hello!".to_string()),
            posted_reply_uri: Some("at://did:plc:bot/app.bsky.feed.post/reply".to_string()),
            silent: false,
            loop_prevented: false,
            error: None,
        };

        assert!(!result.silent);
        assert!(!result.loop_prevented);
        assert!(result.response_text.is_some());
        assert!(result.posted_reply_uri.is_some());
    }
}
