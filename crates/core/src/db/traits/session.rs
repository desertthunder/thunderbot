//! Bluesky session repository operations.

use crate::db::types::SessionRow;
use anyhow::Result;
use async_trait::async_trait;

/// Repository for Bluesky session management.
#[async_trait]
pub trait SessionRepository: Send + Sync {
    /// Save or update a session token.
    async fn save_session(&self, row: SessionRow) -> Result<()>;

    /// Retrieve session by DID.
    async fn get_session(&self, did: &str) -> Result<Option<SessionRow>>;
}
