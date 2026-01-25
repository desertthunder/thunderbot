use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: String,
    pub conversation_id: String,
    pub content: String,
    pub content_hash: String,
    pub metadata: MemoryMetadata,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryMetadata {
    pub author_did: String,
    pub role: String,
    pub parent_uri: Option<String>,
    pub topics: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct MemoryWithScore {
    pub memory: Memory,
    pub score: f32,
}

#[derive(Debug, Clone, Default)]
pub struct SearchFilter {
    pub author_did: Option<String>,
    pub role: Option<String>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub min_score: Option<f32>,
}

#[derive(Debug, Clone)]
pub struct VectorStats {
    pub total_memories: usize,
    pub unique_conversations: usize,
    pub oldest_memory: Option<DateTime<Utc>>,
    pub newest_memory: Option<DateTime<Utc>>,
    pub by_role: HashMap<String, usize>,
}

#[derive(Debug, Clone, Copy)]
pub struct MemoryConfig {
    pub embedding_dim: usize,
    pub top_k_default: usize,
    pub min_similarity: f32,
    pub ttl_days: Option<u64>,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self { embedding_dim: 768, top_k_default: 5, min_similarity: 0.6, ttl_days: Some(90) }
    }
}

#[derive(Debug, Clone)]
pub struct EmbeddingRequest {
    pub text: String,
    pub conversation_id: String,
    pub author_did: String,
    pub role: String,
    pub parent_uri: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingResponse {
    pub embedding: Vec<f32>,
}

#[async_trait::async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
}

#[async_trait::async_trait]
pub trait VectorStore: Send + Sync {
    async fn add_memory(&self, memory: Memory, embedding: Vec<f32>) -> Result<()>;
    async fn search(
        &self, query_embedding: &[f32], top_k: usize, filter: Option<SearchFilter>,
    ) -> Result<Vec<MemoryWithScore>>;
    async fn search_hybrid(
        &self, query_text: &str, query_embedding: &[f32], top_k: usize, filter: Option<SearchFilter>,
    ) -> Result<Vec<MemoryWithScore>>;
    async fn get_stats(&self) -> Result<VectorStats>;
    async fn delete_by_conversation(&self, conversation_id: &str) -> Result<usize>;
    async fn delete_old_memories(&self, older_than: DateTime<Utc>) -> Result<usize>;
    async fn consolidate_conversation(&self, conversation_id: &str) -> Result<Option<Memory>>;
}
