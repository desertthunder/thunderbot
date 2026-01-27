//! Control repository implementation for libSQL.

use anyhow::{Context, Result};

use crate::db::libsql::LibsqlRepository;
use crate::db::traits::ControlRepository;

#[async_trait::async_trait]
impl ControlRepository for LibsqlRepository {
    async fn ping(&self) -> Result<()> {
        let sql = "SELECT 1";
        let conn = self.db.connect()?;
        let mut rows = conn.query(sql, ()).await?;
        let first = rows
            .next()
            .await?
            .ok_or_else(|| anyhow::anyhow!("Database ping failed"))?;
        first.get::<i32>(0)?;
        Ok(())
    }

    async fn backup(&self, path: &str) -> Result<u64> {
        tracing::info!("Creating database backup to: {}", path);

        let conn = self.db.connect()?;
        conn.execute("PRAGMA wal_checkpoint(TRUNCATE)", ()).await?;

        let sql = format!("VACUUM INTO '{}'", path.replace("'", "''"));
        conn.execute(&sql, ()).await?;

        let metadata = std::fs::metadata(path)?;
        let size_bytes = metadata.len();

        tracing::info!("Backup created: {} ({} bytes)", path, size_bytes);
        Ok(size_bytes)
    }

    async fn restore(&self, path: &str) -> Result<()> {
        tracing::info!("Restoring database from: {}", path);

        if !std::path::Path::new(path).exists() {
            anyhow::bail!("Backup file does not exist: {}", path);
        }

        let conn = self.db.connect()?;
        conn.execute("DETACH DATABASE main", ()).await.ok();

        let temp_path = format!("{}.temp", path);
        std::fs::copy(path, &temp_path).context("Failed to copy backup file")?;

        conn.execute(&format!("ATTACH DATABASE '{}' AS backup_db", temp_path), ())
            .await?;

        conn.execute("DROP TABLE IF EXISTS main.conversations", ()).await?;
        conn.execute("DROP TABLE IF EXISTS main.identities", ()).await?;
        conn.execute("DROP TABLE IF EXISTS main.sessions", ()).await?;

        conn.execute(
            "CREATE TABLE main.conversations AS SELECT * FROM backup_db.conversations",
            (),
        )
        .await?;
        conn.execute("CREATE TABLE main.identities AS SELECT * FROM backup_db.identities", ())
            .await?;
        conn.execute("CREATE TABLE main.sessions AS SELECT * FROM backup_db.sessions", ())
            .await?;

        conn.execute("DETACH DATABASE backup_db", ()).await?;

        std::fs::remove_file(&temp_path).ok();

        tracing::info!("Database restored from: {}", path);
        Ok(())
    }

    async fn vacuum(&self) -> Result<(u64, u64)> {
        tracing::info!("Running VACUUM on database");

        let conn = self.db.connect()?;

        let before_size = {
            let sql = "SELECT page_count * page_size as size FROM pragma_page_count(), pragma_page_size()";
            let mut rows = conn.query(sql, ()).await?;
            rows.next().await?.and_then(|r| r.get::<i64>(0).ok()).unwrap_or(0) as u64
        };

        conn.execute("VACUUM", ()).await?;

        let after_size = {
            let sql = "SELECT page_count * page_size as size FROM pragma_page_count(), pragma_page_size()";
            let mut rows = conn.query(sql, ()).await?;
            rows.next().await?.and_then(|r| r.get::<i64>(0).ok()).unwrap_or(0) as u64
        };

        let saved = before_size.saturating_sub(after_size);
        tracing::info!(
            "VACUUM completed: before={} bytes, after={} bytes, saved={} bytes",
            before_size,
            after_size,
            saved
        );

        Ok((before_size, after_size))
    }
}
