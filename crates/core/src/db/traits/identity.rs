//! Identity (DID to handle) repository operations.

use anyhow::Result;
use async_trait::async_trait;
use crate::db::types::IdentityRow;

/// Repository for identity resolution and caching.
#[async_trait]
pub trait IdentityRepository: Send + Sync {
    /// Save or update an identity record.
    async fn save_identity(&self, row: IdentityRow) -> Result<()>;

    /// Quick cache update for DID -> handle mapping.
    async fn cache_identity(&self, did: &str, handle: &str) -> Result<()>;

    /// Look up identity by DID.
    async fn get_identity(&self, did: &str) -> Result<Option<IdentityRow>>;

    /// Get all cached identities.
    async fn get_all_identities(&self) -> Result<Vec<IdentityRow>>;
}
