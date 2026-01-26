use super::embedding::GeminiEmbeddingProvider;
use super::sqlite_store::SqliteVecStore;
use super::types::*;

use anyhow::Result;
use chrono::{Duration, Utc};
use std::sync::Arc;

pub struct SemanticRetriever {
    vector_store: Arc<SqliteVecStore>,
    embedding_provider: Arc<GeminiEmbeddingProvider>,
    config: MemoryConfig,
}

impl SemanticRetriever {
    pub fn new(
        vector_store: Arc<SqliteVecStore>, embedding_provider: Arc<GeminiEmbeddingProvider>, config: MemoryConfig,
    ) -> Self {
        Self { vector_store, embedding_provider, config }
    }

    pub async fn add_conversation_memory(&self, request: EmbeddingRequest) -> Result<()> {
        let content_hash = SqliteVecStore::content_hash(&request.text);

        if self
            .vector_store
            .content_hash_exists(&request.conversation_id, &content_hash)
            .await
            .unwrap_or(false)
        {
            tracing::debug!("Skipping duplicate memory for conversation {}", request.conversation_id);
            return Ok(());
        }

        let memory = Memory {
            id: uuid::Uuid::new_v4().to_string(),
            conversation_id: request.conversation_id,
            content: request.text.clone(),
            content_hash,
            metadata: MemoryMetadata {
                author_did: request.author_did,
                role: request.role,
                parent_uri: request.parent_uri,
                topics: None,
            },
            created_at: Utc::now(),
        };

        let embedding = self.embedding_provider.embed(&request.text).await?;

        self.vector_store.add_memory(memory, embedding).await?;

        Ok(())
    }

    pub async fn add_conversation_memory_batch(&self, requests: Vec<EmbeddingRequest>) -> Result<()> {
        if requests.is_empty() {
            return Ok(());
        }

        tracing::debug!("Adding {} memories in batch", requests.len());

        let hashes: Vec<String> = requests.iter().map(|r| SqliteVecStore::content_hash(&r.text)).collect();
        let conversation_id = requests.first().map(|r| r.conversation_id.as_str()).unwrap_or_default();
        let has_mixed_conversations = requests.iter().any(|r| r.conversation_id.as_str() != conversation_id);
        let existing = if has_mixed_conversations {
            Default::default()
        } else {
            self.vector_store
                .existing_hashes(conversation_id, &hashes)
                .await
                .unwrap_or_default()
        };

        let mut pending_requests = Vec::new();
        let mut pending_hashes = Vec::new();

        for (request, hash) in requests.into_iter().zip(hashes.into_iter()) {
            if existing.contains(&hash) {
                continue;
            }
            pending_hashes.push(hash);
            pending_requests.push(request);
        }

        if pending_requests.is_empty() {
            return Ok(());
        }

        let texts: Vec<String> = pending_requests.iter().map(|r| r.text.clone()).collect();
        let embeddings = self.embedding_provider.embed_batch(&texts).await?;

        for ((request, embedding), content_hash) in pending_requests
            .iter()
            .zip(embeddings.iter())
            .zip(pending_hashes.iter())
        {
            let memory = Memory {
                id: uuid::Uuid::new_v4().to_string(),
                conversation_id: request.conversation_id.clone(),
                content: request.text.clone(),
                content_hash: content_hash.clone(),
                metadata: MemoryMetadata {
                    author_did: request.author_did.clone(),
                    role: request.role.clone(),
                    parent_uri: request.parent_uri.clone(),
                    topics: None,
                },
                created_at: Utc::now(),
            };

            self.vector_store.add_memory(memory, embedding.clone()).await?;
        }

        Ok(())
    }

    pub async fn search_memories(
        &self, query: &str, top_k: Option<usize>, filter: Option<SearchFilter>,
    ) -> Result<Vec<MemoryWithScore>> {
        let k = top_k.unwrap_or(self.config.top_k_default);

        tracing::debug!("Searching for memories with query ({} chars), top_k={}", query.len(), k);

        if query.trim().is_empty() {
            return Ok(Vec::new());
        }

        let query_embedding = self.embedding_provider.embed(query).await?;

        let filter_for_vector = filter.clone();
        let mut results = match self
            .vector_store
            .search_hybrid(query, &query_embedding, k, filter.clone())
            .await
        {
            Ok(results) => results,
            Err(err) => {
                tracing::debug!("Hybrid search failed, falling back to vector search: {}", err);
                VectorStore::search(&*self.vector_store, &query_embedding, k, filter_for_vector).await?
            }
        };

        let threshold = filter
            .as_ref()
            .and_then(|f| f.min_score)
            .unwrap_or(self.config.min_similarity);

        results.retain(|m| m.score >= threshold);

        tracing::debug!("Found {} memories above similarity threshold", results.len());

        Ok(results)
    }

    pub async fn get_relevant_context(&self, query: &str, limit: Option<usize>) -> Result<String> {
        let memories = self.search_memories(query, limit, None).await?;

        if memories.is_empty() {
            return Ok(String::new());
        }

        let context_lines: Vec<String> = memories
            .into_iter()
            .map(|m| {
                format!(
                    "[{} from {}]: {}",
                    m.memory.metadata.role, m.memory.metadata.author_did, m.memory.content
                )
            })
            .collect();

        Ok(context_lines.join("\n"))
    }

    pub async fn get_stats(&self) -> Result<VectorStats> {
        VectorStore::get_stats(&*self.vector_store).await
    }

    pub async fn backfill_conversation(
        &self, conversation_id: &str, messages: &[(String, String, String)],
    ) -> Result<usize> {
        tracing::debug!(
            "Backfilling conversation {} with {} messages",
            conversation_id,
            messages.len()
        );

        let mut requests = Vec::new();

        for (content, author_did, role) in messages {
            let request = EmbeddingRequest {
                text: content.clone(),
                conversation_id: conversation_id.to_string(),
                author_did: author_did.clone(),
                role: role.clone(),
                parent_uri: None,
            };

            requests.push(request);
        }

        self.add_conversation_memory_batch(requests).await?;

        let added = messages.len();

        tracing::info!("Backfilled {} memories for conversation {}", added, conversation_id);

        Ok(added)
    }

    pub async fn cleanup_old_memories(&self) -> Result<usize> {
        let ttl_days = self
            .config
            .ttl_days
            .ok_or_else(|| anyhow::anyhow!("TTL not configured"))?;

        let older_than = Utc::now() - Duration::days(ttl_days as i64);
        tracing::warn!("Deleting memories older than {}", older_than);

        let deleted = VectorStore::delete_old_memories(&*self.vector_store, older_than).await?;
        tracing::info!("Deleted {} old memories", deleted);
        Ok(deleted)
    }

    pub async fn consolidate_old_conversations(&self, _max_age_days: u64) -> Result<usize> {
        let stats = self.get_stats().await?;
        let old_threads = stats.unique_conversations;
        tracing::info!("Found {} conversations to potentially consolidate", old_threads);
        Ok(0)
    }
}
