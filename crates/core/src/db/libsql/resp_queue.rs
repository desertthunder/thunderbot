//! Response queue repository implementation for libSQL.

use anyhow::Result;
use chrono::Utc;
use libsql::params;

use crate::control::{ResponseQueueItem, ResponseStatus};
use crate::db::libsql::LibsqlRepository;
use crate::db::traits::ResponseQueueRepository;

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
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<String>(8)?)
                    .unwrap()
                    .with_timezone(&Utc),
                expires_at: row
                    .get::<Option<String>>(9)?
                    .map(|s| chrono::DateTime::parse_from_rfc3339(&s).unwrap().with_timezone(&Utc)),
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
            created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<String>(8)?)
                .unwrap()
                .with_timezone(&Utc),
            expires_at: row
                .get::<Option<String>>(9)?
                .map(|s| chrono::DateTime::parse_from_rfc3339(&s).unwrap().with_timezone(&Utc)),
        })
    }

    async fn update_response_status(&self, id: &str, status: ResponseStatus) -> Result<()> {
        let sql = "UPDATE response_queue SET status = ?1 WHERE id = ?2";
        self.execute(sql, params![format!("{:?}", status).as_str(), id]).await
    }

    async fn update_response_content(&self, id: &str, content: &str) -> Result<()> {
        let sql = "UPDATE response_queue SET content = ?1, status = ?2 WHERE id = ?3";
        self.execute(
            sql,
            params![content, format!("{:?}", ResponseStatus::Edited).as_str(), id],
        )
        .await
    }
}
