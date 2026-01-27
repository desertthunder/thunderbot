//! Identity repository implementation for libSQL.

use anyhow::Result;
use chrono::Utc;

use crate::db::libsql::LibsqlRepository;
use crate::db::traits::IdentityRepository;
use crate::db::types::IdentityRow;

#[async_trait::async_trait]
impl IdentityRepository for LibsqlRepository {
    async fn save_identity(&self, row: IdentityRow) -> Result<()> {
        let sql = r#"
            INSERT OR REPLACE INTO identities (did, handle, last_updated)
            VALUES (?1, ?2, ?3)
        "#;

        self.execute(
            sql,
            libsql::params![
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

        self.execute(sql, libsql::params![did, handle, Utc::now().to_rfc3339().as_str()])
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
                    last_updated: chrono::DateTime::parse_from_rfc3339(&row.get::<String>(2)?)
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
                last_updated: chrono::DateTime::parse_from_rfc3339(&row.get::<String>(2)?)
                    .unwrap()
                    .with_timezone(&Utc),
            })
        })
        .await
    }
}
