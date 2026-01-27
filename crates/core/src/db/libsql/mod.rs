//! libSQL implementation of all database repository traits.

mod activity;
mod control;
mod identity;
mod operational;
mod resp_queue;
mod search;
mod session;
mod thread;

use anyhow::Result;
use libsql::{Builder, Database};

use crate::db::DatabaseRepository;

/// libSQL-based implementation of all database repository traits.
pub struct LibsqlRepository {
    pub(super) db: Database,
}

impl LibsqlRepository {
    /// Create a new libSQL repository instance.
    pub async fn new(database_url: &str) -> Result<Self> {
        let db = Builder::new_local(database_url).build().await?;
        Ok(Self { db })
    }

    /// Execute a SQL statement with parameters.
    pub(super) async fn execute(&self, sql: &str, params: impl libsql::params::IntoParams) -> Result<()> {
        let conn = self.db.connect()?;
        conn.execute(sql, params).await?;
        Ok(())
    }

    /// Execute a SQL query and map results.
    pub(super) async fn query<T, F>(
        &self, sql: &str, params: impl libsql::params::IntoParams, mut f: F,
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

#[async_trait::async_trait]
impl DatabaseRepository for LibsqlRepository {
    async fn run_migration(&self) -> Result<()> {
        let migration_001 = include_str!("../../../migrations/001_init.sql");
        let migration_002 = include_str!("../../../migrations/002_add_session.sql");
        let migration_003 = include_str!("../../../migrations/003_add_search_indexes.sql");
        let migration_004 = include_str!("../../../migrations/004_add_filters_and_activity.sql");
        let migration_005 = include_str!("../../../migrations/005_operational_controls.sql");

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
