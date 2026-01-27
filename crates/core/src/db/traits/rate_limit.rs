//! Rate limit tracking repository operations.

use crate::control::RateLimitSnapshot;
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};

/// Repository for rate limit tracking and history.
#[async_trait]
pub trait RateLimitRepository: Send + Sync {
    /// Save a rate limit snapshot for historical tracking.
    async fn save_rate_limit_snapshot(&self, endpoint: String, remaining: i64, reset: DateTime<Utc>) -> Result<()>;

    /// Get rate limit history within specified hours.
    async fn get_rate_limit_history(&self, hours: i64) -> Result<Vec<RateLimitSnapshot>>;
}
