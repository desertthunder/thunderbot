//! Reply limits repository operations.

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use crate::control::ReplyLimitsConfig;

/// Repository for reply limits configuration and tracking.
#[async_trait]
pub trait ReplyLimitsRepository: Send + Sync {
    /// Get current reply limits configuration.
    async fn get_reply_limits_config(&self) -> Result<ReplyLimitsConfig>;

    /// Update reply limits configuration.
    async fn update_reply_limits_config(&self, config: ReplyLimitsConfig) -> Result<()>;

    /// Count bot replies in a specific thread.
    async fn count_replies_in_thread(&self, thread_uri: &str) -> Result<i64>;

    /// Count bot replies to a specific author in the last hour.
    async fn count_replies_by_author_last_hour(&self, author_did: &str) -> Result<i64>;

    /// Get timestamp of last reply to a specific author.
    async fn get_last_reply_time(&self, author_did: &str) -> Result<Option<DateTime<Utc>>>;
}
