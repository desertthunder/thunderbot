//! Search repository implementation for libSQL.

use anyhow::Result;
use chrono::{DateTime, Utc};

use crate::db::libsql::LibsqlRepository;
use crate::db::traits::{FilterRepository, SearchRepository};
use crate::db::types::{ConversationRow, FilterPresetRow};

#[async_trait::async_trait]
impl SearchRepository for LibsqlRepository {
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

    async fn get_conversations_with_length_filter(&self, min_messages: usize, limit: usize) -> Result<Vec<String>> {
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
            libsql::params![
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
