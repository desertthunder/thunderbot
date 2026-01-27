//! Activity logging repository operations.

use anyhow::Result;
use async_trait::async_trait;
use crate::db::types::ActivityLogRow;

/// Repository for activity logging.
#[async_trait]
pub trait ActivityRepository: Send + Sync {
    /// Log an activity event.
    async fn log_activity(&self, activity: ActivityLogRow) -> Result<()>;

    /// Get activity log, optionally filtered by action type.
    async fn get_activity_log(
        &self,
        action_type: Option<&str>,
        limit: usize,
    ) -> Result<Vec<ActivityLogRow>>;
}
