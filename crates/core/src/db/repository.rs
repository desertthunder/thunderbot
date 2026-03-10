use crate::db::models::{Conversation, CursorState, FailedEvent, Identity, Memory, Role};
use crate::db::models::{
    CreateConversationParams, CreateFailedEventParams, CreateIdentityParams, CreateMemoryParams, UpdateCursorParams,
};
use crate::error::BotError;
use async_trait::async_trait;
use chrono::Utc;
use libsql::Connection;

/// Repository trait for conversation operations
#[async_trait]
pub trait ConversationRepository: Send + Sync {
    /// Create a new conversation entry (idempotent via post_uri UNIQUE constraint)
    async fn create_conversation(&self, params: CreateConversationParams) -> Result<bool, BotError>;

    /// Get a conversation by id
    async fn get_by_id(&self, id: i64) -> Result<Option<Conversation>, BotError>;

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

/// Repository trait for memory/embedding operations
#[async_trait]
pub trait MemoryRepository: Send + Sync {
    /// Create a new memory entry
    async fn create_memory(
        &self, conversation_id: i64, root_uri: &str, content: &str, embedding: &[f32], author_did: &str,
        created_at: &str,
    ) -> Result<i64, BotError>;

    /// Create a new memory entry with metadata, expiry and deterministic content hash.
    async fn create_memory_with_params(&self, params: CreateMemoryParams) -> Result<i64, BotError>;

    /// Get memories by root URI
    async fn get_memories_by_root(&self, root_uri: &str) -> Result<Vec<Memory>, BotError>;

    /// Get a memory by root URI and content hash (used for deterministic deduplication).
    async fn get_memory_by_root_and_hash(&self, root_uri: &str, content_hash: &str)
    -> Result<Option<Memory>, BotError>;

    /// Search memories by semantic similarity
    async fn search_semantic(&self, query_embedding: &[f32], top_k: usize) -> Result<Vec<Memory>, BotError>;

    /// Search memories semantically with optional metadata filters.
    async fn search_semantic_filtered(
        &self, query_embedding: &[f32], top_k: usize, author_did: Option<&str>, time_after: Option<&str>,
        root_uri: Option<&str>, exclude_root_uri: Option<&str>,
    ) -> Result<Vec<Memory>, BotError>;

    /// Search memories by author
    async fn search_by_author(
        &self, author_did: &str, query_embedding: &[f32], top_k: usize,
    ) -> Result<Vec<Memory>, BotError>;

    /// Search memories using FTS5 keyword matching with optional filters.
    async fn search_keyword(
        &self, query: &str, top_k: usize, author_did: Option<&str>, time_after: Option<&str>, root_uri: Option<&str>,
        exclude_root_uri: Option<&str>,
    ) -> Result<Vec<Memory>, BotError>;

    /// Delete expired memories
    async fn delete_expired(&self) -> Result<u64, BotError>;

    /// Delete all memories for a given root URI.
    async fn delete_memories_by_root(&self, root_uri: &str) -> Result<u64, BotError>;

    /// Count total memories
    async fn count_memories(&self) -> Result<i64, BotError>;

    /// Create an embedding job
    async fn create_embedding_job(&self, conversation_id: i64, created_at: &str) -> Result<i64, BotError>;

    /// Get pending embedding jobs
    async fn get_pending_jobs(&self, limit: i64, max_attempts: u32) -> Result<Vec<(i64, i64, String, u32)>, BotError>;

    /// Mark an embedding job as complete.
    async fn complete_embedding_job(&self, job_id: i64) -> Result<(), BotError>;

    /// Record an embedding job failure. Returns `true` if retries are exhausted and the job is now failed.
    async fn fail_embedding_job(&self, job_id: i64, max_attempts: u32, error: &str) -> Result<bool, BotError>;

    /// Update embedding job status
    async fn update_embedding_job(&self, job_id: i64, status: &str, error: Option<&str>) -> Result<(), BotError>;
}

/// Implementation of all repository traits using libSQL
#[derive(Clone)]
pub struct LibsqlRepository {
    conn: Connection,
}

impl LibsqlRepository {
    /// Create a new repository from a database connection
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    /// Get the underlying connection (for pipeline backfill)
    pub fn conn(&self) -> &Connection {
        &self.conn
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

    async fn get_by_id(&self, id: i64) -> Result<Option<Conversation>, BotError> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, root_uri, post_uri, parent_uri, author_did, role, content, cid, created_at
                 FROM conversations WHERE id = ?1",
                [id],
            )
            .await
            .map_err(|e| BotError::Database(format!("Failed to get conversation by id: {}", e)))?;

        if let Ok(Some(row)) = rows.next().await {
            Ok(Some(parse_conversation_row(&row)?))
        } else {
            Ok(None)
        }
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

fn parse_memory_row(row: &libsql::Row) -> Result<Memory, BotError> {
    let embedding: Option<Vec<f32>> = row.get::<Vec<u8>>(4).ok().map(|bytes| {
        bytes
            .chunks_exact(4)
            .map(|chunk| {
                let arr: [u8; 4] = chunk.try_into().unwrap_or([0; 4]);
                f32::from_le_bytes(arr)
            })
            .collect()
    });

    let metadata: Option<serde_json::Value> = row.get::<String>(6).ok().and_then(|s| serde_json::from_str(&s).ok());

    Ok(Memory {
        id: row
            .get(0)
            .map_err(|e| BotError::Database(format!("Failed to parse id: {}", e)))?,
        conversation_id: row
            .get(1)
            .map_err(|e| BotError::Database(format!("Failed to parse conversation_id: {}", e)))?,
        root_uri: row
            .get(2)
            .map_err(|e| BotError::Database(format!("Failed to parse root_uri: {}", e)))?,
        content: row
            .get(3)
            .map_err(|e| BotError::Database(format!("Failed to parse content: {}", e)))?,
        embedding,
        author_did: row
            .get(5)
            .map_err(|e| BotError::Database(format!("Failed to parse author_did: {}", e)))?,
        metadata,
        content_hash: row
            .get(9)
            .map_err(|e| BotError::Database(format!("Failed to parse content_hash: {}", e)))?,
        created_at: row
            .get(7)
            .map_err(|e| BotError::Database(format!("Failed to parse created_at: {}", e)))?,
        expires_at: row
            .get(8)
            .map_err(|e| BotError::Database(format!("Failed to parse expires_at: {}", e)))?,
        distance: None,
    })
}

#[async_trait]
impl MemoryRepository for LibsqlRepository {
    async fn create_memory(
        &self, conversation_id: i64, root_uri: &str, content: &str, embedding: &[f32], author_did: &str,
        created_at: &str,
    ) -> Result<i64, BotError> {
        self.create_memory_with_params(CreateMemoryParams {
            conversation_id,
            root_uri: root_uri.to_string(),
            content: content.to_string(),
            embedding: embedding.to_vec(),
            author_did: author_did.to_string(),
            metadata: None,
            created_at: created_at.to_string(),
            expires_at: None,
            content_hash: None,
        })
        .await
    }

    async fn create_memory_with_params(&self, params: CreateMemoryParams) -> Result<i64, BotError> {
        let embedding_bytes: Vec<u8> = params.embedding.iter().flat_map(|f| f.to_le_bytes()).collect();
        let metadata_json = params.metadata.as_ref().map(serde_json::Value::to_string);

        self.conn
            .execute(
                "INSERT INTO memories
                 (conversation_id, root_uri, content, embedding, author_did, metadata, created_at, expires_at, content_hash)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                (
                    params.conversation_id,
                    params.root_uri.as_str(),
                    params.content.as_str(),
                    embedding_bytes,
                    params.author_did.as_str(),
                    metadata_json.as_deref(),
                    params.created_at.as_str(),
                    params.expires_at.as_deref(),
                    params.content_hash.as_deref(),
                ),
            )
            .await
            .map_err(|e| {
                tracing::error!("Failed to create memory: {}", e);
                BotError::Database(format!("Failed to create memory: {}", e))
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

    async fn get_memories_by_root(&self, root_uri: &str) -> Result<Vec<Memory>, BotError> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, conversation_id, root_uri, content, embedding, author_did, metadata, created_at, expires_at, content_hash
                 FROM memories WHERE root_uri = ?1 ORDER BY created_at DESC",
                [root_uri],
            )
            .await
            .map_err(|e| BotError::Database(format!("Failed to get memories by root: {}", e)))?;

        let mut memories = Vec::new();
        while let Ok(Some(row)) = rows.next().await {
            memories.push(parse_memory_row(&row)?);
        }

        Ok(memories)
    }

    async fn get_memory_by_root_and_hash(
        &self, root_uri: &str, content_hash: &str,
    ) -> Result<Option<Memory>, BotError> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, conversation_id, root_uri, content, embedding, author_did, metadata, created_at, expires_at, content_hash
                 FROM memories WHERE root_uri = ?1 AND content_hash = ?2
                 ORDER BY created_at DESC
                 LIMIT 1",
                (root_uri, content_hash),
            )
            .await
            .map_err(|e| BotError::Database(format!("Failed to get memory by hash: {}", e)))?;

        if let Ok(Some(row)) = rows.next().await { Ok(Some(parse_memory_row(&row)?)) } else { Ok(None) }
    }

    async fn search_semantic(&self, query_embedding: &[f32], top_k: usize) -> Result<Vec<Memory>, BotError> {
        self.search_semantic_filtered(query_embedding, top_k, None, None, None, None)
            .await
    }

    async fn search_semantic_filtered(
        &self, query_embedding: &[f32], top_k: usize, author_did: Option<&str>, time_after: Option<&str>,
        root_uri: Option<&str>, exclude_root_uri: Option<&str>,
    ) -> Result<Vec<Memory>, BotError> {
        let embedding_bytes: Vec<u8> = query_embedding.iter().flat_map(|f| f.to_le_bytes()).collect();
        let candidate_k = (top_k.max(1)).saturating_mul(10) as i64;

        let mut rows = self
            .conn
            .query(
                "SELECT
                    m.id,
                    m.conversation_id,
                    m.root_uri,
                    m.content,
                    m.embedding,
                    m.author_did,
                    m.metadata,
                    m.created_at,
                    m.expires_at,
                    m.content_hash,
                    vector_distance_cos(m.embedding, vector32(?1)) as distance
                 FROM vector_top_k(libsql_vector_idx, vector32(?1), ?2) AS top_k
                 JOIN memories AS m ON m.rowid = top_k.rowid
                 WHERE (?3 IS NULL OR m.author_did = ?3)
                   AND (?4 IS NULL OR m.created_at >= ?4)
                   AND (?5 IS NULL OR m.root_uri = ?5)
                   AND (?6 IS NULL OR m.root_uri != ?6)
                   AND (m.expires_at IS NULL OR julianday(m.expires_at) > julianday('now'))
                 ORDER BY distance ASC
                 LIMIT ?7",
                (
                    embedding_bytes,
                    candidate_k,
                    author_did,
                    time_after,
                    root_uri,
                    exclude_root_uri,
                    top_k as i64,
                ),
            )
            .await
            .map_err(|e| BotError::Database(format!("Failed to search semantic: {}", e)))?;

        let mut memories = Vec::new();
        while let Ok(Some(row)) = rows.next().await {
            let mut memory = parse_memory_row(&row)?;
            memory.distance = row.get::<f64>(10).ok();
            memories.push(memory);
        }

        Ok(memories)
    }

    async fn search_by_author(
        &self, author_did: &str, query_embedding: &[f32], top_k: usize,
    ) -> Result<Vec<Memory>, BotError> {
        self.search_semantic_filtered(query_embedding, top_k, Some(author_did), None, None, None)
            .await
    }

    async fn search_keyword(
        &self, query: &str, top_k: usize, author_did: Option<&str>, time_after: Option<&str>, root_uri: Option<&str>,
        exclude_root_uri: Option<&str>,
    ) -> Result<Vec<Memory>, BotError> {
        let mut rows = self
            .conn
            .query(
                "SELECT
                    m.id,
                    m.conversation_id,
                    m.root_uri,
                    m.content,
                    m.embedding,
                    m.author_did,
                    m.metadata,
                    m.created_at,
                    m.expires_at,
                    m.content_hash,
                    bm25(memories_fts) AS rank
                 FROM memories_fts
                 JOIN memories m ON m.id = memories_fts.rowid
                 WHERE memories_fts MATCH ?1
                   AND (?2 IS NULL OR m.author_did = ?2)
                   AND (?3 IS NULL OR m.created_at >= ?3)
                   AND (?4 IS NULL OR m.root_uri = ?4)
                   AND (?5 IS NULL OR m.root_uri != ?5)
                   AND (m.expires_at IS NULL OR julianday(m.expires_at) > julianday('now'))
                 ORDER BY rank ASC
                 LIMIT ?6",
                (query, author_did, time_after, root_uri, exclude_root_uri, top_k as i64),
            )
            .await
            .map_err(|e| BotError::Database(format!("Failed to search keyword: {}", e)))?;

        let mut memories = Vec::new();
        while let Ok(Some(row)) = rows.next().await {
            let mut memory = parse_memory_row(&row)?;
            memory.distance = row.get::<f64>(10).ok();
            memories.push(memory);
        }

        Ok(memories)
    }

    async fn delete_expired(&self) -> Result<u64, BotError> {
        let result = self
            .conn
            .execute(
                "DELETE FROM memories
                 WHERE expires_at IS NOT NULL
                   AND julianday(expires_at) <= julianday('now')",
                (),
            )
            .await
            .map_err(|e| BotError::Database(format!("Failed to delete expired memories: {}", e)))?;

        Ok(result as u64)
    }

    async fn delete_memories_by_root(&self, root_uri: &str) -> Result<u64, BotError> {
        let result = self
            .conn
            .execute("DELETE FROM memories WHERE root_uri = ?1", [root_uri])
            .await
            .map_err(|e| BotError::Database(format!("Failed to delete memories by root: {}", e)))?;
        Ok(result as u64)
    }

    async fn count_memories(&self) -> Result<i64, BotError> {
        let mut rows = self
            .conn
            .query("SELECT COUNT(*) FROM memories", ())
            .await
            .map_err(|e| BotError::Database(format!("Failed to count memories: {}", e)))?;

        if let Ok(Some(row)) = rows.next().await {
            row.get::<i64>(0)
                .map_err(|e| BotError::Database(format!("Failed to parse count: {}", e)))
        } else {
            Ok(0)
        }
    }

    async fn create_embedding_job(&self, conversation_id: i64, created_at: &str) -> Result<i64, BotError> {
        let changed = self
            .conn
            .execute(
                "INSERT OR IGNORE INTO embedding_jobs (conversation_id, status, created_at)
                 VALUES (?1, 'pending', ?2)",
                (conversation_id, created_at),
            )
            .await
            .map_err(|e| {
                tracing::error!("Failed to create embedding job: {}", e);
                BotError::Database(format!("Failed to create embedding job: {}", e))
            })?;

        if changed == 0 {
            return Ok(0);
        }

        let mut rows = self
            .conn
            .query(
                "SELECT id FROM embedding_jobs WHERE conversation_id = ?1",
                [conversation_id],
            )
            .await
            .map_err(|e| BotError::Database(format!("Failed to get created embedding job: {}", e)))?;

        if let Ok(Some(row)) = rows.next().await {
            row.get::<i64>(0)
                .map_err(|e| BotError::Database(format!("Failed to parse job id: {}", e)))
        } else {
            Ok(0)
        }
    }

    async fn get_pending_jobs(&self, limit: i64, max_attempts: u32) -> Result<Vec<(i64, i64, String, u32)>, BotError> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, conversation_id, created_at, attempts
                 FROM embedding_jobs
                 WHERE status = 'pending' AND attempts < ?2
                 ORDER BY created_at ASC
                 LIMIT ?1",
                (limit, max_attempts as i64),
            )
            .await
            .map_err(|e| BotError::Database(format!("Failed to get pending jobs: {}", e)))?;

        let mut jobs = Vec::new();
        while let Ok(Some(row)) = rows.next().await {
            let id: i64 = row
                .get(0)
                .map_err(|e| BotError::Database(format!("Failed to parse id: {}", e)))?;
            let conversation_id: i64 = row
                .get(1)
                .map_err(|e| BotError::Database(format!("Failed to parse conversation_id: {}", e)))?;
            let created_at: String = row
                .get(2)
                .map_err(|e| BotError::Database(format!("Failed to parse created_at: {}", e)))?;
            let attempts: u32 = row
                .get::<i64>(3)
                .map_err(|e| BotError::Database(format!("Failed to parse attempts: {}", e)))?
                as u32;
            jobs.push((id, conversation_id, created_at, attempts));
        }

        Ok(jobs)
    }

    async fn complete_embedding_job(&self, job_id: i64) -> Result<(), BotError> {
        let completed_at = Utc::now().to_rfc3339();
        self.conn
            .execute(
                "UPDATE embedding_jobs
                 SET status = 'complete',
                     error = NULL,
                     completed_at = ?1
                 WHERE id = ?2",
                (completed_at.as_str(), job_id),
            )
            .await
            .map_err(|e| BotError::Database(format!("Failed to complete embedding job: {}", e)))?;
        Ok(())
    }

    async fn fail_embedding_job(&self, job_id: i64, max_attempts: u32, error: &str) -> Result<bool, BotError> {
        let mut rows = self
            .conn
            .query("SELECT attempts FROM embedding_jobs WHERE id = ?1", [job_id])
            .await
            .map_err(|e| BotError::Database(format!("Failed to fetch embedding attempts: {}", e)))?;

        let current_attempts = if let Ok(Some(row)) = rows.next().await {
            row.get::<i64>(0)
                .map_err(|e| BotError::Database(format!("Failed to parse attempts: {}", e)))? as u32
        } else {
            return Err(BotError::Database(format!(
                "Embedding job {} not found for failure update",
                job_id
            )));
        };

        let next_attempt = current_attempts.saturating_add(1);
        let exhausted = next_attempt >= max_attempts;
        let status = if exhausted { "failed" } else { "pending" };
        let completed_at = if exhausted { Some(Utc::now().to_rfc3339()) } else { None };

        self.conn
            .execute(
                "UPDATE embedding_jobs
                 SET status = ?1,
                     attempts = ?2,
                     error = ?3,
                     completed_at = ?4
                 WHERE id = ?5",
                (
                    status,
                    next_attempt as i64,
                    Some(error),
                    completed_at.as_deref(),
                    job_id,
                ),
            )
            .await
            .map_err(|e| BotError::Database(format!("Failed to mark embedding job failure: {}", e)))?;

        Ok(exhausted)
    }

    async fn update_embedding_job(&self, job_id: i64, status: &str, error: Option<&str>) -> Result<(), BotError> {
        let completed_at =
            if status == "complete" || status == "failed" { Some(Utc::now().to_rfc3339()) } else { None };

        self.conn
            .execute(
                "UPDATE embedding_jobs
                 SET status = ?1,
                     error = ?2,
                     completed_at = ?3
                 WHERE id = ?4",
                (status, error, completed_at.as_deref(), job_id),
            )
            .await
            .map_err(|e| {
                tracing::error!("Failed to update embedding job: {}", e);
                BotError::Database(format!("Failed to update embedding job: {}", e))
            })?;

        Ok(())
    }
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
