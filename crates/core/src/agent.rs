use super::bsky::BskyClient;
use super::db::{ConversationRow, Db, IdentityResolver, IdentityResolverConfig, ThreadContextBuilder};
use super::gemini::{GeminiClient, PromptBuilder};
use crate::control::{PolicyEnforcer, ResponseQueueItem, ResponseStatus};
use crate::db::types::ActivityLogRow;

use anyhow::{Context, Result};
use chrono::Utc;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

pub struct Agent {
    gemini_client: GeminiClient,
    bsky_client: Arc<BskyClient>,
    db: Db,
    own_did: String,
    system_instruction: Option<String>,
    rag_retriever: Option<Arc<crate::SemanticRetriever>>,
    dry_run: bool,
    /// Policy enforcer for quiet hours and reply limits
    pub policy_enforcer: Arc<PolicyEnforcer>,
    /// Preview mode flag
    preview_mode: Arc<RwLock<bool>>,
}

impl Agent {
    pub fn new(
        gemini_client: GeminiClient, bsky_client: Arc<BskyClient>, db: Db, own_did: String,
        system_instruction: Option<String>,
    ) -> Self {
        let policy_enforcer = Arc::new(PolicyEnforcer::new(db.clone()));
        Self {
            gemini_client,
            bsky_client,
            db,
            own_did,
            system_instruction,
            rag_retriever: None,
            dry_run: false,
            policy_enforcer,
            preview_mode: Arc::new(RwLock::new(false)),
        }
    }

    pub fn from_clients(
        bsky_client: Arc<BskyClient>, db: Db, own_did: String, system_instruction: Option<String>,
    ) -> Result<Self> {
        let gemini_client = GeminiClient::from_env()?;
        let policy_enforcer = Arc::new(PolicyEnforcer::new(db.clone()));

        Ok(Self {
            gemini_client,
            bsky_client,
            db,
            own_did,
            system_instruction,
            rag_retriever: None,
            dry_run: false,
            policy_enforcer,
            preview_mode: Arc::new(RwLock::new(false)),
        })
    }

    pub fn with_rag(mut self, retriever: Arc<crate::SemanticRetriever>) -> Self {
        self.rag_retriever = Some(retriever);
        self
    }

    pub fn with_dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    pub fn is_dry_run(&self) -> bool {
        self.dry_run
    }

    pub async fn set_preview_mode(&self, enabled: bool) {
        let mut mode = self.preview_mode.write().await;
        *mode = enabled;
    }

    pub async fn is_preview_mode(&self) -> bool {
        *self.preview_mode.read().await
    }

    pub async fn process_mention(&self, post_uri: &str, _: &str) -> Result<()> {
        let post = self
            .bsky_client
            .get_post(post_uri)
            .await
            .context("Failed to fetch post")?;

        let post_record = post.value;
        let author_did = post_record
            .get("author")
            .and_then(|a| a.get("did"))
            .and_then(|d| d.as_str())
            .ok_or_else(|| anyhow::anyhow!("Post missing author"))?;

        if author_did == self.own_did {
            tracing::debug!("Skipping reply to own post");
            return Ok(());
        }

        if self.db.is_blocked(author_did).await.unwrap_or(false) {
            tracing::info!("Skipping reply to blocked author: {}", author_did);
            return Ok(());
        }

        let parent_cid = post_record
            .get("cid")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Post missing cid"))?;

        let root_uri = post_record
            .get("reply")
            .and_then(|r| r.get("root"))
            .and_then(|r| r.get("uri"))
            .and_then(|u| u.as_str())
            .unwrap_or(post_uri);

        let root_cid = post_record
            .get("reply")
            .and_then(|r| r.get("root"))
            .and_then(|r| r.get("cid"))
            .and_then(|c| c.as_str())
            .unwrap_or(parent_cid);

        let is_quiet_hours = !self.policy_enforcer.can_post_now().await.unwrap_or(true);
        let is_preview_mode = self.is_preview_mode().await || is_quiet_hours;

        if !is_preview_mode {
            if let Err(e) = self.policy_enforcer.can_reply_to_thread(root_uri, author_did).await {
                tracing::warn!("Reply limits check failed: {}", e);
                return Err(e);
            }

            if !self.policy_enforcer.can_reply_to_thread(root_uri, author_did).await? {
                tracing::info!("Reply limits prevent response to thread {}", root_uri);
                return Ok(());
            }
        }

        let thread_builder = ThreadContextBuilder::new(self.db.clone());
        let identity_resolver = IdentityResolver::new(self.db.clone(), IdentityResolverConfig::default());
        let mut prompt_builder = PromptBuilder::new(thread_builder, identity_resolver, self.system_instruction.clone());

        if let Some(retriever) = &self.rag_retriever {
            prompt_builder = prompt_builder.with_rag(retriever.clone());
        }

        let prompt = prompt_builder
            .build_for_thread(root_uri)
            .await
            .context("Failed to build prompt")?;

        let gemini_request = prompt_builder
            .to_gemini_request(&prompt)
            .context("Failed to convert prompt to Gemini request")?;

        tracing::debug!("Sending prompt to Gemini...");
        let response = self
            .gemini_client
            .generate_content(gemini_request)
            .await
            .context("Failed to generate content")?;

        if response.trim() == "<SILENT_THOUGHT>" {
            tracing::info!("Silent mode: skipping response");
            return Ok(());
        }

        if is_preview_mode {
            tracing::info!("Preview mode active: queuing response for approval");
            let queue_item = ResponseQueueItem {
                id: Uuid::new_v4().to_string(),
                thread_uri: root_uri.to_string(),
                parent_uri: post_uri.to_string(),
                parent_cid: parent_cid.to_string(),
                root_uri: root_uri.to_string(),
                root_cid: root_cid.to_string(),
                content: response.clone(),
                status: ResponseStatus::Pending,
                created_at: Utc::now(),
                expires_at: None,
            };

            self.db.queue_response(queue_item).await?;
            return Ok(());
        }

        if self.dry_run {
            tracing::info!("[DRY-RUN] Would post reply: {}", response);
            return Ok(());
        }

        tracing::debug!("Posting reply to Bluesky...");
        self.post_with_retry(&response, post_uri, parent_cid, root_uri, root_cid)
            .await?;

        let activity = ActivityLogRow {
            id: Uuid::new_v4().to_string(),
            action_type: "reply".to_string(),
            description: format!("Replied to mention: {}", &response.chars().take(50).collect::<String>()),
            thread_uri: Some(root_uri.to_string()),
            metadata_json: Some(
                serde_json::json!({"post_uri": post_uri, "response_length": response.len()}).to_string(),
            ),
            created_at: Utc::now(),
        };
        if let Err(e) = self.db.log_activity(activity).await {
            tracing::warn!("Failed to log activity: {}", e);
        }

        let bot_conversation = ConversationRow {
            id: Uuid::new_v4().to_string(),
            thread_root_uri: root_uri.to_string(),
            post_uri: String::new(),
            parent_uri: Some(post_uri.to_string()),
            author_did: self.own_did.clone(),
            role: "model".to_string(),
            content: response.clone(),
            created_at: Utc::now(),
        };

        if let Err(e) = self.db.save_conversation(bot_conversation).await {
            tracing::warn!("Failed to save bot response to database: {}", e);
        }

        tracing::info!("Successfully processed mention and posted response");

        Ok(())
    }

    pub async fn simulate_response(&self, post_uri: &str) -> Result<String> {
        let thread_builder = ThreadContextBuilder::new(self.db.clone());
        let identity_resolver = IdentityResolver::new(self.db.clone(), IdentityResolverConfig::default());
        let mut prompt_builder = PromptBuilder::new(thread_builder, identity_resolver, self.system_instruction.clone());

        if let Some(retriever) = &self.rag_retriever {
            prompt_builder = prompt_builder.with_rag(retriever.clone());
        }

        let post = self
            .bsky_client
            .get_post(post_uri)
            .await
            .context("Failed to fetch post")?;

        let post_record = post.value;
        let root_uri = post_record
            .get("reply")
            .and_then(|r| r.get("root"))
            .and_then(|r| r.get("uri"))
            .and_then(|u| u.as_str())
            .unwrap_or(post_uri);

        let prompt = prompt_builder
            .build_for_thread(root_uri)
            .await
            .context("Failed to build prompt")?;

        if prompt.history.is_empty() {
            return self
                .one_shot_prompt("Please respond to this post. How can I help?")
                .await;
        }

        let gemini_request = prompt_builder
            .to_gemini_request(&prompt)
            .context("Failed to convert prompt to Gemini request")?;

        let response = self
            .gemini_client
            .generate_content(gemini_request)
            .await
            .context("Failed to generate content")?;

        Ok(response)
    }

    pub async fn one_shot_prompt(&self, text: &str) -> Result<String> {
        let response = self
            .gemini_client
            .prompt(text, self.system_instruction.clone())
            .await
            .context("Failed to generate content")?;

        Ok(response)
    }

    async fn post_with_retry(
        &self, text: &str, parent_uri: &str, parent_cid: &str, root_uri: &str, root_cid: &str,
    ) -> Result<()> {
        let max_retries = 3;
        let mut last_error = None;

        for attempt in 1..=max_retries {
            if attempt > 1 {
                let backoff = std::time::Duration::from_millis(2000 * 2u64.pow(attempt as u32 - 1));
                tracing::warn!("Rate limited, retry attempt {} after {:?}", attempt, backoff);
                tokio::time::sleep(backoff).await;
            }

            match self
                .bsky_client
                .reply_to_post(text, parent_uri, parent_cid, root_uri, root_cid)
                .await
            {
                Ok(_) => return Ok(()),
                Err(e) => {
                    let error_str = e.to_string();
                    if error_str.contains("429") || error_str.contains("rate limit") {
                        last_error = Some(e);
                    } else {
                        return Err(e);
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Max retries exceeded")))
    }

    pub fn get_own_did(&self) -> &str {
        &self.own_did
    }
}
