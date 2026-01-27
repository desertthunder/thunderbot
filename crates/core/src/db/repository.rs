use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use libsql::{Builder, Database, params};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationRow {
    pub id: String,
    pub thread_root_uri: String,
    pub post_uri: String,
    pub parent_uri: Option<String>,
    pub author_did: String,
    pub role: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityRow {
    pub did: String,
    pub handle: String,
    pub last_updated: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRow {
    pub did: String,
    pub handle: String,
    pub access_jwt: String,
    pub refresh_jwt: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutedAuthorRow {
    pub did: String,
    pub muted_at: DateTime<Utc>,
    pub muted_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterPresetRow {
    pub id: String,
    pub name: String,
    pub filters_json: String,
    pub created_at: DateTime<Utc>,
    pub created_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityLogRow {
    pub id: String,
    pub action_type: String,
    pub description: String,
    pub thread_uri: Option<String>,
    pub metadata_json: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[async_trait::async_trait]
pub trait DatabaseRepository: Send + Sync {
    async fn run_migration(&self) -> Result<()>;
    async fn save_conversation(&self, row: ConversationRow) -> Result<()>;
    async fn get_thread_history(&self, thread_root_uri: &str) -> Result<Vec<ConversationRow>>;
    async fn get_all_threads(&self, limit: usize) -> Result<Vec<String>>;
    async fn get_user_threads(&self, author_did: &str, limit: usize) -> Result<Vec<String>>;
    async fn save_identity(&self, row: IdentityRow) -> Result<()>;
    async fn cache_identity(&self, did: &str, handle: &str) -> Result<()>;
    async fn get_identity(&self, did: &str) -> Result<Option<IdentityRow>>;
    async fn get_all_identities(&self) -> Result<Vec<IdentityRow>>;
    async fn save_session(&self, row: SessionRow) -> Result<()>;
    async fn get_session(&self, did: &str) -> Result<Option<SessionRow>>;
    async fn get_stats(&self) -> Result<DatabaseStats>;
    async fn ping(&self) -> Result<()>;
    async fn backup(&self, path: &str) -> Result<u64>;
    async fn restore(&self, path: &str) -> Result<()>;
    async fn vacuum(&self) -> Result<(u64, u64)>;

    async fn search_conversations(
        &self, query: &str, author_filter: Option<&str>, role_filter: Option<&str>, date_from: Option<DateTime<Utc>>,
        date_to: Option<DateTime<Utc>>, limit: usize,
    ) -> Result<Vec<ConversationRow>>;
    async fn export_all_conversations(&self) -> Result<Vec<ConversationRow>>;
    async fn export_thread(&self, thread_root_uri: &str) -> Result<Vec<ConversationRow>>;
    async fn delete_conversations_by_uris(&self, thread_uris: &[String]) -> Result<usize>;
    async fn delete_old_conversations(&self, days: i64) -> Result<usize>;
    async fn get_muted_authors(&self) -> Result<Vec<MutedAuthorRow>>;
    async fn mute_author(&self, did: &str, muted_by: &str) -> Result<()>;
    async fn unmute_author(&self, did: &str) -> Result<()>;
    async fn save_filter_preset(&self, preset: FilterPresetRow) -> Result<()>;
    async fn get_filter_presets(&self, user_did: &str) -> Result<Vec<FilterPresetRow>>;
    async fn get_conversations_with_length_filter(&self, min_messages: usize, limit: usize) -> Result<Vec<String>>;
    async fn get_recent_threads(&self, hours: i64, limit: usize) -> Result<Vec<String>>;
    async fn log_activity(&self, activity: ActivityLogRow) -> Result<()>;
    async fn get_activity_log(&self, action_type: Option<&str>, limit: usize) -> Result<Vec<ActivityLogRow>>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseStats {
    pub conversation_count: i64,
    pub thread_count: i64,
    pub identity_count: i64,
}

pub struct LibsqlRepository {
    db: Database,
}

impl LibsqlRepository {
    pub async fn new(database_url: &str) -> Result<Self> {
        let db = Builder::new_local(database_url).build().await?;
        Ok(Self { db })
    }

    async fn execute(&self, sql: &str, params: impl libsql::params::IntoParams) -> Result<()> {
        let conn = self.db.connect()?;
        conn.execute(sql, params).await?;
        Ok(())
    }

    async fn query<T, F>(&self, sql: &str, params: impl libsql::params::IntoParams, mut f: F) -> Result<Vec<T>>
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

#[async_trait::async_trait]
impl DatabaseRepository for LibsqlRepository {
    async fn run_migration(&self) -> Result<()> {
        let migration_001 = include_str!("../../migrations/001_init.sql");
        let migration_002 = include_str!("../../migrations/002_add_session.sql");
        let migration_003 = include_str!("../../migrations/003_add_search_indexes.sql");
        let migration_004 = include_str!("../../migrations/004_add_filters_and_activity.sql");

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

        Ok(())
    }

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

        let rows = self
            .query(sql, [thread_root_uri], |row| {
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
            .await?;

        Ok(rows)
    }

    async fn get_all_threads(&self, limit: usize) -> Result<Vec<String>> {
        let sql = r#"
            SELECT thread_root_uri
            FROM conversations
            GROUP BY thread_root_uri
            ORDER BY MAX(created_at) DESC
            LIMIT ?
        "#;

        let threads = self
            .query(sql, [limit as i64], |row| Ok::<String, anyhow::Error>(row.get(0)?))
            .await?;

        Ok(threads)
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
        let threads = self
            .query(sql, [author_did, limit.as_str()], |row| {
                Ok::<String, anyhow::Error>(row.get(0)?)
            })
            .await?;

        Ok(threads)
    }

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

        let rows = self
            .query(sql, (), |row| {
                Ok(IdentityRow {
                    did: row.get(0)?,
                    handle: row.get(1)?,
                    last_updated: DateTime::parse_from_rfc3339(&row.get::<String>(2)?)
                        .unwrap()
                        .with_timezone(&Utc),
                })
            })
            .await?;

        Ok(rows)
    }

    async fn get_stats(&self) -> Result<DatabaseStats> {
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

        Ok(DatabaseStats {
            conversation_count: *conversation_count,
            thread_count: *thread_count,
            identity_count: *identity_count,
        })
    }

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

    async fn search_conversations(
        &self, query: &str, author_filter: Option<&str>, role_filter: Option<&str>, date_from: Option<DateTime<Utc>>,
        date_to: Option<DateTime<Utc>>, limit: usize,
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

        let rows = self
            .query(sql, (), |row| {
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
            .await?;

        Ok(rows)
    }

    async fn export_thread(&self, thread_root_uri: &str) -> Result<Vec<ConversationRow>> {
        let sql = r#"
            SELECT id, thread_root_uri, post_uri, parent_uri,
                   author_did, role, content, created_at
            FROM conversations
            WHERE thread_root_uri = ?
            ORDER BY created_at ASC
        "#;

        let rows = self
            .query(sql, [thread_root_uri], |row| {
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
            .await?;

        Ok(rows)
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

        let rows = self
            .query(sql, (), |row| {
                Ok(MutedAuthorRow {
                    did: row.get(0)?,
                    muted_at: DateTime::parse_from_rfc3339(&row.get::<String>(1)?)
                        .unwrap()
                        .with_timezone(&Utc),
                    muted_by: row.get(2)?,
                })
            })
            .await?;

        Ok(rows)
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

        let rows = self
            .query(sql, [user_did], |row| {
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
            .await?;

        Ok(rows)
    }

    async fn get_conversations_with_length_filter(&self, min_messages: usize, limit: usize) -> Result<Vec<String>> {
        let sql = r#"
            SELECT thread_root_uri
            FROM conversations
            GROUP BY thread_root_uri
            HAVING COUNT(*) >= ?
            ORDER BY MAX(created_at) DESC
            LIMIT ?
        "#;

        let threads = self
            .query(sql, [min_messages as i64, limit as i64], |row| {
                Ok::<String, anyhow::Error>(row.get(0)?)
            })
            .await?;

        Ok(threads)
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

    async fn get_activity_log(&self, action_type: Option<&str>, limit: usize) -> Result<Vec<ActivityLogRow>> {
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

pub type Db = Arc<dyn DatabaseRepository>;
