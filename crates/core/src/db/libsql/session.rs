//! Session repository implementation for libSQL.

use anyhow::Result;
use chrono::Utc;

use crate::db::libsql::LibsqlRepository;
use crate::db::traits::SessionRepository;
use crate::db::types::SessionRow;

#[async_trait::async_trait]
impl SessionRepository for LibsqlRepository {
    async fn save_session(&self, row: SessionRow) -> Result<()> {
        let sql = r#"
            INSERT OR REPLACE INTO sessions (did, handle, access_jwt, refresh_jwt, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
        "#;

        self.execute(
            sql,
            libsql::params![
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
                    updated_at: chrono::DateTime::parse_from_rfc3339(&row.get::<String>(4)?)
                        .unwrap()
                        .with_timezone(&Utc),
                })
            })
            .await?;

        Ok(rows.into_iter().next())
    }
}
