//! Activity repository implementation for libSQL.

use anyhow::Result;
use chrono::Utc;
use libsql::params;

use crate::db::libsql::LibsqlRepository;
use crate::db::traits::ActivityRepository;
use crate::db::types::ActivityLogRow;

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
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<String>(5)?)
                    .unwrap()
                    .with_timezone(&Utc),
            });
        }

        Ok(results)
    }
}
