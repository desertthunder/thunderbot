//! Database control operations (backup, restore, maintenance).

use anyhow::Result;
use async_trait::async_trait;

/// Repository for database maintenance and control operations.
#[async_trait]
pub trait ControlRepository: Send + Sync {
    /// Check database connectivity.
    async fn ping(&self) -> Result<()>;

    /// Create database backup to specified path. Returns backup size in bytes.
    async fn backup(&self, path: &str) -> Result<u64>;

    /// Restore database from backup file.
    async fn restore(&self, path: &str) -> Result<()>;

    /// Run VACUUM to reclaim space. Returns (before_size, after_size).
    async fn vacuum(&self) -> Result<(u64, u64)>;
}
