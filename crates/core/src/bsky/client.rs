use super::types::*;
use crate::db::{DatabaseRepository, SessionRow};

use anyhow::Context;
use chrono::Utc;
use reqwest::Client;
use std::sync::Arc;
use tokio::sync::RwLock;

const SESSION_FILE: &str = ".bsky_session.json";

#[derive(Clone)]
pub struct BskyClient {
    client: Client,
    pds_host: String,
    session: Arc<RwLock<Option<Session>>>,
    db: Option<Arc<dyn DatabaseRepository>>,
}

impl BskyClient {
    pub fn new(pds_host: &str, db: Option<Arc<dyn DatabaseRepository>>) -> Self {
        let session = Self::load_session_from_file_sync();

        Self { client: Client::new(), pds_host: pds_host.to_string(), session: Arc::new(RwLock::new(session)), db }
    }

    fn load_session_from_file_sync() -> Option<Session> {
        if let Ok(content) = std::fs::read_to_string(SESSION_FILE)
            && let Ok(session) = serde_json::from_str::<Session>(&content)
        {
            tracing::info!("Loaded session from file for: {}", session.handle);
            return Some(session);
        }
        None
    }

    pub async fn load_from_database(&self) -> Option<Session> {
        if let Some(session) = self.load_session_from_database().await {
            let mut session_guard = self.session.write().await;
            *session_guard = Some(session.clone());
            Some(session)
        } else {
            None
        }
    }

    async fn load_session_from_database(&self) -> Option<Session> {
        if let Some(db) = &self.db
            && let Ok(handle) = std::env::var("BSKY_HANDLE")
            && let Ok(did) = self.resolve_handle(&handle).await
            && let Ok(Some(session_row)) = db.get_session(&did).await
        {
            tracing::info!("Loaded session from database for: {}", session_row.handle);
            return Some(Session {
                did: session_row.did,
                handle: session_row.handle,
                access_jwt: session_row.access_jwt,
                refresh_jwt: session_row.refresh_jwt,
            });
        }
        None
    }

    async fn save_session(&self, session: &Session) -> anyhow::Result<()> {
        if let Some(db) = &self.db {
            let session_row = SessionRow {
                did: session.did.clone(),
                handle: session.handle.clone(),
                access_jwt: session.access_jwt.clone(),
                refresh_jwt: session.refresh_jwt.clone(),
                updated_at: Utc::now(),
            };

            if let Err(e) = db.save_session(session_row).await {
                tracing::warn!("Failed to save session to database: {}", e);
            } else {
                tracing::debug!("Saved session to database");
            }
        }

        let content = serde_json::to_string_pretty(session)?;
        std::fs::write(SESSION_FILE, content)?;
        tracing::debug!("Saved session to file (fallback)");
        Ok(())
    }

    pub async fn create_session(&self, identifier: &str, password: &str) -> anyhow::Result<Session> {
        let url = format!("{}/xrpc/com.atproto.server.createSession", self.pds_host);

        tracing::debug!("Creating session for identifier: {}", identifier);

        let response = self
            .client
            .post(&url)
            .json(&CreateSessionRequest { identifier: identifier.to_string(), password: password.to_string() })
            .send()
            .await?
            .error_for_status()?;

        let session_response: SessionResponse = response.json().await?;
        let session: Session = session_response.into();

        {
            let mut session_guard = self.session.write().await;
            *session_guard = Some(session.clone());
        }

        tracing::info!(
            "Session created successfully for: {} (DID: {})",
            session.handle,
            session.did
        );

        self.save_session(&session).await?;

        Ok(session)
    }

    pub async fn refresh_session(&self) -> anyhow::Result<Session> {
        let refresh_jwt = {
            let session_guard = self.session.read().await;
            session_guard
                .as_ref()
                .map(|s| s.refresh_jwt.clone())
                .ok_or_else(|| anyhow::anyhow!("No session to refresh"))?
        };

        let url = format!("{}/xrpc/com.atproto.server.refreshSession", self.pds_host);

        tracing::debug!("Refreshing session");

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", refresh_jwt))
            .send()
            .await?
            .error_for_status()?;

        let session_response: SessionResponse = response.json().await?;
        let session: Session = session_response.into();

        {
            let mut session_guard = self.session.write().await;
            *session_guard = Some(session.clone());
        }

        tracing::info!("Session refreshed successfully");

        self.save_session(&session).await?;

        Ok(session)
    }

    pub async fn get_session(&self) -> Option<Session> {
        let session_guard = self.session.read().await;
        session_guard.clone()
    }

    async fn auth_header(&self) -> anyhow::Result<String> {
        let session_guard = self.session.read().await;
        session_guard
            .as_ref()
            .map(|s| format!("Bearer {}", s.access_jwt))
            .ok_or_else(|| anyhow::anyhow!("Not authenticated"))
    }

    pub async fn create_post(&self, text: &str) -> anyhow::Result<CreateRecordResponse> {
        let session = {
            let session_guard = self.session.read().await;
            session_guard
                .as_ref()
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Not authenticated"))?
        };

        let url = format!("{}/xrpc/com.atproto.repo.createRecord", self.pds_host);

        let record = PostRecordWrite {
            record_type: "app.bsky.feed.post".to_string(),
            text: text.to_string(),
            created_at: Utc::now().to_rfc3339(),
            reply: None,
        };

        let request = CreateRecordRequest {
            repo: session.did.clone(),
            collection: "app.bsky.feed.post".to_string(),
            record: serde_json::to_value(record)?,
        };

        tracing::debug!("Creating post: {}", text);

        let response = self
            .client
            .post(&url)
            .header("Authorization", self.auth_header().await?)
            .json(&request)
            .send()
            .await?
            .error_for_status()
            .with_context(|| "Failed to create post")?;

        let result: CreateRecordResponse = response.json().await?;

        tracing::info!("Post created: {}", result.uri);

        Ok(result)
    }

    pub async fn reply_to_post(
        &self, text: &str, parent_uri: &str, parent_cid: &str, root_uri: &str, root_cid: &str,
    ) -> anyhow::Result<CreateRecordResponse> {
        let session = {
            let session_guard = self.session.read().await;
            session_guard
                .as_ref()
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Not authenticated"))?
        };

        let url = format!("{}/xrpc/com.atproto.repo.createRecord", self.pds_host);

        let record = PostRecordWrite {
            record_type: "app.bsky.feed.post".to_string(),
            text: text.to_string(),
            created_at: Utc::now().to_rfc3339(),
            reply: Some(ReplyRefWrite {
                root: StrongRefWrite { uri: root_uri.to_string(), cid: root_cid.to_string() },
                parent: StrongRefWrite { uri: parent_uri.to_string(), cid: parent_cid.to_string() },
            }),
        };

        let request = CreateRecordRequest {
            repo: session.did.clone(),
            collection: "app.bsky.feed.post".to_string(),
            record: serde_json::to_value(record)?,
        };

        tracing::debug!("Replying to post: {}", parent_uri);

        let response = self
            .client
            .post(&url)
            .header("Authorization", self.auth_header().await?)
            .json(&request)
            .send()
            .await?
            .error_for_status()
            .with_context(|| "Failed to reply to post")?;

        let result: CreateRecordResponse = response.json().await?;

        tracing::info!("Reply created: {}", result.uri);

        Ok(result)
    }

    pub async fn get_post(&self, uri: &str) -> anyhow::Result<GetRecordResponse> {
        let parts: Vec<&str> = uri
            .strip_prefix("at://")
            .ok_or_else(|| anyhow::anyhow!("Invalid URI: must start with at://"))?
            .split('/')
            .collect();

        let repo = parts
            .first()
            .ok_or_else(|| anyhow::anyhow!("Invalid URI: missing repo"))?;
        let collection = parts
            .get(1)
            .ok_or_else(|| anyhow::anyhow!("Invalid URI: missing collection"))?;
        let rkey = parts
            .get(2)
            .ok_or_else(|| anyhow::anyhow!("Invalid URI: missing rkey"))?;

        let url = format!(
            "{}/xrpc/com.atproto.repo.getRecord?repo={}&collection={}&rkey={}",
            self.pds_host, repo, collection, rkey
        );

        tracing::debug!("Fetching post: {}", uri);

        let response = self
            .client
            .get(&url)
            .header("Authorization", self.auth_header().await?)
            .send()
            .await?
            .error_for_status()
            .with_context(|| "Failed to get post")?;

        Ok(response.json().await?)
    }

    pub async fn resolve_handle(&self, handle: &str) -> anyhow::Result<String> {
        let url = format!(
            "{}/xrpc/com.atproto.identity.resolveHandle?handle={}",
            self.pds_host, handle
        );

        tracing::debug!("Resolving handle: {}", handle);

        let response: ResolveHandleResponse = self
            .client
            .get(&url)
            .send()
            .await?
            .error_for_status()
            .with_context(|| format!("Failed to resolve handle: {}", handle))?
            .json()
            .await?;

        let did = response.did.clone();

        if let Some(db) = &self.db
            && let Err(e) = db.cache_identity(&did, handle).await
        {
            tracing::warn!("Failed to cache identity: {}", e);
        }

        Ok(did)
    }

    pub async fn get_profile(&self, did: &str) -> anyhow::Result<GetProfileResponse> {
        let url = format!("{}/xrpc/app.bsky.actor.getProfile?actor={}", self.pds_host, did);

        tracing::debug!("Fetching profile: {}", did);

        let response = self
            .client
            .get(&url)
            .header("Authorization", self.auth_header().await?)
            .send()
            .await?
            .error_for_status()
            .with_context(|| format!("Failed to get profile: {}", did))?;

        Ok(response.json().await?)
    }
}
