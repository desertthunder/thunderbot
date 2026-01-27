//! Filter preset repository operations.

use crate::db::types::FilterPresetRow;
use anyhow::Result;
use async_trait::async_trait;

/// Repository for filter preset management.
#[async_trait]
pub trait FilterRepository: Send + Sync {
    /// Save a filter preset for a user.
    async fn save_filter_preset(&self, preset: FilterPresetRow) -> Result<()>;

    /// Get all filter presets for a user.
    async fn get_filter_presets(&self, user_did: &str) -> Result<Vec<FilterPresetRow>>;
}
