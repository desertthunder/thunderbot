//! Quiet hours repository operations.

use anyhow::Result;
use async_trait::async_trait;
use crate::control::QuietHoursWindow;

/// Repository for quiet hours configuration.
#[async_trait]
pub trait QuietHoursRepository: Send + Sync {
    /// Get all configured quiet hours windows.
    async fn get_quiet_hours(&self) -> Result<Vec<QuietHoursWindow>>;

    /// Save or update a quiet hours window.
    async fn save_quiet_hours(&self, window: QuietHoursWindow) -> Result<()>;

    /// Delete a quiet hours window by ID.
    async fn delete_quiet_hours(&self, id: &str) -> Result<()>;

    /// Check if quiet hours are currently active.
    async fn is_quiet_hours_active(&self) -> Result<bool>;
}
