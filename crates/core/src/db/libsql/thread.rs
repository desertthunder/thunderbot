//! Thread repository implementation for libSQL.

use anyhow::Result;
use chrono::Utc;
use libsql::params;

use crate::db::libsql::LibsqlRepository;
use crate::db::traits::ThreadRepository;
use crate::db::types::{ConversationRow, DatabaseStats, MutedAuthorRow};

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
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<String>(7)?)
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
                muted_at: chrono::DateTime::parse_from_rfc3339(&row.get::<String>(1)?)
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
