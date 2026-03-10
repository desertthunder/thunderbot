use libsql::{Connection, Database};

use crate::error::BotError;

/// List of all migrations to be applied
const MIGRATIONS: &[(&str, &str)] = &[("001_initial", include_str!("../../migrations/001_initial.sql"))];

/// Runs all pending migrations on the database
pub async fn run_migrations(db: &Database) -> Result<(), BotError> {
    let conn = db
        .connect()
        .map_err(|e| BotError::Database(format!("Failed to connect to database: {}", e)))?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS _migrations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            applied_at TEXT NOT NULL
        )",
        (),
    )
    .await
    .map_err(|e| BotError::Database(format!("Failed to create migrations table: {}", e)))?;

    let applied_migrations = get_applied_migrations(&conn).await?;

    for (name, sql) in MIGRATIONS {
        if applied_migrations.contains(&name.to_string()) {
            tracing::info!("Migration {} already applied, skipping", name);
            continue;
        }

        tracing::info!("Applying migration: {}", name);

        conn.execute_batch(sql).await.map_err(|e| {
            tracing::error!("Failed to apply migration {}: {}", name, e);
            BotError::Database(format!("Migration {} failed: {}", name, e))
        })?;

        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO _migrations (name, applied_at) VALUES (?1, ?2)",
            (*name, now.as_str()),
        )
        .await
        .map_err(|e| BotError::Database(format!("Failed to record migration {}: {}", name, e)))?;

        tracing::info!("Successfully applied migration: {}", name);
    }

    tracing::info!("All migrations applied successfully");
    Ok(())
}

/// Get list of already applied migration names
async fn get_applied_migrations(conn: &Connection) -> Result<Vec<String>, BotError> {
    let mut rows = conn
        .query(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='_migrations'",
            (),
        )
        .await
        .map_err(|e| BotError::Database(format!("Failed to check migrations table: {}", e)))?;

    let table_exists = rows.next().await.ok().flatten().is_some();

    if !table_exists {
        return Ok(Vec::new());
    }

    let mut rows = conn
        .query("SELECT name FROM _migrations ORDER BY id", ())
        .await
        .map_err(|e| BotError::Database(format!("Failed to query migrations: {}", e)))?;

    let mut migrations = Vec::new();
    while let Ok(Some(row)) = rows.next().await {
        if let Ok(name) = row.get::<String>(0) {
            migrations.push(name);
        }
    }

    Ok(migrations)
}

/// Check if all migrations are up to date
pub async fn check_migrations(db: &Database) -> Result<bool, BotError> {
    let conn = db
        .connect()
        .map_err(|e| BotError::Database(format!("Failed to connect to database: {}", e)))?;

    let applied = get_applied_migrations(&conn).await?;
    let expected: Vec<&str> = MIGRATIONS.iter().map(|(name, _)| *name).collect();

    Ok(applied.len() == expected.len() && expected.iter().all(|name| applied.contains(&name.to_string())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tempfile::TempDir;

    async fn create_test_db() -> (Database, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        // Use unique database names to avoid conflicts between parallel tests
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = temp_dir.path().join(format!("test_{}.db", timestamp));
        let db = libsql::Builder::new_local(db_path.to_str().unwrap())
            .build()
            .await
            .unwrap();
        (db, temp_dir)
    }

    #[tokio::test]
    async fn test_run_migrations() {
        let (db, _) = create_test_db().await;

        run_migrations(&db).await.expect("Failed to run migrations");

        let conn = db.connect().unwrap();
        let mut rows = conn
            .query("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name", ())
            .await
            .unwrap();

        let mut tables = Vec::new();
        while let Ok(Some(row)) = rows.next().await {
            if let Ok(name) = row.get::<String>(0) {
                tables.push(name);
            }
        }

        assert!(tables.contains(&"conversations".to_string()));
        assert!(tables.contains(&"identities".to_string()));
        assert!(tables.contains(&"failed_events".to_string()));
        assert!(tables.contains(&"cursor_state".to_string()));
        assert!(tables.contains(&"_migrations".to_string()));
    }

    #[tokio::test]
    async fn test_check_migrations() {
        let (db, _) = create_test_db().await;

        let result = check_migrations(&db).await.expect("Failed to check migrations");
        assert!(!result);

        run_migrations(&db).await.expect("Failed to run migrations");

        let result = check_migrations(&db).await.expect("Failed to check migrations");
        assert!(result);
    }
}
