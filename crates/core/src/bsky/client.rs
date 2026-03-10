//! Bluesky XRPC Client implementation
//!
//! Manual XRPC client with:
//! - Session management (create/refresh)
//! - Automatic token renewal
//! - Error handling (rate limits, auth failures)
//! - Post creation with proper threading

use crate::bsky::models::*;
use crate::bsky::session::Session;
use crate::error::{BotError, Result, XrpcErrorResponse};
use rand::RngExt;
use reqwest::StatusCode;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

const MAX_TRANSIENT_RETRIES: usize = 3;
const INITIAL_BACKOFF_MS: u64 = 250;

/// Bluesky XRPC Client
///
/// Provides methods for authentication, posting, and identity resolution.
/// Automatically handles session refresh before token expiry.
#[derive(Clone)]
pub struct BskyClient {
    http: reqwest::Client,
    pds_host: String,
    session: Arc<RwLock<Option<Session>>>,
    credentials: Option<Credentials>,
}

/// Credentials for authentication
#[derive(Debug, Clone)]
struct Credentials {
    handle: String,
    app_password: String,
}

impl BskyClient {
    /// Create a new unauthenticated client
    pub fn new(pds_host: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            pds_host: pds_host.into(),
            session: Arc::new(RwLock::new(None)),
            credentials: None,
        }
    }

    /// Create a new client with credentials for automatic authentication
    pub fn with_credentials(
        pds_host: impl Into<String>, handle: impl Into<String>, app_password: impl Into<String>,
    ) -> Self {
        Self {
            http: reqwest::Client::new(),
            pds_host: pds_host.into(),
            session: Arc::new(RwLock::new(None)),
            credentials: Some(Credentials { handle: handle.into(), app_password: app_password.into() }),
        }
    }

    /// Check if the client has an active session
    pub async fn is_authenticated(&self) -> bool {
        let session = self.session.read().await;
        session.is_some()
    }

    /// Get current session info (if authenticated)
    pub async fn get_session(&self) -> Option<Session> {
        self.session.read().await.clone()
    }

    /// Replace the currently stored session.
    pub async fn set_session(&self, session: Session) {
        let mut stored = self.session.write().await;
        *stored = Some(session);
    }

    /// Authenticate with handle and app password
    ///
    /// Calls `com.atproto.server.createSession` and stores the session tokens.
    pub async fn login(&self, handle: &str, app_password: &str) -> Result<Session> {
        let url = format!("{}/xrpc/com.atproto.server.createSession", self.pds_host);

        let request = CreateSessionRequest { identifier: handle.to_string(), password: app_password.to_string() };
        let response = self.http.post(&url).json(&request).send().await?;
        let status = response.status();

        if !status.is_success() {
            let error_response = response.json::<XrpcErrorResponse>().await.ok();
            return Err(BotError::from_xrpc_status(status, error_response));
        }

        let create_response: CreateSessionResponse = response.json().await?;
        let session = Session::from_create_response(create_response)?;

        let mut stored_session = self.session.write().await;
        *stored_session = Some(session.clone());

        tracing::info!("Successfully authenticated as {} ({})", session.handle, session.did);

        Ok(session)
    }

    /// Refresh the current session
    ///
    /// Calls `com.atproto.server.refreshSession` with the refresh token.
    pub async fn refresh_session(&self) -> Result<Session> {
        let refresh_jwt = {
            let session = self.session.read().await;
            match session.as_ref() {
                Some(s) => s.refresh_jwt.clone(),
                None => return Err(BotError::SessionExpired),
            }
        };

        let url = format!("{}/xrpc/com.atproto.server.refreshSession", self.pds_host);

        let response = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", refresh_jwt))
            .send()
            .await?;

        let status = response.status();

        if status == StatusCode::UNAUTHORIZED {
            tracing::warn!("Refresh token expired, need to re-authenticate");

            let mut session = self.session.write().await;
            *session = None;

            return Err(BotError::SessionExpired);
        }

        if !status.is_success() {
            let error_response = response.json::<XrpcErrorResponse>().await.ok();
            return Err(BotError::from_xrpc_status(status, error_response));
        }

        let refresh_response: RefreshSessionResponse = response.json().await?;

        let mut session = self.session.write().await;

        match session.as_mut() {
            Some(ref mut s) => {
                s.update_from_refresh(refresh_response.access_jwt, refresh_response.refresh_jwt)?;
                tracing::debug!("Session refreshed successfully");
                Ok(s.clone())
            }
            None => Err(BotError::SessionExpired),
        }
    }

    /// Ensure we have a valid session, refreshing if necessary
    ///
    /// This should be called before any authenticated request.
    pub async fn ensure_valid_session(&self) -> Result<Session> {
        let should_refresh = {
            let session = self.session.read().await;
            match session.as_ref() {
                None => match self.credentials {
                    Some(_) => false,
                    None => {
                        return Err(BotError::XrpcAuthentication("No active session".to_string()));
                    }
                },
                Some(s) => s.is_expiring(60),
            }
        };

        if should_refresh {
            match self.refresh_session().await {
                Ok(session) => return Ok(session),
                Err(BotError::SessionExpired) => {}
                Err(e) => return Err(e),
            }
        }

        let needs_login = {
            let session = self.session.read().await;
            session.is_none() && self.credentials.is_some()
        };

        if needs_login && let Some(creds) = &self.credentials {
            return self.login(&creds.handle, &creds.app_password).await;
        }

        let session = self.session.read().await;
        session.clone().ok_or(BotError::SessionExpired)
    }

    /// Load a serialized session from disk if present.
    pub async fn load_session_from_file(&self, path: impl AsRef<Path>) -> Result<Option<Session>> {
        let path = path.as_ref();
        let contents = match tokio::fs::read_to_string(path).await {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(BotError::Io(e)),
        };

        let session: Session = serde_json::from_str(&contents)?;
        self.set_session(session.clone()).await;
        Ok(Some(session))
    }

    /// Persist the current session to disk for reuse by subsequent CLI commands.
    pub async fn save_session_to_file(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let session = self.session.read().await.clone().ok_or(BotError::SessionExpired)?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let serialized = serde_json::to_string_pretty(&session)?;
        tokio::fs::write(path, serialized).await?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).await?;
        }

        Ok(())
    }

    /// Logout and clear the session
    pub async fn logout(&self) {
        let mut session = self.session.write().await;
        if session.is_some() {
            tracing::info!("Logging out and clearing session");
            *session = None;
        }
    }

    /// Create a record (e.g., a post)
    ///
    /// Calls `com.atproto.repo.createRecord`.
    pub async fn create_record(&self, collection: &str, record: serde_json::Value) -> Result<CreateRecordResponse> {
        let session = self.ensure_valid_session().await?;

        let url = format!("{}/xrpc/com.atproto.repo.createRecord", self.pds_host);

        let request = CreateRecordRequest { repo: session.did.clone(), collection: collection.to_string(), record };

        for attempt in 0..MAX_TRANSIENT_RETRIES {
            let response = self
                .http
                .post(&url)
                .header("Authorization", session.auth_header())
                .json(&request)
                .send()
                .await?;

            let status = response.status();

            if status == StatusCode::UNAUTHORIZED {
                self.refresh_session().await?;
                let refreshed = self.ensure_valid_session().await?;
                let retry_response = self
                    .http
                    .post(&url)
                    .header("Authorization", refreshed.auth_header())
                    .json(&request)
                    .send()
                    .await?;

                let retry_status = retry_response.status();
                if !retry_status.is_success() {
                    let error_response = retry_response.json::<XrpcErrorResponse>().await.ok();
                    return Err(BotError::from_xrpc_status(retry_status, error_response));
                }

                return retry_response.json().await.map_err(|e| e.into());
            }

            if status.is_success() {
                return response.json().await.map_err(|e| e.into());
            }

            if (status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error())
                && attempt + 1 < MAX_TRANSIENT_RETRIES
            {
                sleep_with_jitter_backoff(attempt).await;
                continue;
            }

            let error_response = response.json::<XrpcErrorResponse>().await.ok();
            return Err(BotError::from_xrpc_status(status, error_response));
        }

        Err(BotError::XrpcHttp(
            "create_record retry loop exhausted unexpectedly".to_string(),
        ))
    }

    /// Get a record by URI
    ///
    /// Calls `com.atproto.repo.getRecord`.
    pub async fn get_record(&self, repo: &str, collection: &str, rkey: &str) -> Result<GetRecordResponse> {
        let url = format!(
            "{}/xrpc/com.atproto.repo.getRecord?repo={}&collection={}&rkey={}",
            self.pds_host,
            urlencoding::encode(repo),
            urlencoding::encode(collection),
            urlencoding::encode(rkey)
        );

        let response = self.http.get(&url).send().await?;
        let status = response.status();

        if !status.is_success() {
            let error_response = response.json::<XrpcErrorResponse>().await.ok();
            return Err(BotError::from_xrpc_status(status, error_response));
        }

        response.json().await.map_err(|e| e.into())
    }

    /// Get a record by AT URI
    pub async fn get_record_by_uri(&self, uri: &str) -> Result<GetRecordResponse> {
        let at_uri =
            AtUri::parse(uri).ok_or_else(|| BotError::XrpcInvalidRequest(format!("Invalid AT URI: {}", uri)))?;
        self.get_record(&at_uri.repo, &at_uri.collection, &at_uri.rkey).await
    }

    /// Create a new post
    pub async fn create_post(&self, text: impl Into<String>) -> Result<CreateRecordResponse> {
        let record = PostRecord::new(text);
        let record_json = serde_json::to_value(record)?;
        self.create_record("app.bsky.feed.post", record_json).await
    }

    /// Reply to a post
    ///
    /// Fetches the parent post to determine the root for proper threading.
    pub async fn reply_to(&self, parent_uri: &str, text: impl Into<String>) -> Result<CreateRecordResponse> {
        let parent = self.get_record_by_uri(parent_uri).await?;

        let root = if let Some(reply) = parent.value.get("reply") {
            if let Some(root_obj) = reply.get("root") {
                let root_uri = root_obj
                    .get("uri")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| BotError::XrpcInvalidRequest("Missing root.uri in parent".to_string()))?;
                let root_cid = root_obj
                    .get("cid")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| BotError::XrpcInvalidRequest("Missing root.cid in parent".to_string()))?;
                StrongRef { uri: root_uri.to_string(), cid: root_cid.to_string() }
            } else {
                StrongRef { uri: parent.uri.clone(), cid: parent.cid.clone() }
            }
        } else {
            StrongRef { uri: parent.uri.clone(), cid: parent.cid.clone() }
        };

        let parent_ref = StrongRef { uri: parent.uri, cid: parent.cid };

        let record = PostRecord::reply(text, root, parent_ref);
        let record_json = serde_json::to_value(record)?;

        self.create_record("app.bsky.feed.post", record_json).await
    }

    /// Resolve a handle to a DID
    ///
    /// Calls `com.atproto.identity.resolveHandle`.
    pub async fn resolve_handle(&self, handle: &str) -> Result<String> {
        let url = format!(
            "{}/xrpc/com.atproto.identity.resolveHandle?handle={}",
            self.pds_host,
            urlencoding::encode(handle)
        );

        let response = self.http.get(&url).send().await?;
        let status = response.status();

        if !status.is_success() {
            let error_response = response.json::<XrpcErrorResponse>().await.ok();
            return Err(BotError::from_xrpc_status(status, error_response));
        }

        let result: ResolveHandleResponse = response.json().await?;
        Ok(result.did)
    }

    /// Get a user's profile
    ///
    /// Calls `app.bsky.actor.getProfile`.
    pub async fn get_profile(&self, actor: &str) -> Result<GetProfileResponse> {
        let url = format!(
            "{}/xrpc/app.bsky.actor.getProfile?actor={}",
            self.pds_host,
            urlencoding::encode(actor)
        );

        let response = self.http.get(&url).send().await?;
        let status = response.status();

        if !status.is_success() {
            let error_response = response.json::<XrpcErrorResponse>().await.ok();
            return Err(BotError::from_xrpc_status(status, error_response));
        }

        response.json().await.map_err(|e| e.into())
    }
}

async fn sleep_with_jitter_backoff(attempt: usize) {
    let factor = 1_u64 << attempt;
    let base = INITIAL_BACKOFF_MS.saturating_mul(factor);
    let jitter_ms = rand::rng().random_range(0..=100_u64);
    tokio::time::sleep(Duration::from_millis(base.saturating_add(jitter_ms))).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use tempfile::TempDir;

    #[test]
    fn test_bsky_client_new() {
        let client = BskyClient::new("https://bsky.social");
        assert_eq!(client.pds_host, "https://bsky.social");
    }

    #[test]
    fn test_bsky_client_with_credentials() {
        let client = BskyClient::with_credentials("https://bsky.social", "test.bsky.social", "app-password-123");
        assert_eq!(client.pds_host, "https://bsky.social");
    }

    #[tokio::test]
    async fn test_save_and_load_session_from_file() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let session_path = temp_dir.path().join("session.json");

        let client = BskyClient::new("https://bsky.social");
        let session = Session {
            did: "did:plc:test".to_string(),
            handle: "test.bsky.social".to_string(),
            access_jwt: "access".to_string(),
            refresh_jwt: "refresh".to_string(),
            access_expires_at: Utc::now() + Duration::hours(2),
        };
        client.set_session(session.clone()).await;
        client
            .save_session_to_file(&session_path)
            .await
            .expect("session should be persisted");

        let restored_client = BskyClient::new("https://bsky.social");
        let restored = restored_client
            .load_session_from_file(&session_path)
            .await
            .expect("session should load")
            .expect("session should exist");

        assert_eq!(restored.did, session.did);
        assert_eq!(restored.handle, session.handle);
        assert_eq!(restored.access_jwt, session.access_jwt);
        assert_eq!(restored.refresh_jwt, session.refresh_jwt);
    }
}
