use crate::db::models::{Conversation, CursorState, FailedEvent, Identity, Role};
use crate::db::models::{CreateConversationParams, CreateFailedEventParams, CreateIdentityParams, UpdateCursorParams};
use crate::error::BotError;
use async_trait::async_trait;
use libsql::Connection;

/// Repository trait for conversation operations
#[async_trait]
pub trait ConversationRepository: Send + Sync {
    /// Create a new conversation entry (idempotent via post_uri UNIQUE constraint)
    async fn create_conversation(&self, params: CreateConversationParams) -> Result<bool, BotError>;

    /// Get a conversation by post_uri
    async fn get_by_post_uri(&self, post_uri: &str) -> Result<Option<Conversation>, BotError>;

    /// Get all conversations in a thread ordered by created_at
    async fn get_thread_by_root(&self, root_uri: &str) -> Result<Vec<Conversation>, BotError>;

    /// Get recent conversations with pagination
    async fn get_recent(&self, limit: i64, offset: i64) -> Result<Vec<Conversation>, BotError>;

    /// Count total conversations
    async fn count(&self) -> Result<i64, BotError>;

    /// Get unique thread roots with most recent activity
    async fn get_recent_threads(&self, limit: i64) -> Result<Vec<(String, i64)>, BotError>;
}

/// Repository trait for identity operations
#[async_trait]
pub trait IdentityRepository: Send + Sync {
    /// Create or update an identity
    async fn upsert_identity(&self, params: CreateIdentityParams) -> Result<(), BotError>;

    /// Get identity by DID
    async fn get_by_did(&self, did: &str) -> Result<Option<Identity>, BotError>;

    /// Get identity by handle
    async fn get_by_handle(&self, handle: &str) -> Result<Option<Identity>, BotError>;

    /// Get identities that need refresh (older than given timestamp)
    async fn get_stale_identities(&self, before: &str) -> Result<Vec<Identity>, BotError>;

    /// List all identities
    async fn list_all(&self) -> Result<Vec<Identity>, BotError>;

    /// Delete an identity by DID
    async fn delete(&self, did: &str) -> Result<bool, BotError>;
}

/// Repository trait for failed events (dead letter queue)
#[async_trait]
pub trait FailedEventRepository: Send + Sync {
    /// Create a failed event entry
    async fn create(&self, params: CreateFailedEventParams) -> Result<i64, BotError>;

    /// Get a failed event by ID
    async fn get_by_id(&self, id: i64) -> Result<Option<FailedEvent>, BotError>;

    /// Get failed events by post_uri
    async fn get_by_post_uri(&self, post_uri: &str) -> Result<Vec<FailedEvent>, BotError>;

    /// Get all failed events ordered by most recent
    async fn get_recent(&self, limit: i64) -> Result<Vec<FailedEvent>, BotError>;

    /// Increment attempt count and update last_tried
    async fn increment_attempts(&self, id: i64, error: &str) -> Result<bool, BotError>;

    /// Delete a failed event
    async fn delete(&self, id: i64) -> Result<bool, BotError>;
}

/// Repository trait for cursor state
#[async_trait]
pub trait CursorRepository: Send + Sync {
    /// Get the current cursor state
    async fn get(&self) -> Result<Option<CursorState>, BotError>;

    /// Update or insert cursor state
    async fn update(&self, params: UpdateCursorParams) -> Result<(), BotError>;
}

/// Implementation of all repository traits using libSQL
pub struct LibsqlRepository {
    conn: Connection,
}

impl LibsqlRepository {
    /// Create a new repository from a database connection
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }
}

#[async_trait]
impl ConversationRepository for LibsqlRepository {
    async fn create_conversation(&self, params: CreateConversationParams) -> Result<bool, BotError> {
        let result = self
            .conn
            .execute(
                "INSERT OR IGNORE INTO conversations
                 (root_uri, post_uri, parent_uri, author_did, role, content, cid, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                (
                    params.root_uri,
                    params.post_uri,
                    params.parent_uri,
                    params.author_did,
                    params.role.to_string(),
                    params.content,
                    params.cid,
                    params.created_at,
                ),
            )
            .await
            .map_err(|e| {
                tracing::error!("Failed to create conversation: {}", e);
                BotError::Database(format!("Failed to create conversation: {}", e))
            })?;

        Ok(result > 0)
    }

    async fn get_by_post_uri(&self, post_uri: &str) -> Result<Option<Conversation>, BotError> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, root_uri, post_uri, parent_uri, author_did, role, content, cid, created_at
                 FROM conversations WHERE post_uri = ?1",
                [post_uri],
            )
            .await
            .map_err(|e| BotError::Database(format!("Failed to get conversation: {}", e)))?;

        if let Ok(Some(row)) = rows.next().await {
            Ok(Some(parse_conversation_row(&row)?))
        } else {
            Ok(None)
        }
    }

    async fn get_thread_by_root(&self, root_uri: &str) -> Result<Vec<Conversation>, BotError> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, root_uri, post_uri, parent_uri, author_did, role, content, cid, created_at
                 FROM conversations WHERE root_uri = ?1 ORDER BY created_at ASC",
                [root_uri],
            )
            .await
            .map_err(|e| BotError::Database(format!("Failed to get thread conversations: {}", e)))?;

        let mut conversations = Vec::new();
        while let Ok(Some(row)) = rows.next().await {
            conversations.push(parse_conversation_row(&row)?);
        }

        Ok(conversations)
    }

    async fn get_recent(&self, limit: i64, offset: i64) -> Result<Vec<Conversation>, BotError> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, root_uri, post_uri, parent_uri, author_did, role, content, cid, created_at
                 FROM conversations ORDER BY created_at DESC LIMIT ?1 OFFSET ?2",
                (limit, offset),
            )
            .await
            .map_err(|e| BotError::Database(format!("Failed to get recent conversations: {}", e)))?;

        let mut conversations = Vec::new();
        while let Ok(Some(row)) = rows.next().await {
            conversations.push(parse_conversation_row(&row)?);
        }

        Ok(conversations)
    }

    async fn count(&self) -> Result<i64, BotError> {
        let mut rows = self
            .conn
            .query("SELECT COUNT(*) FROM conversations", ())
            .await
            .map_err(|e| BotError::Database(format!("Failed to count conversations: {}", e)))?;

        if let Ok(Some(row)) = rows.next().await {
            row.get::<i64>(0)
                .map_err(|e| BotError::Database(format!("Failed to parse count: {}", e)))
        } else {
            Ok(0)
        }
    }

    async fn get_recent_threads(&self, limit: i64) -> Result<Vec<(String, i64)>, BotError> {
        let mut rows = self
            .conn
            .query(
                "SELECT root_uri, CAST(strftime('%s', MAX(created_at)) AS INTEGER) * 1000000 as last_activity_us
                 FROM conversations
                 GROUP BY root_uri
                 ORDER BY MAX(created_at) DESC
                 LIMIT ?1",
                [limit],
            )
            .await
            .map_err(|e| BotError::Database(format!("Failed to get recent threads: {}", e)))?;

        let mut threads = Vec::new();
        while let Ok(Some(row)) = rows.next().await {
            let root_uri: String = row
                .get(0)
                .map_err(|e| BotError::Database(format!("Failed to parse root_uri: {}", e)))?;
            let last_activity_us: i64 = row
                .get(1)
                .map_err(|e| BotError::Database(format!("Failed to parse last_activity_us: {}", e)))?;
            threads.push((root_uri, last_activity_us));
        }

        Ok(threads)
    }
}

#[async_trait]
impl IdentityRepository for LibsqlRepository {
    async fn upsert_identity(&self, params: CreateIdentityParams) -> Result<(), BotError> {
        self.conn
            .execute(
                "INSERT INTO identities (did, handle, display_name, last_updated)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(did) DO UPDATE SET
                 handle = excluded.handle,
                 display_name = excluded.display_name,
                 last_updated = excluded.last_updated",
                (params.did, params.handle, params.display_name, params.last_updated),
            )
            .await
            .map_err(|e| {
                tracing::error!("Failed to upsert identity: {}", e);
                BotError::Database(format!("Failed to upsert identity: {}", e))
            })?;

        Ok(())
    }

    async fn get_by_did(&self, did: &str) -> Result<Option<Identity>, BotError> {
        let mut rows = self
            .conn
            .query(
                "SELECT did, handle, display_name, last_updated FROM identities WHERE did = ?1",
                [did],
            )
            .await
            .map_err(|e| BotError::Database(format!("Failed to get identity: {}", e)))?;

        if let Ok(Some(row)) = rows.next().await { Ok(Some(parse_identity_row(&row)?)) } else { Ok(None) }
    }

    async fn get_by_handle(&self, handle: &str) -> Result<Option<Identity>, BotError> {
        let mut rows = self
            .conn
            .query(
                "SELECT did, handle, display_name, last_updated FROM identities WHERE handle = ?1",
                [handle],
            )
            .await
            .map_err(|e| BotError::Database(format!("Failed to get identity: {}", e)))?;

        if let Ok(Some(row)) = rows.next().await { Ok(Some(parse_identity_row(&row)?)) } else { Ok(None) }
    }

    async fn get_stale_identities(&self, before: &str) -> Result<Vec<Identity>, BotError> {
        let mut rows = self
            .conn
            .query(
                "SELECT did, handle, display_name, last_updated
                 FROM identities
                 WHERE last_updated < ?1
                 ORDER BY last_updated ASC",
                [before],
            )
            .await
            .map_err(|e| BotError::Database(format!("Failed to get stale identities: {}", e)))?;

        let mut identities = Vec::new();
        while let Ok(Some(row)) = rows.next().await {
            identities.push(parse_identity_row(&row)?);
        }

        Ok(identities)
    }

    async fn list_all(&self) -> Result<Vec<Identity>, BotError> {
        let mut rows = self
            .conn
            .query(
                "SELECT did, handle, display_name, last_updated FROM identities ORDER BY last_updated DESC",
                (),
            )
            .await
            .map_err(|e| BotError::Database(format!("Failed to list identities: {}", e)))?;

        let mut identities = Vec::new();
        while let Ok(Some(row)) = rows.next().await {
            identities.push(parse_identity_row(&row)?);
        }

        Ok(identities)
    }

    async fn delete(&self, did: &str) -> Result<bool, BotError> {
        let result = self
            .conn
            .execute("DELETE FROM identities WHERE did = ?1", [did])
            .await
            .map_err(|e| BotError::Database(format!("Failed to delete identity: {}", e)))?;

        Ok(result > 0)
    }
}

#[async_trait]
impl FailedEventRepository for LibsqlRepository {
    async fn create(&self, params: CreateFailedEventParams) -> Result<i64, BotError> {
        self.conn
            .execute(
                "INSERT INTO failed_events (post_uri, event_json, error, created_at, last_tried)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                (
                    params.post_uri,
                    params.event_json,
                    params.error,
                    params.created_at,
                    params.last_tried,
                ),
            )
            .await
            .map_err(|e| {
                tracing::error!("Failed to create failed event: {}", e);
                BotError::Database(format!("Failed to create failed event: {}", e))
            })?;

        let mut rows = self
            .conn
            .query("SELECT last_insert_rowid()", ())
            .await
            .map_err(|e| BotError::Database(format!("Failed to get last insert rowid: {}", e)))?;

        if let Ok(Some(row)) = rows.next().await {
            row.get::<i64>(0)
                .map_err(|e| BotError::Database(format!("Failed to parse rowid: {}", e)))
        } else {
            Ok(0)
        }
    }

    async fn get_by_id(&self, id: i64) -> Result<Option<FailedEvent>, BotError> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, post_uri, event_json, error, attempts, created_at, last_tried
                 FROM failed_events WHERE id = ?1",
                [id],
            )
            .await
            .map_err(|e| BotError::Database(format!("Failed to get failed event: {}", e)))?;

        if let Ok(Some(row)) = rows.next().await {
            Ok(Some(parse_failed_event_row(&row)?))
        } else {
            Ok(None)
        }
    }

    async fn get_by_post_uri(&self, post_uri: &str) -> Result<Vec<FailedEvent>, BotError> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, post_uri, event_json, error, attempts, created_at, last_tried
                 FROM failed_events WHERE post_uri = ?1 ORDER BY created_at DESC",
                [post_uri],
            )
            .await
            .map_err(|e| BotError::Database(format!("Failed to get failed events by post_uri: {}", e)))?;

        let mut events = Vec::new();
        while let Ok(Some(row)) = rows.next().await {
            events.push(parse_failed_event_row(&row)?);
        }

        Ok(events)
    }

    async fn get_recent(&self, limit: i64) -> Result<Vec<FailedEvent>, BotError> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, post_uri, event_json, error, attempts, created_at, last_tried
                 FROM failed_events ORDER BY created_at DESC LIMIT ?1",
                [limit],
            )
            .await
            .map_err(|e| BotError::Database(format!("Failed to get recent failed events: {}", e)))?;

        let mut events = Vec::new();
        while let Ok(Some(row)) = rows.next().await {
            events.push(parse_failed_event_row(&row)?);
        }

        Ok(events)
    }

    async fn increment_attempts(&self, id: i64, error: &str) -> Result<bool, BotError> {
        let now = chrono::Utc::now().to_rfc3339();
        let result = self
            .conn
            .execute(
                "UPDATE failed_events
                 SET attempts = attempts + 1, error = ?1, last_tried = ?2
                 WHERE id = ?3",
                (error, now.as_str(), id),
            )
            .await
            .map_err(|e| BotError::Database(format!("Failed to increment attempts: {}", e)))?;

        Ok(result > 0)
    }

    async fn delete(&self, id: i64) -> Result<bool, BotError> {
        let result = self
            .conn
            .execute("DELETE FROM failed_events WHERE id = ?1", [id])
            .await
            .map_err(|e| BotError::Database(format!("Failed to delete failed event: {}", e)))?;

        Ok(result > 0)
    }
}

#[async_trait]
impl CursorRepository for LibsqlRepository {
    async fn get(&self) -> Result<Option<CursorState>, BotError> {
        let mut rows = self
            .conn
            .query("SELECT id, time_us, updated FROM cursor_state WHERE id = 1", ())
            .await
            .map_err(|e| BotError::Database(format!("Failed to get cursor state: {}", e)))?;

        if let Ok(Some(row)) = rows.next().await {
            Ok(Some(parse_cursor_state_row(&row)?))
        } else {
            Ok(None)
        }
    }

    async fn update(&self, params: UpdateCursorParams) -> Result<(), BotError> {
        self.conn
            .execute(
                "INSERT INTO cursor_state (id, time_us, updated)
                 VALUES (1, ?1, ?2)
                 ON CONFLICT(id) DO UPDATE SET
                 time_us = excluded.time_us,
                 updated = excluded.updated",
                (params.time_us, params.updated),
            )
            .await
            .map_err(|e| {
                tracing::error!("Failed to update cursor state: {}", e);
                BotError::Database(format!("Failed to update cursor state: {}", e))
            })?;

        Ok(())
    }
}

fn parse_conversation_row(row: &libsql::Row) -> Result<Conversation, BotError> {
    let role_str: String = row
        .get(5)
        .map_err(|e| BotError::Database(format!("Failed to parse role: {}", e)))?;
    let role = Role::try_from(role_str.as_str())
        .map_err(|e| BotError::Database(format!("Invalid role in database: {}", e)))?;

    Ok(Conversation {
        id: row
            .get(0)
            .map_err(|e| BotError::Database(format!("Failed to parse id: {}", e)))?,
        root_uri: row
            .get(1)
            .map_err(|e| BotError::Database(format!("Failed to parse root_uri: {}", e)))?,
        post_uri: row
            .get(2)
            .map_err(|e| BotError::Database(format!("Failed to parse post_uri: {}", e)))?,
        parent_uri: row
            .get(3)
            .map_err(|e| BotError::Database(format!("Failed to parse parent_uri: {}", e)))?,
        author_did: row
            .get(4)
            .map_err(|e| BotError::Database(format!("Failed to parse author_did: {}", e)))?,
        role,
        content: row
            .get(6)
            .map_err(|e| BotError::Database(format!("Failed to parse content: {}", e)))?,
        cid: row
            .get(7)
            .map_err(|e| BotError::Database(format!("Failed to parse cid: {}", e)))?,
        created_at: row
            .get(8)
            .map_err(|e| BotError::Database(format!("Failed to parse created_at: {}", e)))?,
    })
}

fn parse_identity_row(row: &libsql::Row) -> Result<Identity, BotError> {
    Ok(Identity {
        did: row
            .get(0)
            .map_err(|e| BotError::Database(format!("Failed to parse did: {}", e)))?,
        handle: row
            .get(1)
            .map_err(|e| BotError::Database(format!("Failed to parse handle: {}", e)))?,
        display_name: row
            .get(2)
            .map_err(|e| BotError::Database(format!("Failed to parse display_name: {}", e)))?,
        last_updated: row
            .get(3)
            .map_err(|e| BotError::Database(format!("Failed to parse last_updated: {}", e)))?,
    })
}

fn parse_failed_event_row(row: &libsql::Row) -> Result<FailedEvent, BotError> {
    Ok(FailedEvent {
        id: row
            .get(0)
            .map_err(|e| BotError::Database(format!("Failed to parse id: {}", e)))?,
        post_uri: row
            .get(1)
            .map_err(|e| BotError::Database(format!("Failed to parse post_uri: {}", e)))?,
        event_json: row
            .get(2)
            .map_err(|e| BotError::Database(format!("Failed to parse event_json: {}", e)))?,
        error: row
            .get(3)
            .map_err(|e| BotError::Database(format!("Failed to parse error: {}", e)))?,
        attempts: row
            .get(4)
            .map_err(|e| BotError::Database(format!("Failed to parse attempts: {}", e)))?,
        created_at: row
            .get(5)
            .map_err(|e| BotError::Database(format!("Failed to parse created_at: {}", e)))?,
        last_tried: row
            .get(6)
            .map_err(|e| BotError::Database(format!("Failed to parse last_tried: {}", e)))?,
    })
}

fn parse_cursor_state_row(row: &libsql::Row) -> Result<CursorState, BotError> {
    Ok(CursorState {
        id: row
            .get(0)
            .map_err(|e| BotError::Database(format!("Failed to parse id: {}", e)))?,
        time_us: row
            .get(1)
            .map_err(|e| BotError::Database(format!("Failed to parse time_us: {}", e)))?,
        updated: row
            .get(2)
            .map_err(|e| BotError::Database(format!("Failed to parse updated: {}", e)))?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tempfile::TempDir;

    async fn setup_test_repo() -> (LibsqlRepository, libsql::Database, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let db_path = temp_dir.path().join(format!("repo_test_{ts}.db"));
        let db = libsql::Builder::new_local(db_path.to_str().unwrap())
            .build()
            .await
            .unwrap();
        migrations::run_migrations(&db).await.unwrap();
        let conn = db.connect().unwrap();
        (LibsqlRepository::new(conn), db, temp_dir)
    }

    #[tokio::test]
    async fn test_get_thread_by_root_returns_chronological_order() {
        let (repo, _db, _temp_dir) = setup_test_repo().await;
        let root_uri = "at://did:plc:root/app.bsky.feed.post/root";

        let rows = vec![
            ("at://did:plc:u1/app.bsky.feed.post/1", "2024-01-01T00:00:02Z"),
            ("at://did:plc:u1/app.bsky.feed.post/2", "2024-01-01T00:00:01Z"),
            ("at://did:plc:u1/app.bsky.feed.post/3", "2024-01-01T00:00:03Z"),
        ];

        for (post_uri, created_at) in rows {
            repo.create_conversation(CreateConversationParams {
                root_uri: root_uri.to_string(),
                post_uri: post_uri.to_string(),
                parent_uri: None,
                author_did: "did:plc:u1".to_string(),
                role: Role::User,
                content: "hello".to_string(),
                cid: None,
                created_at: created_at.to_string(),
            })
            .await
            .unwrap();
        }

        let thread = repo.get_thread_by_root(root_uri).await.unwrap();
        let ordered_post_uris: Vec<&str> = thread.iter().map(|r| r.post_uri.as_str()).collect();
        assert_eq!(
            ordered_post_uris,
            vec![
                "at://did:plc:u1/app.bsky.feed.post/2",
                "at://did:plc:u1/app.bsky.feed.post/1",
                "at://did:plc:u1/app.bsky.feed.post/3"
            ]
        );
    }

    #[tokio::test]
    async fn test_get_recent_threads_includes_last_activity_timestamp() {
        let (repo, _db, _temp_dir) = setup_test_repo().await;
        let root_uri = "at://did:plc:root/app.bsky.feed.post/root";

        repo.create_conversation(CreateConversationParams {
            root_uri: root_uri.to_string(),
            post_uri: "at://did:plc:u1/app.bsky.feed.post/1".to_string(),
            parent_uri: None,
            author_did: "did:plc:u1".to_string(),
            role: Role::User,
            content: "hello".to_string(),
            cid: None,
            created_at: "2024-01-01T00:00:00Z".to_string(),
        })
        .await
        .unwrap();

        let threads = repo.get_recent_threads(10).await.unwrap();
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].0, root_uri);
        assert!(threads[0].1 > 0);
    }
}
