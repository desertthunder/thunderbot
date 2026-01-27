//! Blocklist repository operations.

use anyhow::Result;
use async_trait::async_trait;
use crate::control::BlocklistEntry;

/// Repository for blocklist management.
#[async_trait]
pub trait BlocklistRepository: Send + Sync {
    /// Get all blocklist entries.
    async fn get_blocklist(&self) -> Result<Vec<BlocklistEntry>>;

    /// Add an entry to the blocklist.
    async fn add_to_blocklist(&self, entry: BlocklistEntry) -> Result<()>;

    /// Remove an entry from the blocklist.
    async fn remove_from_blocklist(&self, did: &str) -> Result<()>;

    /// Check if a DID is blocked (including expired entries check).
    async fn is_blocked(&self, did: &str) -> Result<bool>;
}
