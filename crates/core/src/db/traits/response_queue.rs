//! Response preview queue repository operations.

use crate::control::{ResponseQueueItem, ResponseStatus};
use anyhow::Result;
use async_trait::async_trait;

/// Repository for response preview queue management.
#[async_trait]
pub trait ResponseQueueRepository: Send + Sync {
    /// Add a response to the approval queue.
    async fn queue_response(&self, item: ResponseQueueItem) -> Result<()>;

    /// Get all pending responses awaiting approval.
    async fn get_pending_responses(&self) -> Result<Vec<ResponseQueueItem>>;

    /// Get a specific queued response by ID.
    async fn get_response_item(&self, id: &str) -> Result<ResponseQueueItem>;

    /// Update response status (approve/discard/etc).
    async fn update_response_status(&self, id: &str, status: ResponseStatus) -> Result<()>;

    /// Update response content (marks as edited).
    async fn update_response_content(&self, id: &str, content: &str) -> Result<()>;
}
