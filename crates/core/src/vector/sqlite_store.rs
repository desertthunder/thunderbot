use crate::vector::types::*;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::sync::Once;
use tokio_rusqlite::{Connection, params, rusqlite};

static SQLITE_VEC_INIT: Once = Once::new();

pub struct SqliteVecStore {
    conn: Connection,
    config: MemoryConfig,
}

impl SqliteVecStore {
    pub async fn new(db_url: &str, config: MemoryConfig) -> Result<Self> {
        SQLITE_VEC_INIT.call_once(|| unsafe {
            #[allow(clippy::missing_transmute_annotations)]
            rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(sqlite_vec::sqlite3_vec_init as *const ())));
        });

        let path = db_url.strip_prefix("file:").unwrap_or(db_url).to_string();
        let conn = Connection::open(path).await?;

        let store = Self { conn, config };
        store.ensure_tables().await?;
        Ok(store)
    }

    async fn ensure_tables(&self) -> Result<()> {
        let dim = self.config.embedding_dim;

        self.conn
            .call(move |conn| {
                conn.execute(
                    "
                CREATE TABLE IF NOT EXISTS memories (
                    id TEXT PRIMARY KEY,
                    conversation_id TEXT,
                    content TEXT,
                    content_hash TEXT,
                    author_did TEXT,
                    role TEXT,
                    parent_uri TEXT,
                    created_at TEXT
                )
                ",
                    [],
                )?;

                conn.execute(
                    "
                CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
                    id UNINDEXED,
                    content
                )
                ",
                    [],
                )?;

                conn.execute(
                    &format!(
                        "
                CREATE VIRTUAL TABLE IF NOT EXISTS memories_vec USING vec0(
                    id TEXT PRIMARY KEY,
                    embedding float[{}]
                )
                ",
                        dim
                    ),
                    [],
                )?;

                Ok::<(), rusqlite::Error>(())
            })
            .await
            .map_err(|e| anyhow!("Failed to ensure tables: {}", e))
    }

    pub fn content_hash(content: &str) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        hex::encode(hasher.finalize())
    }

    pub async fn content_hash_exists(&self, conversation_id: &str, content_hash: &str) -> Result<bool> {
        let conv_id = conversation_id.to_string();
        let hash = content_hash.to_string();

        self.conn
            .call(move |conn| {
                let count: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM memories WHERE conversation_id = ?1 AND content_hash = ?2",
                    params![conv_id, hash],
                    |r| r.get(0),
                )?;
                Ok::<bool, rusqlite::Error>(count > 0)
            })
            .await
            .map_err(|e| anyhow!("Failed to check content hash: {}", e))
    }

    pub async fn existing_hashes(
        &self, conversation_id: &str, hashes: &[String],
    ) -> Result<std::collections::HashSet<String>> {
        if hashes.is_empty() {
            return Ok(std::collections::HashSet::new());
        }

        let conv_id = conversation_id.to_string();
        let hashes_vec = hashes.to_vec();

        self.conn
            .call(move |conn| {
                let quoted_hashes = hashes_vec
                    .iter()
                    .map(|h| format!("'{}'", h.replace('\'', "''")))
                    .collect::<Vec<_>>()
                    .join(", ");

                let sql = format!(
                    "SELECT content_hash FROM memories WHERE conversation_id = ?1 AND content_hash IN ({})",
                    quoted_hashes
                );

                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(params![conv_id], |row| row.get(0))?;
                let mut existing = std::collections::HashSet::new();
                for hash in rows {
                    existing.insert(hash?);
                }
                Ok::<std::collections::HashSet<String>, rusqlite::Error>(existing)
            })
            .await
            .map_err(|e| anyhow!("Failed to get existing hashes: {}", e))
    }

    fn f32_to_bytes(v: &[f32]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(v.len() * 4);
        for &f in v {
            bytes.extend_from_slice(&f.to_le_bytes());
        }
        bytes
    }
}

#[async_trait]
impl VectorStore for SqliteVecStore {
    async fn add_memory(&self, memory: Memory, embedding: Vec<f32>) -> Result<()> {
        if embedding.len() != self.config.embedding_dim {
            return Err(anyhow!(
                "Embedding dimension mismatch: expected {}, got {}",
                self.config.embedding_dim,
                embedding.len()
            ));
        }

        let embedding_bytes = Self::f32_to_bytes(&embedding);

        self.conn.call(move |conn| {
            let tx = conn.transaction()?;

            tx.execute(
                "INSERT OR IGNORE INTO memories (id, conversation_id, content, content_hash, author_did, role, parent_uri, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    memory.id,
                    memory.conversation_id,
                    memory.content,
                    memory.content_hash,
                    memory.metadata.author_did,
                    memory.metadata.role,
                    memory.metadata.parent_uri,
                    memory.created_at.to_rfc3339()
                ],
            )?;

            tx.execute(
                "INSERT INTO memories_fts (id, content) VALUES (?1, ?2)",
                params![memory.id, memory.content],
            )?;

            tx.execute(
                "INSERT INTO memories_vec (id, embedding) VALUES (?1, ?2)",
                params![memory.id, embedding_bytes],
            )?;

            tx.commit()?;
            Ok::<(), rusqlite::Error>(())
        })
        .await
        .map_err(|e| anyhow!("Failed to add memory: {}", e))
    }

    async fn search(
        &self, query_embedding: &[f32], top_k: usize, filter: Option<SearchFilter>,
    ) -> Result<Vec<MemoryWithScore>> {
        let embedding_bytes = Self::f32_to_bytes(query_embedding);

        self.conn.call(move |conn| {
            let mut sql = "
                SELECT m.id, m.conversation_id, m.content, m.content_hash, m.author_did, m.role, m.parent_uri, m.created_at, v.distance
                FROM memories_vec v
                JOIN memories m ON v.id = m.id
                WHERE v.embedding MATCH ?1 AND v.k = ?2
            ".to_string();

            if let Some(f) = &filter {
                if let Some(author) = &f.author_did {
                    sql.push_str(&format!(" AND m.author_did = '{}'", author.replace('\'', "''")));
                }
                if let Some(role) = &f.role {
                    sql.push_str(&format!(" AND m.role = '{}'", role.replace('\'', "''")));
                }
                if let Some(start) = &f.start_time {
                    sql.push_str(&format!(" AND m.created_at >= '{}'", start.to_rfc3339()));
                }
                if let Some(end) = &f.end_time {
                    sql.push_str(&format!(" AND m.created_at <= '{}'", end.to_rfc3339()));
                }
            }

            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(params![embedding_bytes, top_k as i64], |row| {
                let created_at_str: String = row.get(7)?;
                let created_at = DateTime::parse_from_rfc3339(&created_at_str).map_err(|_| rusqlite::Error::InvalidQuery)?.with_timezone(&Utc);
                let distance: f64 = row.get(8)?;

                Ok(MemoryWithScore {
                    memory: Memory {
                        id: row.get(0)?,
                        conversation_id: row.get(1)?,
                        content: row.get(2)?,
                        content_hash: row.get(3)?,
                        metadata: MemoryMetadata {
                            author_did: row.get(4)?,
                            role: row.get(5)?,
                            parent_uri: row.get(6)?,
                            topics: None,
                        },
                        created_at,
                    },
                    score: 1.0 / (1.0 + distance as f32),
                })
            })?;

            let mut results = Vec::new();
            for r in rows {
                results.push(r?);
            }
            Ok::<Vec<MemoryWithScore>, rusqlite::Error>(results)
        })
        .await
        .map_err(|e| anyhow!("Search failed: {}", e))
    }

    async fn search_hybrid(
        &self, query_text: &str, query_embedding: &[f32], top_k: usize, filter: Option<SearchFilter>,
    ) -> Result<Vec<MemoryWithScore>> {
        let embedding_bytes = Self::f32_to_bytes(query_embedding);
        let q_text = query_text.to_string();
        let rrf_k = 60.0;

        self.conn
            .call(move |conn| {
                let mut where_clause = String::new();
                if let Some(f) = &filter {
                    if let Some(author) = &f.author_did {
                        where_clause.push_str(&format!(" AND m.author_did = '{}'", author.replace('\'', "''")));
                    }
                    if let Some(role) = &f.role {
                        where_clause.push_str(&format!(" AND m.role = '{}'", role.replace('\'', "''")));
                    }
                    if let Some(start) = &f.start_time {
                        where_clause.push_str(&format!(" AND m.created_at >= '{}'", start.to_rfc3339()));
                    }
                    if let Some(end) = &f.end_time {
                        where_clause.push_str(&format!(" AND m.created_at <= '{}'", end.to_rfc3339()));
                    }
                }

                let sql = format!(
                    "
                WITH vec_matches AS (
                    SELECT id, row_number() OVER (ORDER BY distance) as rank_number
                    FROM memories_vec
                    WHERE embedding MATCH ?1 AND k = ?2
                ),
                fts_matches AS (
                    SELECT id, row_number() OVER (ORDER BY rank) as rank_number
                    FROM memories_fts
                    WHERE content MATCH ?3
                )
                SELECT
                    m.id, m.conversation_id, m.content, m.content_hash,
                    m.author_did, m.role, m.parent_uri, m.created_at,
                    (
                        COALESCE(1.0 / ({} + v.rank_number), 0.0) +
                        COALESCE(1.0 / ({} + f.rank_number), 0.0)
                    ) as rrf_score
                FROM vec_matches v
                FULL OUTER JOIN fts_matches f ON v.id = f.id
                JOIN memories m ON m.id = COALESCE(v.id, f.id)
                WHERE 1=1 {}
                ORDER BY rrf_score DESC
                LIMIT ?2
            ",
                    rrf_k, rrf_k, where_clause
                );

                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(params![embedding_bytes, top_k as i64, q_text], |row| {
                    let created_at_str: String = row.get(7)?;
                    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
                        .map_err(|_| rusqlite::Error::InvalidQuery)?
                        .with_timezone(&Utc);

                    Ok(MemoryWithScore {
                        memory: Memory {
                            id: row.get(0)?,
                            conversation_id: row.get(1)?,
                            content: row.get(2)?,
                            content_hash: row.get(3)?,
                            metadata: MemoryMetadata {
                                author_did: row.get(4)?,
                                role: row.get(5)?,
                                parent_uri: row.get(6)?,
                                topics: None,
                            },
                            created_at,
                        },
                        score: row.get::<_, f64>(8)? as f32,
                    })
                })?;

                let mut results = Vec::new();
                for r in rows {
                    results.push(r?);
                }
                Ok::<Vec<MemoryWithScore>, rusqlite::Error>(results)
            })
            .await
            .map_err(|e| anyhow!("Hybrid search failed: {}", e))
    }

    async fn get_stats(&self) -> Result<VectorStats> {
        self.conn
            .call(|conn| {
                let total_memories: usize = conn.query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0))?;
                let unique_conversations: usize =
                    conn.query_row("SELECT COUNT(DISTINCT conversation_id) FROM memories", [], |r| r.get(0))?;

                let oldest_memory: Option<String> =
                    conn.query_row("SELECT MIN(created_at) FROM memories", [], |r| r.get(0))?;
                let newest_memory: Option<String> =
                    conn.query_row("SELECT MAX(created_at) FROM memories", [], |r| r.get(0))?;

                let oldest_memory = oldest_memory
                    .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                    .map(|dt| dt.with_timezone(&Utc));
                let newest_memory = newest_memory
                    .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                    .map(|dt| dt.with_timezone(&Utc));

                let mut stmt = conn.prepare("SELECT role, COUNT(*) FROM memories GROUP BY role")?;
                let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, usize>(1)?)))?;
                let mut by_role = std::collections::HashMap::new();
                for r in rows {
                    let (role, count) = r?;
                    by_role.insert(role, count);
                }

                Ok::<VectorStats, rusqlite::Error>(VectorStats {
                    total_memories,
                    unique_conversations,
                    oldest_memory,
                    newest_memory,
                    by_role,
                })
            })
            .await
            .map_err(|e| anyhow!("Failed to get stats: {}", e))
    }

    async fn delete_by_conversation(&self, conversation_id: &str) -> Result<usize> {
        let conv_id = conversation_id.to_string();
        self.conn
            .call(move |conn| {
                let tx = conn.transaction()?;

                let count: usize = tx.query_row(
                    "SELECT COUNT(*) FROM memories WHERE conversation_id = ?1",
                    params![conv_id],
                    |r| r.get(0),
                )?;

                tx.execute(
                    "DELETE FROM memories_vec WHERE id IN (SELECT id FROM memories WHERE conversation_id = ?1)",
                    params![conv_id],
                )?;
                tx.execute(
                    "DELETE FROM memories_fts WHERE id IN (SELECT id FROM memories WHERE conversation_id = ?1)",
                    params![conv_id],
                )?;
                tx.execute("DELETE FROM memories WHERE conversation_id = ?1", params![conv_id])?;

                tx.commit()?;
                Ok::<usize, rusqlite::Error>(count)
            })
            .await
            .map_err(|e| anyhow!("Failed to delete by conversation: {}", e))
    }

    async fn delete_old_memories(&self, older_than: DateTime<Utc>) -> Result<usize> {
        let older_than_str = older_than.to_rfc3339();
        self.conn
            .call(move |conn| {
                let tx = conn.transaction()?;

                let count: usize = tx.query_row(
                    "SELECT COUNT(*) FROM memories WHERE created_at < ?1",
                    params![older_than_str],
                    |r| r.get(0),
                )?;

                tx.execute(
                    "DELETE FROM memories_vec WHERE id IN (SELECT id FROM memories WHERE created_at < ?1)",
                    params![older_than_str],
                )?;
                tx.execute(
                    "DELETE FROM memories_fts WHERE id IN (SELECT id FROM memories WHERE created_at < ?1)",
                    params![older_than_str],
                )?;
                tx.execute("DELETE FROM memories WHERE created_at < ?1", params![older_than_str])?;

                tx.commit()?;
                Ok::<usize, rusqlite::Error>(count)
            })
            .await
            .map_err(|e| anyhow!("Failed to delete old memories: {}", e))
    }

    async fn consolidate_conversation(&self, conversation_id: &str) -> Result<Option<Memory>> {
        let conv_id = conversation_id.to_string();

        let contents = self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT content, role, author_did FROM memories WHERE conversation_id = ?1 ORDER BY created_at ASC",
                )?;
                let rows = stmt.query_map(params![conv_id], |row| {
                    Ok(format!(
                        "[{}:{}] {}",
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(0)?
                    ))
                })?;

                let mut contents = Vec::new();
                for c in rows {
                    contents.push(c?);
                }
                Ok::<Vec<String>, rusqlite::Error>(contents)
            })
            .await
            .map_err(|e| anyhow!("Failed to fetch messages for consolidation: {}", e))?;

        if contents.len() < 4 {
            return Ok(None);
        }

        let summary_text = contents.into_iter().take(12).collect::<Vec<_>>().join(" | ");

        let memory = Memory {
            id: uuid::Uuid::new_v4().to_string(),
            conversation_id: conversation_id.to_string(),
            content: format!("Summary: {}", summary_text),
            content_hash: Self::content_hash(&summary_text),
            metadata: MemoryMetadata {
                author_did: "system".to_string(),
                role: "summary".to_string(),
                parent_uri: None,
                topics: None,
            },
            created_at: Utc::now(),
        };

        Ok(Some(memory))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tempfile::tempdir;

    async fn setup_store() -> (SqliteVecStore, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db_url = format!("file:{}", db_path.to_str().unwrap());
        let config = MemoryConfig { embedding_dim: 3, top_k_default: 5, min_similarity: 0.1, ttl_days: None };
        let store = SqliteVecStore::new(&db_url, config).await.unwrap();
        (store, dir)
    }

    #[tokio::test]
    async fn test_add_and_search() {
        let (store, _dir) = setup_store().await;

        let memory = Memory {
            id: "1".to_string(),
            conversation_id: "conv1".to_string(),
            content: "hello world".to_string(),
            content_hash: "hash1".to_string(),
            metadata: MemoryMetadata {
                author_did: "author1".to_string(),
                role: "user".to_string(),
                parent_uri: None,
                topics: None,
            },
            created_at: Utc::now(),
        };

        let embedding = vec![1.0, 0.0, 0.0];
        store.add_memory(memory, embedding).await.unwrap();

        let query = vec![1.0, 0.1, 0.0];
        let results = store.search(&query, 5, None).await.unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].memory.id, "1");
        assert!(results[0].score > 0.0);
    }

    #[tokio::test]
    async fn test_search_hybrid() {
        let (store, _dir) = setup_store().await;

        let memory1 = Memory {
            id: "1".to_string(),
            conversation_id: "conv1".to_string(),
            content: "apple pie".to_string(),
            content_hash: "hash1".to_string(),
            metadata: MemoryMetadata {
                author_did: "author1".to_string(),
                role: "user".to_string(),
                parent_uri: None,
                topics: None,
            },
            created_at: Utc::now(),
        };
        let embedding1 = vec![1.0, 0.0, 0.0];
        store.add_memory(memory1, embedding1).await.unwrap();

        let memory2 = Memory {
            id: "2".to_string(),
            conversation_id: "conv1".to_string(),
            content: "banana split".to_string(),
            content_hash: "hash2".to_string(),
            metadata: MemoryMetadata {
                author_did: "author1".to_string(),
                role: "user".to_string(),
                parent_uri: None,
                topics: None,
            },
            created_at: Utc::now(),
        };
        let embedding2 = vec![0.0, 1.0, 0.0];
        store.add_memory(memory2, embedding2).await.unwrap();

        let query_embedding = vec![0.1, 0.9, 0.0];
        let results = store.search_hybrid("apple", &query_embedding, 5, None).await.unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].memory.id, "1");
    }

    #[tokio::test]
    async fn test_stats() {
        let (store, _dir) = setup_store().await;

        let memory = Memory {
            id: "1".to_string(),
            conversation_id: "conv1".to_string(),
            content: "test".to_string(),
            content_hash: "hash1".to_string(),
            metadata: MemoryMetadata {
                author_did: "author1".to_string(),
                role: "user".to_string(),
                parent_uri: None,
                topics: None,
            },
            created_at: Utc::now(),
        };
        store.add_memory(memory, vec![0.0, 0.0, 0.0]).await.unwrap();

        let stats = store.get_stats().await.unwrap();
        assert_eq!(stats.total_memories, 1);
        assert_eq!(stats.unique_conversations, 1);
        assert!(stats.oldest_memory.is_some());
        assert!(stats.newest_memory.is_some());
    }

    #[tokio::test]
    async fn test_delete() {
        let (store, _dir) = setup_store().await;

        let memory = Memory {
            id: "1".to_string(),
            conversation_id: "conv1".to_string(),
            content: "test".to_string(),
            content_hash: "hash1".to_string(),
            metadata: MemoryMetadata {
                author_did: "author1".to_string(),
                role: "user".to_string(),
                parent_uri: None,
                topics: None,
            },
            created_at: Utc::now(),
        };
        store.add_memory(memory, vec![0.0, 0.0, 0.0]).await.unwrap();

        store.delete_by_conversation("conv1").await.unwrap();
        let stats = store.get_stats().await.unwrap();
        assert_eq!(stats.total_memories, 0);
    }
}
