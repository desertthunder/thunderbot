use anyhow::Result;
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
}

pub type Db = Arc<dyn DatabaseRepository>;
