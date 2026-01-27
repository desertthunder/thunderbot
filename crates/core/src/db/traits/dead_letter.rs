//! Dead letter queue repository operations.

use crate::control::DeadLetterItem;
use anyhow::Result;
use async_trait::async_trait;

/// Repository for dead letter queue management.
#[async_trait]
pub trait DeadLetterRepository: Send + Sync {
    /// Add a failed event to the DLQ.
    async fn add_to_dlq(&self, item: DeadLetterItem) -> Result<()>;

    /// Get DLQ items, most recent first.
    async fn get_dlq_items(&self, limit: usize) -> Result<Vec<DeadLetterItem>>;

    /// Get a specific DLQ item by ID.
    async fn get_dlq_item(&self, id: &str) -> Result<DeadLetterItem>;

    /// Remove an item from the DLQ.
    async fn remove_from_dlq(&self, id: &str) -> Result<()>;

    /// Delete all items from the DLQ.
    async fn purge_dlq(&self) -> Result<()>;

    /// Delete items older than specified days. Returns count purged.
    async fn purge_old_dlq_items(&self, days: i64) -> Result<u64>;
}
