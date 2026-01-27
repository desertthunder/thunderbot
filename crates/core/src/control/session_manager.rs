//! Proactive session management and refresh.
use crate::bsky::BskyClient;
use crate::db::DatabaseRepository;

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Session manager with proactive refresh capability.
pub struct SessionManager {
    bsky_client: Arc<BskyClient>,
    db: Arc<dyn DatabaseRepository>,
    refresh_enabled: Arc<RwLock<bool>>,
}

impl SessionManager {
    /// Create a new session manager.
    pub fn new(bsky_client: Arc<BskyClient>, db: Arc<dyn DatabaseRepository>) -> Self {
        Self { bsky_client, db, refresh_enabled: Arc::new(RwLock::new(true)) }
    }

    /// Start proactive refresh background task.
    pub async fn start_proactive_refresh(&self) {
        let manager = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(300));

            loop {
                interval.tick().await;

                if let Ok(Some(metadata)) = manager.db.get_session_metadata(&manager.get_did().await).await {
                    let now = Utc::now();
                    let should_refresh = if let Some(force_before) = metadata.force_refresh_before {
                        now >= force_before || (now + Duration::minutes(15) >= metadata.access_jwt_expires_at)
                    } else {
                        now + Duration::minutes(15) >= metadata.access_jwt_expires_at
                    };

                    if should_refresh {
                        tracing::info!("Proactive session refresh triggered");
                        if let Err(e) = manager.force_refresh().await {
                            tracing::warn!("Proactive refresh failed: {}", e);
                        }
                    }
                }
            }
        });
    }

    /// Force an immediate session refresh.
    pub async fn force_refresh(&self) -> Result<()> {
        let session = self.bsky_client.refresh_session().await?;
        let now = Utc::now();
        let access_expires = now + Duration::hours(1);
        let refresh_expires = now + Duration::days(30);

        let metadata = crate::control::SessionMetadata {
            did: session.did.clone(),
            access_jwt_expires_at: access_expires,
            refresh_jwt_expires_at: refresh_expires,
            last_refresh_at: Some(now),
            force_refresh_before: Some(now + Duration::minutes(45)),
        };

        self.db.save_session_metadata(metadata).await?;
        tracing::info!("Session refreshed and metadata saved");

        Ok(())
    }

    /// Get session info for display.
    pub async fn get_session_info(&self) -> SessionInfo {
        if let Some(session) = self.bsky_client.get_session().await {
            let metadata = self.db.get_session_metadata(&session.did).await.ok().flatten();

            let expires_in = metadata
                .as_ref()
                .map(|m| (m.access_jwt_expires_at - Utc::now()).num_seconds().max(0));

            SessionInfo {
                did: session.did,
                handle: session.handle,
                expires_in,
                last_refresh: metadata.as_ref().and_then(|m| m.last_refresh_at),
            }
        } else {
            SessionInfo {
                did: "unknown".to_string(),
                handle: "unknown".to_string(),
                expires_in: None,
                last_refresh: None,
            }
        }
    }

    /// Get the current DID.
    async fn get_did(&self) -> String {
        if let Some(session) = self.bsky_client.get_session().await {
            session.did
        } else {
            "unknown".to_string()
        }
    }
}

impl Clone for SessionManager {
    fn clone(&self) -> Self {
        Self {
            bsky_client: Arc::clone(&self.bsky_client),
            db: Arc::clone(&self.db),
            refresh_enabled: Arc::clone(&self.refresh_enabled),
        }
    }
}

/// Session information for display.
#[derive(Clone, Debug)]
pub struct SessionInfo {
    pub did: String,
    pub handle: String,
    /// Seconds until expiration (None if unknown)
    pub expires_in: Option<i64>,
    pub last_refresh: Option<DateTime<Utc>>,
}
