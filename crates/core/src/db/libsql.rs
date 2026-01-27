//! libSQL implementation of all database repository traits.

use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Utc};
use libsql::{Builder, Database, params};

// Import all the traits
use crate::db::traits::{
    ActivityRepository, BlocklistRepository, ControlRepository, DeadLetterRepository,
    FilterRepository, IdentityRepository, QuietHoursRepository, RateLimitRepository,
    ReplyLimitsRepository, ResponseQueueRepository, SearchRepository, SessionMetadataRepository,
    SessionRepository, ThreadRepository,
};
// Import the combined DatabaseRepository trait
use crate::db::DatabaseRepository;
// Import types
use crate::db::types::{
    ActivityLogRow, ConversationRow, FilterPresetRow, IdentityRow, MutedAuthorRow, SessionRow,
};
// Import control types
use crate::control::{
    BlocklistEntry, BlockType, DeadLetterItem, QuietHoursWindow, RateLimitSnapshot,
    ReplyLimitsConfig, ResponseQueueItem, ResponseStatus, SessionMetadata,
};

/// libSQL-based implementation of all database repository traits.
pub struct LibsqlRepository {
    db: Database,
}

impl LibsqlRepository {
    /// Create a new libSQL repository instance.
    pub async fn new(database_url: &str) -> Result<Self> {
        let db = Builder::new_local(database_url).build().await?;
        Ok(Self { db })
    }

    /// Execute a SQL statement with parameters.
    async fn execute(&self, sql: &str, params: impl libsql::params::IntoParams) -> Result<()> {
        let conn = self.db.connect()?;
        conn.execute(sql, params).await?;
        Ok(())
    }

    /// Execute a SQL query and map results.
    async fn query<T, F>(
        &self,
        sql: &str,
        params: impl libsql::params::IntoParams,
        mut f: F,
    ) -> Result<Vec<T>>
    where
        F: FnMut(&libsql::Row) -> Result<T> + Send + 'static,
        T: Send,
    {
        let conn = self.db.connect()?;
        let mut rows = conn.query(sql, params).await?;
        let mut results = Vec::new();

        while let Some(row) = rows.next().await? {
            results.push(f(&row)?);
        }

        Ok(results)
    }
}

// Implement the combined DatabaseRepository trait
#[async_trait::async_trait]
impl DatabaseRepository for LibsqlRepository {
    async fn run_migration(&self) -> Result<()> {
        let migration_001 = include_str!("../../migrations/001_init.sql");
        let migration_002 = include_str!("../../migrations/002_add_session.sql");
        let migration_003 = include_str!("../../migrations/003_add_search_indexes.sql");
        let migration_004 = include_str!("../../migrations/004_add_filters_and_activity.sql");
        let migration_005 = include_str!("../../migrations/005_operational_controls.sql");

        for statement in migration_001.split(';') {
            let statement = statement.trim();
            if !statement.is_empty() {
                self.execute(statement, ()).await?;
            }
        }

        for statement in migration_002.split(';') {
            let statement = statement.trim();
            if !statement.is_empty() {
                self.execute(statement, ()).await?;
            }
        }

        for statement in migration_003.split(';') {
            let statement = statement.trim();
            if !statement.is_empty() {
                self.execute(statement, ()).await?;
            }
        }

        for statement in migration_004.split(';') {
            let statement = statement.trim();
            if !statement.is_empty() {
                self.execute(statement, ()).await?;
            }
        }

        for statement in migration_005.split(';') {
            let statement = statement.trim();
            if !statement.is_empty() {
                self.execute(statement, ()).await?;
            }
        }

        Ok(())
    }
}

// Implement all individual traits - each gets its own impl block

#[async_trait::async_trait]
impl ThreadRepository for LibsqlRepository {
    async fn save_conversation(&self, row: ConversationRow) -> Result<()> {
        let sql = r#"
            INSERT OR IGNORE INTO conversations
                (id, thread_root_uri, post_uri, parent_uri, author_did, role, content, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        "#;

        self.execute(
            sql,
            params![
                row.id.as_str(),
                row.thread_root_uri.as_str(),
                row.post_uri.as_str(),
                row.parent_uri.as_deref(),
                row.author_did.as_str(),
                row.role.as_str(),
                row.content.as_str(),
                row.created_at.to_rfc3339().as_str()
            ],
        )
        .await
    }

    async fn get_thread_history(&self, thread_root_uri: &str) -> Result<Vec<ConversationRow>> {
        let sql = r#"
            SELECT id, thread_root_uri, post_uri, parent_uri,
                   author_did, role, content, created_at
            FROM conversations
            WHERE thread_root_uri = ?1
            ORDER BY created_at ASC
        "#;

        self.query(sql, [thread_root_uri], |row| {
            Ok(ConversationRow {
                id: row.get(0)?,
                thread_root_uri: row.get(1)?,
                post_uri: row.get(2)?,
                parent_uri: row.get(3)?,
                author_did: row.get(4)?,
                role: row.get(5)?,
                content: row.get(6)?,
                created_at: DateTime::parse_from_rfc3339(&row.get::<String>(7)?)
                    .unwrap()
                    .with_timezone(&Utc),
            })
        })
        .await
    }

    async fn get_all_threads(&self, limit: usize) -> Result<Vec<String>> {
        let sql = r#"
            SELECT thread_root_uri
            FROM conversations
            GROUP BY thread_root_uri
            ORDER BY MAX(created_at) DESC
            LIMIT ?
        "#;

        self.query(sql, [limit as i64], |row| Ok::<String, anyhow::Error>(row.get(0)?))
            .await
    }

    async fn get_user_threads(&self, author_did: &str, limit: usize) -> Result<Vec<String>> {
        let sql = r#"
            SELECT DISTINCT thread_root_uri
            FROM conversations
            WHERE author_did = ?
            ORDER BY created_at DESC
            LIMIT ?
        "#;

        let limit = format!("{}", limit);
        self.query(sql, [author_did, limit.as_str()], |row| {
            Ok::<String, anyhow::Error>(row.get(0)?)
        })
        .await
    }

    async fn get_stats(&self) -> Result<crate::db::types::DatabaseStats> {
        let conversation_sql = "SELECT COUNT(*) FROM conversations";
        let thread_sql = "SELECT COUNT(DISTINCT thread_root_uri) FROM conversations";
        let identity_sql = "SELECT COUNT(*) FROM identities";

        let conversation_results = self
            .query(conversation_sql, (), |row| Ok::<i64, anyhow::Error>(row.get(0)?))
            .await?;
        let conversation_count = conversation_results
            .first()
            .ok_or_else(|| anyhow::anyhow!("No conversation count"))?;

        let thread_results = self
            .query(thread_sql, (), |row| Ok::<i64, anyhow::Error>(row.get(0)?))
            .await?;
        let thread_count = thread_results
            .first()
            .ok_or_else(|| anyhow::anyhow!("No thread count"))?;

        let identity_results = self
            .query(identity_sql, (), |row| Ok::<i64, anyhow::Error>(row.get(0)?))
            .await?;
        let identity_count = identity_results
            .first()
            .ok_or_else(|| anyhow::anyhow!("No identity count"))?;

        Ok(crate::db::types::DatabaseStats {
            conversation_count: *conversation_count,
            thread_count: *thread_count,
            identity_count: *identity_count,
        })
    }

    async fn delete_conversations_by_uris(&self, thread_uris: &[String]) -> Result<usize> {
        let conn = self.db.connect()?;
        let mut total_deleted = 0;

        for thread_uri in thread_uris {
            let sql = "DELETE FROM conversations WHERE thread_root_uri = ?";
            match conn.execute(sql, [thread_uri.as_str()]).await {
                Ok(_) => total_deleted += 1,
                Err(e) => {
                    tracing::warn!("Failed to delete thread {}: {}", thread_uri, e);
                }
            }
        }

        Ok(total_deleted)
    }

    async fn delete_old_conversations(&self, days: i64) -> Result<usize> {
        let cutoff_date = Utc::now() - chrono::Duration::days(days);
        let sql = "DELETE FROM conversations WHERE created_at < ?";

        let conn = self.db.connect()?;
        let rows_affected = conn.execute(sql, [cutoff_date.to_rfc3339().as_str()]).await?;

        Ok(rows_affected as usize)
    }

    async fn get_muted_authors(&self) -> Result<Vec<MutedAuthorRow>> {
        let sql = r#"
            SELECT did, muted_at, muted_by
            FROM muted_authors
            ORDER BY muted_at DESC
        "#;

        self.query(sql, (), |row| {
            Ok(MutedAuthorRow {
                did: row.get(0)?,
                muted_at: DateTime::parse_from_rfc3339(&row.get::<String>(1)?)
                    .unwrap()
                    .with_timezone(&Utc),
                muted_by: row.get(2)?,
            })
        })
        .await
    }

    async fn mute_author(&self, did: &str, muted_by: &str) -> Result<()> {
        let sql = r#"
            INSERT OR REPLACE INTO muted_authors (did, muted_at, muted_by)
            VALUES (?, ?, ?)
        "#;

        self.execute(sql, params![did, Utc::now().to_rfc3339().as_str(), muted_by])
            .await
    }

    async fn unmute_author(&self, did: &str) -> Result<()> {
        let sql = "DELETE FROM muted_authors WHERE did = ?";
        self.execute(sql, [did]).await
    }
}

#[async_trait::async_trait]
impl IdentityRepository for LibsqlRepository {
    async fn save_identity(&self, row: IdentityRow) -> Result<()> {
        let sql = r#"
            INSERT OR REPLACE INTO identities (did, handle, last_updated)
            VALUES (?1, ?2, ?3)
        "#;

        self.execute(
            sql,
            params![
                row.did.as_str(),
                row.handle.as_str(),
                row.last_updated.to_rfc3339().as_str()
            ],
        )
        .await
    }

    async fn cache_identity(&self, did: &str, handle: &str) -> Result<()> {
        let sql = r#"
            INSERT OR REPLACE INTO identities (did, handle, last_updated)
            VALUES (?1, ?2, ?3)
        "#;

        self.execute(sql, params![did, handle, Utc::now().to_rfc3339().as_str()])
            .await
    }

    async fn get_identity(&self, did: &str) -> Result<Option<IdentityRow>> {
        let sql = r#"
            SELECT did, handle, last_updated
            FROM identities
            WHERE did = ?1
        "#;

        let rows = self
            .query(sql, [did], |row| {
                Ok(IdentityRow {
                    did: row.get(0)?,
                    handle: row.get(1)?,
                    last_updated: DateTime::parse_from_rfc3339(&row.get::<String>(2)?)
                        .unwrap()
                        .with_timezone(&Utc),
                })
            })
            .await?;

        Ok(rows.into_iter().next())
    }

    async fn get_all_identities(&self) -> Result<Vec<IdentityRow>> {
        let sql = r#"
            SELECT did, handle, last_updated
            FROM identities
            ORDER BY last_updated DESC
        "#;

        self.query(sql, (), |row| {
            Ok(IdentityRow {
                did: row.get(0)?,
                handle: row.get(1)?,
                last_updated: DateTime::parse_from_rfc3339(&row.get::<String>(2)?)
                    .unwrap()
                    .with_timezone(&Utc),
            })
        })
        .await
    }
}

#[async_trait::async_trait]
impl SessionRepository for LibsqlRepository {
    async fn save_session(&self, row: SessionRow) -> Result<()> {
        let sql = r#"
            INSERT OR REPLACE INTO sessions (did, handle, access_jwt, refresh_jwt, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
        "#;

        self.execute(
            sql,
            params![
                row.did.as_str(),
                row.handle.as_str(),
                row.access_jwt.as_str(),
                row.refresh_jwt.as_str(),
                row.updated_at.to_rfc3339().as_str()
            ],
        )
        .await
    }

    async fn get_session(&self, did: &str) -> Result<Option<SessionRow>> {
        let sql = r#"
            SELECT did, handle, access_jwt, refresh_jwt, updated_at
            FROM sessions
            WHERE did = ?1
        "#;

        let rows = self
            .query(sql, [did], |row| {
                Ok(SessionRow {
                    did: row.get(0)?,
                    handle: row.get(1)?,
                    access_jwt: row.get(2)?,
                    refresh_jwt: row.get(3)?,
                    updated_at: DateTime::parse_from_rfc3339(&row.get::<String>(4)?)
                        .unwrap()
                        .with_timezone(&Utc),
                })
            })
            .await?;

        Ok(rows.into_iter().next())
    }
}

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

#[async_trait::async_trait]
impl SearchRepository for LibsqlRepository {
    async fn search_conversations(
        &self,
        query: &str,
        author_filter: Option<&str>,
        role_filter: Option<&str>,
        date_from: Option<DateTime<Utc>>,
        date_to: Option<DateTime<Utc>>,
        limit: usize,
    ) -> Result<Vec<ConversationRow>> {
        let escaped_query = query.replace('\'', "''");
        let mut where_parts = vec![format!(
            "c.id IN (SELECT id FROM conversations_fts WHERE conversations_fts MATCH '{}')",
            escaped_query
        )];

        if let Some(author) = author_filter {
            let escaped_author = author.replace('\'', "''");
            where_parts.push(format!("c.author_did = '{}'", escaped_author));
        }

        if let Some(role) = role_filter {
            let escaped_role = role.replace('\'', "''");
            where_parts.push(format!("c.role = '{}'", escaped_role));
        }

        if let Some(from) = date_from {
            where_parts.push(format!("c.created_at >= '{}'", from.to_rfc3339()));
        }

        if let Some(to) = date_to {
            where_parts.push(format!("c.created_at <= '{}'", to.to_rfc3339()));
        }

        let sql = format!(
            r#"
                SELECT c.id, c.thread_root_uri, c.post_uri, c.parent_uri,
                       c.author_did, c.role, c.content, c.created_at
                FROM conversations c
                WHERE {}
                ORDER BY c.created_at DESC
                LIMIT {}
            "#,
            where_parts.join(" AND "),
            limit
        );

        self.query(&sql, (), |row| {
            Ok(ConversationRow {
                id: row.get(0)?,
                thread_root_uri: row.get(1)?,
                post_uri: row.get(2)?,
                parent_uri: row.get(3)?,
                author_did: row.get(4)?,
                role: row.get(5)?,
                content: row.get(6)?,
                created_at: DateTime::parse_from_rfc3339(&row.get::<String>(7)?)
                    .unwrap()
                    .with_timezone(&Utc),
            })
        })
        .await
    }

    async fn export_all_conversations(&self) -> Result<Vec<ConversationRow>> {
        let sql = r#"
            SELECT id, thread_root_uri, post_uri, parent_uri,
                   author_did, role, content, created_at
            FROM conversations
            ORDER BY created_at DESC
        "#;

        self.query(sql, (), |row| {
            Ok(ConversationRow {
                id: row.get(0)?,
                thread_root_uri: row.get(1)?,
                post_uri: row.get(2)?,
                parent_uri: row.get(3)?,
                author_did: row.get(4)?,
                role: row.get(5)?,
                content: row.get(6)?,
                created_at: DateTime::parse_from_rfc3339(&row.get::<String>(7)?)
                    .unwrap()
                    .with_timezone(&Utc),
            })
        })
        .await
    }

    async fn export_thread(&self, thread_root_uri: &str) -> Result<Vec<ConversationRow>> {
        let sql = r#"
            SELECT id, thread_root_uri, post_uri, parent_uri,
                   author_did, role, content, created_at
            FROM conversations
            WHERE thread_root_uri = ?
            ORDER BY created_at ASC
        "#;

        self.query(sql, [thread_root_uri], |row| {
            Ok(ConversationRow {
                id: row.get(0)?,
                thread_root_uri: row.get(1)?,
                post_uri: row.get(2)?,
                parent_uri: row.get(3)?,
                author_did: row.get(4)?,
                role: row.get(5)?,
                content: row.get(6)?,
                created_at: DateTime::parse_from_rfc3339(&row.get::<String>(7)?)
                    .unwrap()
                    .with_timezone(&Utc),
            })
        })
        .await
    }

    async fn get_conversations_with_length_filter(
        &self,
        min_messages: usize,
        limit: usize,
    ) -> Result<Vec<String>> {
        let sql = r#"
            SELECT thread_root_uri
            FROM conversations
            GROUP BY thread_root_uri
            HAVING COUNT(*) >= ?
            ORDER BY MAX(created_at) DESC
            LIMIT ?
        "#;

        self.query(sql, [min_messages as i64, limit as i64], |row| {
            Ok::<String, anyhow::Error>(row.get(0)?)
        })
        .await
    }

    async fn get_recent_threads(&self, hours: i64, limit: usize) -> Result<Vec<String>> {
        let cutoff = Utc::now() - chrono::Duration::hours(hours);
        let sql = format!(
            r#"
                SELECT DISTINCT thread_root_uri
                FROM conversations
                WHERE created_at >= '{}'
                ORDER BY MAX(created_at) OVER (PARTITION BY thread_root_uri) DESC
                LIMIT {}
            "#,
            cutoff.to_rfc3339(),
            limit
        );

        self.query(&sql, (), |row| Ok::<String, anyhow::Error>(row.get(0)?))
            .await
    }
}

#[async_trait::async_trait]
impl FilterRepository for LibsqlRepository {
    async fn save_filter_preset(&self, preset: FilterPresetRow) -> Result<()> {
        let sql = r#"
            INSERT OR REPLACE INTO filter_presets (id, name, filters_json, created_at, created_by)
            VALUES (?, ?, ?, ?, ?)
        "#;

        self.execute(
            sql,
            params![
                preset.id.as_str(),
                preset.name.as_str(),
                preset.filters_json.as_str(),
                preset.created_at.to_rfc3339().as_str(),
                preset.created_by.as_str()
            ],
        )
        .await
    }

    async fn get_filter_presets(&self, user_did: &str) -> Result<Vec<FilterPresetRow>> {
        let sql = r#"
            SELECT id, name, filters_json, created_at, created_by
            FROM filter_presets
            WHERE created_by = ?
            ORDER BY created_at DESC
        "#;

        self.query(sql, [user_did], |row| {
            Ok(FilterPresetRow {
                id: row.get(0)?,
                name: row.get(1)?,
                filters_json: row.get(2)?,
                created_at: DateTime::parse_from_rfc3339(&row.get::<String>(3)?)
                    .unwrap()
                    .with_timezone(&Utc),
                created_by: row.get(4)?,
            })
        })
        .await
    }
}

#[async_trait::async_trait]
impl ActivityRepository for LibsqlRepository {
    async fn log_activity(&self, activity: ActivityLogRow) -> Result<()> {
        let sql = r#"
            INSERT OR REPLACE INTO activity_log (id, action_type, description, thread_uri, metadata_json, created_at)
            VALUES (?, ?, ?, ?, ?, ?)
        "#;

        self.execute(
            sql,
            params![
                activity.id.as_str(),
                activity.action_type.as_str(),
                activity.description.as_str(),
                activity.thread_uri.as_deref(),
                activity.metadata_json.as_deref(),
                activity.created_at.to_rfc3339().as_str()
            ],
        )
        .await
    }

    async fn get_activity_log(
        &self,
        action_type: Option<&str>,
        limit: usize,
    ) -> Result<Vec<ActivityLogRow>> {
        let sql = if let Some(action) = action_type {
            format!(
                r#"
                    SELECT id, action_type, description, thread_uri, metadata_json, created_at
                    FROM activity_log
                    WHERE action_type = '{}'
                    ORDER BY created_at DESC
                    LIMIT {}
                "#,
                action.replace("'", "''"),
                limit
            )
        } else {
            format!(
                r#"
                    SELECT id, action_type, description, thread_uri, metadata_json, created_at
                    FROM activity_log
                    ORDER BY created_at DESC
                    LIMIT {}
                "#,
                limit
            )
        };

        let conn = self.db.connect()?;
        let mut rows = conn.query(&sql, ()).await?;
        let mut results = Vec::new();

        while let Some(row) = rows.next().await? {
            results.push(ActivityLogRow {
                id: row.get(0)?,
                action_type: row.get(1)?,
                description: row.get(2)?,
                thread_uri: row.get(3)?,
                metadata_json: row.get(4)?,
                created_at: DateTime::parse_from_rfc3339(&row.get::<String>(5)?)
                    .unwrap()
                    .with_timezone(&Utc),
            });
        }

        Ok(results)
    }
}

#[async_trait::async_trait]
impl ResponseQueueRepository for LibsqlRepository {
    async fn queue_response(&self, item: ResponseQueueItem) -> Result<()> {
        let sql = r#"
            INSERT INTO response_queue
                (id, thread_uri, parent_uri, parent_cid, root_uri, root_cid, content, status, created_at, expires_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        "#;

        self.execute(
            sql,
            params![
                item.id.as_str(),
                item.thread_uri.as_str(),
                item.parent_uri.as_str(),
                item.parent_cid.as_str(),
                item.root_uri.as_str(),
                item.root_cid.as_str(),
                item.content.as_str(),
                format!("{:?}", item.status).as_str(),
                item.created_at.to_rfc3339().as_str(),
                item.expires_at.map(|d| d.to_rfc3339()),
            ],
        )
        .await
    }

    async fn get_pending_responses(&self) -> Result<Vec<ResponseQueueItem>> {
        let sql = r#"
            SELECT id, thread_uri, parent_uri, parent_cid, root_uri, root_cid, content, status, created_at, expires_at
            FROM response_queue
            WHERE status = 'Pending'
            ORDER BY created_at ASC
        "#;

        let conn = self.db.connect()?;
        let mut rows = conn.query(sql, ()).await?;
        let mut results = Vec::new();

        while let Some(row) = rows.next().await? {
            let status_str: String = row.get(7)?;
            let status = match status_str.as_str() {
                "Pending" => ResponseStatus::Pending,
                "Approved" => ResponseStatus::Approved,
                "Edited" => ResponseStatus::Edited,
                "Discarded" => ResponseStatus::Discarded,
                _ => anyhow::bail!("Unknown response status: {}", status_str),
            };

            results.push(ResponseQueueItem {
                id: row.get(0)?,
                thread_uri: row.get(1)?,
                parent_uri: row.get(2)?,
                parent_cid: row.get(3)?,
                root_uri: row.get(4)?,
                root_cid: row.get(5)?,
                content: row.get(6)?,
                status,
                created_at: DateTime::parse_from_rfc3339(&row.get::<String>(8)?)
                    .unwrap()
                    .with_timezone(&Utc),
                expires_at: row
                    .get::<Option<String>>(9)?
                    .map(|s| DateTime::parse_from_rfc3339(&s).unwrap().with_timezone(&Utc)),
            });
        }

        Ok(results)
    }

    async fn get_response_item(&self, id: &str) -> Result<ResponseQueueItem> {
        let sql = r#"
            SELECT id, thread_uri, parent_uri, parent_cid, root_uri, root_cid, content, status, created_at, expires_at
            FROM response_queue
            WHERE id = ?1
        "#;

        let conn = self.db.connect()?;
        let mut rows = conn.query(sql, [id]).await?;
        let row = rows
            .next()
            .await?
            .ok_or_else(|| anyhow::anyhow!("Response not found: {}", id))?;

        let status_str: String = row.get(7)?;
        let status = match status_str.as_str() {
            "Pending" => ResponseStatus::Pending,
            "Approved" => ResponseStatus::Approved,
            "Edited" => ResponseStatus::Edited,
            "Discarded" => ResponseStatus::Discarded,
            _ => anyhow::bail!("Unknown response status: {}", status_str),
        };

        Ok(ResponseQueueItem {
            id: row.get(0)?,
            thread_uri: row.get(1)?,
            parent_uri: row.get(2)?,
            parent_cid: row.get(3)?,
            root_uri: row.get(4)?,
            root_cid: row.get(5)?,
            content: row.get(6)?,
            status,
            created_at: DateTime::parse_from_rfc3339(&row.get::<String>(8)?)
                .unwrap()
                .with_timezone(&Utc),
            expires_at: row
                .get::<Option<String>>(9)?
                .map(|s| DateTime::parse_from_rfc3339(&s).unwrap().with_timezone(&Utc)),
        })
    }

    async fn update_response_status(&self, id: &str, status: ResponseStatus) -> Result<()> {
        let sql = "UPDATE response_queue SET status = ?1 WHERE id = ?2";
        self.execute(sql, params![format!("{:?}", status).as_str(), id])
            .await
    }

    async fn update_response_content(&self, id: &str, content: &str) -> Result<()> {
        let sql = "UPDATE response_queue SET content = ?1, status = ?2 WHERE id = ?3";
        self.execute(
            sql,
            params![
                content,
                format!("{:?}", ResponseStatus::Edited).as_str(),
                id
            ],
        )
        .await
    }
}

#[async_trait::async_trait]
impl QuietHoursRepository for LibsqlRepository {
    async fn get_quiet_hours(&self) -> Result<Vec<QuietHoursWindow>> {
        let sql = r#"
            SELECT id, day_of_week, start_time, end_time, timezone, enabled
            FROM quiet_hours
            ORDER BY day_of_week, start_time
        "#;

        let conn = self.db.connect()?;
        let mut rows = conn.query(sql, ()).await?;
        let mut results = Vec::new();

        while let Some(row) = rows.next().await? {
            results.push(QuietHoursWindow {
                id: row.get(0)?,
                day_of_week: row.get::<i32>(1)? as u8,
                start_time: row.get(2)?,
                end_time: row.get(3)?,
                timezone: row.get(4)?,
                enabled: row.get::<i32>(5)? == 1,
            });
        }

        Ok(results)
    }

    async fn save_quiet_hours(&self, window: QuietHoursWindow) -> Result<()> {
        let sql = r#"
            INSERT OR REPLACE INTO quiet_hours (id, day_of_week, start_time, end_time, timezone, enabled)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#;

        self.execute(
            sql,
            params![
                window.id.as_str(),
                window.day_of_week,
                window.start_time.as_str(),
                window.end_time.as_str(),
                window.timezone.as_str(),
                if window.enabled { 1i32 } else { 0 },
            ],
        )
        .await
    }

    async fn delete_quiet_hours(&self, id: &str) -> Result<()> {
        let sql = "DELETE FROM quiet_hours WHERE id = ?1";
        self.execute(sql, [id]).await
    }

    async fn is_quiet_hours_active(&self) -> Result<bool> {
        use chrono_tz::Tz;

        let windows = self.get_quiet_hours().await?;
        let now = Utc::now();

        for window in windows {
            if !window.enabled {
                continue;
            }

            let tz: Tz = window
                .timezone
                .parse()
                .with_context(|| format!("Invalid timezone: {}", window.timezone))?;
            let local_time = now.with_timezone(&tz);

            let day_match = local_time.weekday().num_days_from_sunday() as u8 == window.day_of_week;

            if day_match {
                let current_time = local_time.format("%H:%M").to_string();
                if current_time >= window.start_time && current_time <= window.end_time {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }
}

#[async_trait::async_trait]
impl ReplyLimitsRepository for LibsqlRepository {
    async fn get_reply_limits_config(&self) -> Result<ReplyLimitsConfig> {
        let sql = r#"
            SELECT id, max_replies_per_thread, cooldown_seconds, max_replies_per_author_hour, updated_at
            FROM reply_limits_config
            LIMIT 1
        "#;

        let conn = self.db.connect()?;
        let mut rows = conn.query(sql, ()).await?;

        match rows.next().await? {
            Some(row) => Ok(ReplyLimitsConfig {
                id: row.get(0)?,
                max_replies_per_thread: row.get(1)?,
                cooldown_seconds: row.get(2)?,
                max_replies_per_author_hour: row.get(3)?,
                updated_at: DateTime::parse_from_rfc3339(&row.get::<String>(4)?)
                    .unwrap()
                    .with_timezone(&Utc),
            }),
            None => {
                // Return default config
                Ok(ReplyLimitsConfig {
                    id: uuid::Uuid::new_v4().to_string(),
                    max_replies_per_thread: 10,
                    cooldown_seconds: 60,
                    max_replies_per_author_hour: 5,
                    updated_at: Utc::now(),
                })
            }
        }
    }

    async fn update_reply_limits_config(&self, config: ReplyLimitsConfig) -> Result<()> {
        let sql = r#"
            INSERT OR REPLACE INTO reply_limits_config
                (id, max_replies_per_thread, cooldown_seconds, max_replies_per_author_hour, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
        "#;

        self.execute(
            sql,
            params![
                config.id.as_str(),
                config.max_replies_per_thread,
                config.cooldown_seconds,
                config.max_replies_per_author_hour,
                config.updated_at.to_rfc3339().as_str(),
            ],
        )
        .await
    }

    async fn count_replies_in_thread(&self, thread_uri: &str) -> Result<i64> {
        let sql = r#"
            SELECT COUNT(*)
            FROM conversations
            WHERE thread_root_uri = ?1 AND role = 'assistant'
        "#;

        let conn = self.db.connect()?;
        let mut rows = conn.query(sql, [thread_uri]).await?;
        let row = rows
            .next()
            .await?
            .ok_or_else(|| anyhow::anyhow!("Count failed"))?;

        Ok(row.get(0)?)
    }

    async fn count_replies_by_author_last_hour(&self, author_did: &str) -> Result<i64> {
        let cutoff = Utc::now() - chrono::Duration::hours(1);
        let sql = format!(
            r#"
            SELECT COUNT(*)
            FROM conversations
            WHERE author_did = '{}' AND role = 'assistant' AND created_at >= '{}'
        "#,
            author_did.replace('\'', "''"),
            cutoff.to_rfc3339()
        );

        let conn = self.db.connect()?;
        let mut rows = conn.query(&sql, ()).await?;
        let row = rows
            .next()
            .await?
            .ok_or_else(|| anyhow::anyhow!("Count failed"))?;

        Ok(row.get(0)?)
    }

    async fn get_last_reply_time(&self, author_did: &str) -> Result<Option<DateTime<Utc>>> {
        let sql = r#"
            SELECT created_at
            FROM conversations
            WHERE author_did = ?1 AND role = 'assistant'
            ORDER BY created_at DESC
            LIMIT 1
        "#;

        let conn = self.db.connect()?;
        let mut rows = conn.query(sql, [author_did]).await?;

        match rows.next().await? {
            Some(row) => Ok(Some(
                DateTime::parse_from_rfc3339(&row.get::<String>(0)?)
                    .unwrap()
                    .with_timezone(&Utc),
            )),
            None => Ok(None),
        }
    }
}

#[async_trait::async_trait]
impl BlocklistRepository for LibsqlRepository {
    async fn get_blocklist(&self) -> Result<Vec<BlocklistEntry>> {
        let sql = r#"
            SELECT did, blocked_at, blocked_by, reason, expires_at, block_type
            FROM blocklist
            ORDER BY blocked_at DESC
        "#;

        let conn = self.db.connect()?;
        let mut rows = conn.query(sql, ()).await?;
        let mut results = Vec::new();

        while let Some(row) = rows.next().await? {
            let block_type_str: String = row.get(5)?;
            let block_type = match block_type_str.as_str() {
                "Author" => BlockType::Author,
                "Domain" => BlockType::Domain,
                _ => anyhow::bail!("Unknown block type: {}", block_type_str),
            };

            results.push(BlocklistEntry {
                did: row.get(0)?,
                blocked_at: DateTime::parse_from_rfc3339(&row.get::<String>(1)?)
                    .unwrap()
                    .with_timezone(&Utc),
                blocked_by: row.get(2)?,
                reason: row.get(3)?,
                expires_at: row
                    .get::<Option<String>>(4)?
                    .map(|s| DateTime::parse_from_rfc3339(&s).unwrap().with_timezone(&Utc)),
                block_type,
            });
        }

        Ok(results)
    }

    async fn add_to_blocklist(&self, entry: BlocklistEntry) -> Result<()> {
        let sql = r#"
            INSERT OR REPLACE INTO blocklist (did, blocked_at, blocked_by, reason, expires_at, block_type)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#;

        self.execute(
            sql,
            params![
                entry.did.as_str(),
                entry.blocked_at.to_rfc3339().as_str(),
                entry.blocked_by.as_str(),
                entry.reason.as_deref(),
                entry.expires_at.map(|d| d.to_rfc3339()),
                format!("{:?}", entry.block_type).as_str(),
            ],
        )
        .await
    }

    async fn remove_from_blocklist(&self, did: &str) -> Result<()> {
        let sql = "DELETE FROM blocklist WHERE did = ?1";
        self.execute(sql, [did]).await
    }

    async fn is_blocked(&self, did: &str) -> Result<bool> {
        let sql = r#"
            SELECT did, expires_at
            FROM blocklist
            WHERE did = ?1
        "#;

        let conn = self.db.connect()?;
        let mut rows = conn.query(sql, [did]).await?;

        if let Some(row) = rows.next().await? {
            // Check if expired
            if let Some(expiration_str) = row.get::<Option<String>>(1)? {
                let expiration =
                    DateTime::parse_from_rfc3339(&expiration_str)?.with_timezone(&Utc);
                if expiration < Utc::now() {
                    // Expired, not blocked
                    return Ok(false);
                }
            }
            return Ok(true);
        }

        Ok(false)
    }
}

#[async_trait::async_trait]
impl DeadLetterRepository for LibsqlRepository {
    async fn add_to_dlq(&self, item: DeadLetterItem) -> Result<()> {
        let sql = r#"
            INSERT INTO dead_letter_queue (id, event_json, error_message, retry_count, last_retry_at, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#;

        self.execute(
            sql,
            params![
                item.id.as_str(),
                item.event_json.as_str(),
                item.error_message.as_str(),
                item.retry_count,
                item.last_retry_at.map(|d| d.to_rfc3339()),
                item.created_at.to_rfc3339().as_str(),
            ],
        )
        .await
    }

    async fn get_dlq_items(&self, limit: usize) -> Result<Vec<DeadLetterItem>> {
        let sql = r#"
            SELECT id, event_json, error_message, retry_count, last_retry_at, created_at
            FROM dead_letter_queue
            ORDER BY created_at DESC
            LIMIT ?
        "#;

        let conn = self.db.connect()?;
        let mut rows = conn.query(sql, [limit as i64]).await?;
        let mut results = Vec::new();

        while let Some(row) = rows.next().await? {
            results.push(DeadLetterItem {
                id: row.get(0)?,
                event_json: row.get(1)?,
                error_message: row.get(2)?,
                retry_count: row.get(3)?,
                last_retry_at: row
                    .get::<Option<String>>(4)?
                    .map(|s| DateTime::parse_from_rfc3339(&s).unwrap().with_timezone(&Utc)),
                created_at: DateTime::parse_from_rfc3339(&row.get::<String>(5)?)
                    .unwrap()
                    .with_timezone(&Utc),
            });
        }

        Ok(results)
    }

    async fn get_dlq_item(&self, id: &str) -> Result<DeadLetterItem> {
        let sql = r#"
            SELECT id, event_json, error_message, retry_count, last_retry_at, created_at
            FROM dead_letter_queue
            WHERE id = ?1
        "#;

        let conn = self.db.connect()?;
        let mut rows = conn.query(sql, [id]).await?;
        let row = rows
            .next()
            .await?
            .ok_or_else(|| anyhow::anyhow!("DLQ item not found: {}", id))?;

        Ok(DeadLetterItem {
            id: row.get(0)?,
            event_json: row.get(1)?,
            error_message: row.get(2)?,
            retry_count: row.get(3)?,
            last_retry_at: row
                .get::<Option<String>>(4)?
                .map(|s| DateTime::parse_from_rfc3339(&s).unwrap().with_timezone(&Utc)),
            created_at: DateTime::parse_from_rfc3339(&row.get::<String>(5)?)
                .unwrap()
                .with_timezone(&Utc),
        })
    }

    async fn remove_from_dlq(&self, id: &str) -> Result<()> {
        let sql = "DELETE FROM dead_letter_queue WHERE id = ?1";
        self.execute(sql, [id]).await
    }

    async fn purge_dlq(&self) -> Result<()> {
        let sql = "DELETE FROM dead_letter_queue";
        self.execute(sql, ()).await
    }

    async fn purge_old_dlq_items(&self, days: i64) -> Result<u64> {
        let cutoff = Utc::now() - chrono::Duration::days(days);
        let sql = "DELETE FROM dead_letter_queue WHERE created_at < ?";

        let conn = self.db.connect()?;
        let rows_affected = conn.execute(sql, [cutoff.to_rfc3339().as_str()]).await?;

        Ok(rows_affected as u64)
    }
}

#[async_trait::async_trait]
impl RateLimitRepository for LibsqlRepository {
    async fn save_rate_limit_snapshot(
        &self,
        endpoint: String,
        remaining: i64,
        reset: DateTime<Utc>,
    ) -> Result<()> {
        let sql = r#"
            INSERT INTO rate_limit_history (id, endpoint, limit_remaining, limit_reset, recorded_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
        "#;

        self.execute(
            sql,
            params![
                uuid::Uuid::new_v4().to_string().as_str(),
                endpoint.as_str(),
                remaining,
                reset.to_rfc3339().as_str(),
                Utc::now().to_rfc3339().as_str(),
            ],
        )
        .await
    }

    async fn get_rate_limit_history(&self, hours: i64) -> Result<Vec<RateLimitSnapshot>> {
        let cutoff = Utc::now() - chrono::Duration::hours(hours);
        let sql = format!(
            r#"
            SELECT id, endpoint, limit_remaining, limit_reset, recorded_at
            FROM rate_limit_history
            WHERE recorded_at >= '{}'
            ORDER BY recorded_at DESC
        "#,
            cutoff.to_rfc3339()
        );

        let conn = self.db.connect()?;
        let mut rows = conn.query(&sql, ()).await?;
        let mut results = Vec::new();

        while let Some(row) = rows.next().await? {
            results.push(RateLimitSnapshot {
                id: row.get(0)?,
                endpoint: row.get(1)?,
                limit_remaining: row.get(2)?,
                limit_reset: DateTime::parse_from_rfc3339(&row.get::<String>(3)?)
                    .unwrap()
                    .with_timezone(&Utc),
                recorded_at: DateTime::parse_from_rfc3339(&row.get::<String>(4)?)
                    .unwrap()
                    .with_timezone(&Utc),
            });
        }

        Ok(results)
    }
}

#[async_trait::async_trait]
impl SessionMetadataRepository for LibsqlRepository {
    async fn save_session_metadata(&self, metadata: SessionMetadata) -> Result<()> {
        let sql = r#"
            INSERT OR REPLACE INTO session_metadata
                (did, access_jwt_expires_at, refresh_jwt_expires_at, last_refresh_at, force_refresh_before)
            VALUES (?1, ?2, ?3, ?4, ?5)
        "#;

        self.execute(
            sql,
            params![
                metadata.did.as_str(),
                metadata.access_jwt_expires_at.to_rfc3339().as_str(),
                metadata.refresh_jwt_expires_at.to_rfc3339().as_str(),
                metadata.last_refresh_at.map(|d| d.to_rfc3339()),
                metadata.force_refresh_before.map(|d| d.to_rfc3339()),
            ],
        )
        .await
    }

    async fn get_session_metadata(&self, did: &str) -> Result<Option<SessionMetadata>> {
        let sql = r#"
            SELECT did, access_jwt_expires_at, refresh_jwt_expires_at, last_refresh_at, force_refresh_before
            FROM session_metadata
            WHERE did = ?1
        "#;

        let conn = self.db.connect()?;
        let mut rows = conn.query(sql, [did]).await?;

        match rows.next().await? {
            Some(row) => Ok(Some(SessionMetadata {
                did: row.get(0)?,
                access_jwt_expires_at: DateTime::parse_from_rfc3339(&row.get::<String>(1)?)
                    .unwrap()
                    .with_timezone(&Utc),
                refresh_jwt_expires_at: DateTime::parse_from_rfc3339(&row.get::<String>(2)?)
                    .unwrap()
                    .with_timezone(&Utc),
                last_refresh_at: row
                    .get::<Option<String>>(3)?
                    .map(|s| DateTime::parse_from_rfc3339(&s).unwrap().with_timezone(&Utc)),
                force_refresh_before: row
                    .get::<Option<String>>(4)?
                    .map(|s| DateTime::parse_from_rfc3339(&s).unwrap().with_timezone(&Utc)),
            })),
            None => Ok(None),
        }
    }
}
