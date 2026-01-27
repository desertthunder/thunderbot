//! Session metadata repository operations.

use crate::control::SessionMetadata;
use anyhow::Result;
use async_trait::async_trait;

/// Repository for session metadata and proactive refresh tracking.
#[async_trait]
pub trait SessionMetadataRepository: Send + Sync {
    /// Save session metadata (expiration times, refresh tracking).
    async fn save_session_metadata(&self, metadata: SessionMetadata) -> Result<()>;

    /// Get session metadata for a DID.
    async fn get_session_metadata(&self, did: &str) -> Result<Option<SessionMetadata>>;
}
