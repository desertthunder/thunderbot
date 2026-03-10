use crate::error::BotError;
use libsql::Database;
use std::path::Path;

/// Manages database connections and provides access to the libSQL database
#[derive(Debug)]
pub struct DatabaseManager {
    db: Database,
    path: String,
}

impl DatabaseManager {
    /// Open a database at the given path, creating it if it doesn't exist
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, BotError> {
        let path = path.as_ref();
        let path_str = path.to_string_lossy().to_string();

        if let Some(parent) = path.parent()
            && !parent.exists()
        {
            tracing::info!("Creating database directory: {:?}", parent);
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| BotError::Database(format!("Failed to create database directory: {}", e)))?;
        }

        tracing::info!("Opening database at: {}", path_str);

        let db = libsql::Builder::new_local(path_str.clone())
            .build()
            .await
            .map_err(|e| BotError::Database(format!("Failed to open database: {}", e)))?;

        tracing::info!("Database opened successfully");

        Ok(Self { db, path: path_str })
    }

    /// Get a reference to the underlying database
    pub fn db(&self) -> &Database {
        &self.db
    }

    /// Get the database path
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Close the database connection
    pub async fn close(self) {
        tracing::info!("Closing database connection");
        drop(self.db);
    }

    /// Get database statistics
    pub async fn stats(&self) -> Result<DatabaseStats, BotError> {
        let conn = self
            .db
            .connect()
            .map_err(|e| BotError::Database(format!("Failed to connect to database: {}", e)))?;

        let mut stats = DatabaseStats {
            path: self.path.clone(),
            conversations_count: 0,
            identities_count: 0,
            failed_events_count: 0,
            ..Default::default()
        };

        let mut rows = conn
            .query("SELECT COUNT(*) FROM conversations", ())
            .await
            .map_err(|e| BotError::Database(format!("Failed to count conversations: {}", e)))?;
        if let Ok(Some(row)) = rows.next().await {
            stats.conversations_count = row.get::<i64>(0).unwrap_or(0);
        }

        let mut rows = conn
            .query("SELECT COUNT(*) FROM identities", ())
            .await
            .map_err(|e| BotError::Database(format!("Failed to count identities: {}", e)))?;
        if let Ok(Some(row)) = rows.next().await {
            stats.identities_count = row.get::<i64>(0).unwrap_or(0);
        }

        let mut rows = conn
            .query("SELECT COUNT(*) FROM failed_events", ())
            .await
            .map_err(|e| BotError::Database(format!("Failed to count failed events: {}", e)))?;
        if let Ok(Some(row)) = rows.next().await {
            stats.failed_events_count = row.get::<i64>(0).unwrap_or(0);
        }

        if let Ok(metadata) = tokio::fs::metadata(&self.path).await {
            stats.file_size_bytes = metadata.len();
        } else {
            tracing::warn!("Could not get database file size");
        }

        let mut rows = conn
            .query("SELECT time_us FROM cursor_state WHERE id = 1", ())
            .await
            .map_err(|e| BotError::Database(format!("Failed to get cursor state: {}", e)))?;

        if let Ok(Some(row)) = rows.next().await {
            stats.last_cursor_time_us = row.get::<i64>(0).ok();
        }

        Ok(stats)
    }
}

/// Database statistics
#[derive(Debug, Clone, Default)]
pub struct DatabaseStats {
    pub path: String,
    pub conversations_count: i64,
    pub identities_count: i64,
    pub failed_events_count: i64,
    pub file_size_bytes: u64,
    pub last_cursor_time_us: Option<i64>,
}

impl std::fmt::Display for DatabaseStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Database: {}", self.path)?;
        writeln!(f, "  Conversations: {}", self.conversations_count)?;
        writeln!(f, "  Identities: {}", self.identities_count)?;
        writeln!(f, "  Failed Events: {}", self.failed_events_count)?;
        writeln!(f, "  File Size: {} bytes", self.file_size_bytes)?;
        if let Some(cursor) = self.last_cursor_time_us {
            writeln!(f, "  Last Cursor: {}", cursor)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_open_database() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let manager = DatabaseManager::open(&db_path).await.expect("Failed to open database");

        assert_eq!(manager.path(), db_path.to_str().unwrap());
        assert!(db_path.exists() || db_path.parent().unwrap().exists());
    }

    #[tokio::test]
    async fn test_open_creates_directories() {
        let temp_dir = TempDir::new().unwrap();
        let nested_path = temp_dir.path().join("nested").join("deep").join("test.db");

        let manager = DatabaseManager::open(&nested_path)
            .await
            .expect("Failed to open database");

        assert!(nested_path.parent().unwrap().exists());
        assert_eq!(manager.path(), nested_path.to_str().unwrap());
    }
}
