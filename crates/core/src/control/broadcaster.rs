//! Status broadcasting (bio updates, status posts).

use crate::bsky::BskyClient;
use anyhow::Result;
use std::sync::Arc;

/// Broadcasts status updates and bio changes.
pub struct StatusBroadcaster {
    bsky_client: Arc<BskyClient>,
}

impl StatusBroadcaster {
    /// Create a new status broadcaster.
    pub fn new(bsky_client: Arc<BskyClient>) -> Self {
        Self { bsky_client }
    }

    /// Update bot bio with status message.
    pub async fn update_bio(&self, status: &str) -> Result<()> {
        let new_description = format!("🤖 {}", status);
        self.bsky_client.update_profile(Some(&new_description)).await
    }

    /// Post a status update as a regular post.
    pub async fn post_status_update(&self, message: &str) -> Result<()> {
        tracing::info!("Posting status update: {}", message);
        self.bsky_client.create_post(message).await?;
        Ok(())
    }

    /// Post a maintenance announcement.
    pub async fn post_maintenance_announcement(&self, duration_minutes: u64) -> Result<()> {
        let msg = format!(
            "⚠️ Scheduled maintenance in progress. Expect limited functionality for the next {} minutes.",
            duration_minutes
        );
        self.post_status_update(&msg).await
    }
}
