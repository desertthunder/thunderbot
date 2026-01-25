use crate::bsky::types::*;
use crate::web::cookies::UserSession;
use anyhow::{Context, Result};
use chrono::Utc;
use reqwest::Client;

pub struct ReplyContext {
    pub text: String,
    pub parent_uri: String,
    pub parent_cid: String,
    pub root_uri: String,
    pub root_cid: String,
    pub bot_did: String,
    pub bot_handle: String,
}

pub struct UserClient {
    client: Client,
    pds_host: String,
    session: UserSession,
}

impl UserClient {
    pub fn new(pds_host: String, session: UserSession) -> Self {
        Self { client: Client::new(), pds_host, session }
    }

    pub async fn ensure_valid_session(&mut self) -> Result<()> {
        if self.session.is_expired() {
            let url = format!("{}/xrpc/com.atproto.server.refreshSession", self.pds_host);

            tracing::debug!("Refreshing user session for: {}", self.session.handle);

            let response = self
                .client
                .post(&url)
                .header("Authorization", format!("Bearer {}", self.session.refresh_jwt))
                .send()
                .await?
                .error_for_status()
                .context("Failed to refresh session")?;

            let session_response: SessionResponse =
                response.json().await.context("Failed to parse session response")?;

            self.session.access_jwt = session_response.access_jwt;
            self.session.refresh_jwt = session_response.refresh_jwt;
            self.session.exp = Utc::now().timestamp() + 7200;

            tracing::info!("User session refreshed for: {}", self.session.handle);
        }

        Ok(())
    }

    pub fn create_mention_facets(text: &str, bot_did: &str, bot_handle: &str) -> Vec<Facet> {
        let mention_text = format!("@{}", bot_handle);
        if let Some(start) = text.find(&mention_text) {
            let end = start + mention_text.len();
            vec![Facet::mention(bot_did.to_string(), start, end)]
        } else {
            vec![]
        }
    }

    pub async fn create_post(&mut self, text: &str, bot_did: &str, bot_handle: &str) -> Result<CreateRecordResponse> {
        self.ensure_valid_session().await?;

        let mention_text = format!("@{} {}", bot_handle, text.trim());
        let facets = Self::create_mention_facets(&mention_text, bot_did, bot_handle);

        let mut record = PostRecordWrite::new(mention_text, Utc::now().to_rfc3339());
        record.facets = facets;

        let request = CreateRecordRequest {
            repo: self.session.did.clone(),
            collection: "app.bsky.feed.post".to_string(),
            record: serde_json::to_value(&record).context("Failed to serialize post record")?,
        };

        let url = format!("{}/xrpc/com.atproto.repo.createRecord", self.pds_host);

        tracing::debug!("Creating post as user {}: {}", self.session.handle, text);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.session.access_jwt))
            .json(&request)
            .send()
            .await?
            .error_for_status()
            .context("Failed to create post")?;

        let result: CreateRecordResponse = response.json().await.context("Failed to parse response")?;

        tracing::info!("Post created as user {}: {}", self.session.handle, result.uri);

        Ok(result)
    }

    pub async fn create_reply(&mut self, reply_context: &ReplyContext) -> Result<CreateRecordResponse> {
        self.ensure_valid_session().await?;

        let mention_text = format!("@{} {}", reply_context.bot_handle, reply_context.text.trim());
        let facets = Self::create_mention_facets(&mention_text, &reply_context.bot_did, &reply_context.bot_handle);

        let mut record = PostRecordWrite::new(mention_text, Utc::now().to_rfc3339());
        record.facets = facets;
        record.reply = Some(ReplyRefWrite {
            root: StrongRefWrite { uri: reply_context.root_uri.clone(), cid: reply_context.root_cid.clone() },
            parent: StrongRefWrite { uri: reply_context.parent_uri.clone(), cid: reply_context.parent_cid.clone() },
        });

        let request = CreateRecordRequest {
            repo: self.session.did.clone(),
            collection: "app.bsky.feed.post".to_string(),
            record: serde_json::to_value(&record).context("Failed to serialize reply record")?,
        };

        let url = format!("{}/xrpc/com.atproto.repo.createRecord", self.pds_host);

        tracing::debug!("Creating reply as user {}: {}", self.session.handle, reply_context.text);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.session.access_jwt))
            .json(&request)
            .send()
            .await?
            .error_for_status()
            .context("Failed to create reply")?;

        let result: CreateRecordResponse = response.json().await.context("Failed to parse response")?;

        tracing::info!("Reply created as user {}: {}", self.session.handle, result.uri);

        Ok(result)
    }

    pub fn session(&self) -> &UserSession {
        &self.session
    }
}
