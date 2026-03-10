//! Memory retrieval service with semantic, keyword, and hybrid (RRF) search.

use crate::db::models::{Memory, MemorySearchFilters, MemorySearchResult, SearchSource};
use crate::db::repository::MemoryRepository;
use crate::embedding::EmbeddingProvider;
use crate::error::BotError;
use std::collections::HashMap;
use std::sync::Arc;

/// Configuration for hybrid memory retrieval.
#[derive(Debug, Clone)]
pub struct MemoryRetrieverConfig {
    /// Number of results returned to callers.
    pub top_k: usize,
    /// Reciprocal Rank Fusion constant (higher = flatter rank impact).
    pub rrf_k: usize,
}

impl Default for MemoryRetrieverConfig {
    fn default() -> Self {
        Self { top_k: 5, rrf_k: 60 }
    }
}

/// Retrieves memories for RAG by combining vector and keyword ranking.
#[derive(Clone)]
pub struct MemoryRetriever<R: MemoryRepository + Clone + Send + Sync> {
    repo: R,
    provider: Arc<dyn EmbeddingProvider>,
    config: MemoryRetrieverConfig,
}

impl<R: MemoryRepository + Clone + Send + Sync> std::fmt::Debug for MemoryRetriever<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryRetriever")
            .field("config", &self.config)
            .field("provider_dimensions", &self.provider.dimensions())
            .finish()
    }
}

impl<R: MemoryRepository + Clone + Send + Sync> MemoryRetriever<R> {
    pub fn new(repo: R, provider: Arc<dyn EmbeddingProvider>, config: MemoryRetrieverConfig) -> Self {
        Self { repo, provider, config }
    }

    /// Semantic-only retrieval.
    pub async fn retrieve_semantic(
        &self, query: &str, filters: MemorySearchFilters, top_k: Option<usize>,
    ) -> Result<Vec<MemorySearchResult>, BotError> {
        let query_embedding = self.provider.embed(query).await?;
        let limit = top_k.unwrap_or(self.config.top_k);
        let rows = self
            .repo
            .search_semantic_filtered(
                &query_embedding,
                limit,
                filters.author_did.as_deref(),
                filters.time_after.as_deref(),
                filters.root_uri.as_deref(),
                filters.exclude_root_uri.as_deref(),
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(|memory| MemorySearchResult {
                score: 1.0 - memory.distance.unwrap_or(1.0),
                memory,
                source: SearchSource::Semantic,
            })
            .collect())
    }

    /// Hybrid retrieval: semantic + FTS5 keyword in parallel, merged via RRF.
    pub async fn retrieve_hybrid(
        &self, query: &str, filters: MemorySearchFilters, top_k: Option<usize>,
    ) -> Result<Vec<MemorySearchResult>, BotError> {
        let query_embedding = self.provider.embed(query).await?;
        let limit = top_k.unwrap_or(self.config.top_k);

        let semantic_fut = self.repo.search_semantic_filtered(
            &query_embedding,
            limit,
            filters.author_did.as_deref(),
            filters.time_after.as_deref(),
            filters.root_uri.as_deref(),
            filters.exclude_root_uri.as_deref(),
        );
        let keyword_fut = self.repo.search_keyword(
            query,
            limit,
            filters.author_did.as_deref(),
            filters.time_after.as_deref(),
            filters.root_uri.as_deref(),
            filters.exclude_root_uri.as_deref(),
        );

        let (semantic_rows, keyword_rows) = tokio::join!(semantic_fut, keyword_fut);
        let semantic_rows = semantic_rows?;
        let keyword_rows = keyword_rows?;

        Ok(fuse_rrf(semantic_rows, keyword_rows, self.config.rrf_k, limit))
    }
}

fn fuse_rrf(semantic: Vec<Memory>, keyword: Vec<Memory>, rrf_k: usize, top_k: usize) -> Vec<MemorySearchResult> {
    #[derive(Clone)]
    struct Entry {
        memory: Memory,
        semantic_rank: Option<usize>,
        keyword_rank: Option<usize>,
        score: f64,
    }

    let mut merged: HashMap<i64, Entry> = HashMap::new();

    for (idx, memory) in semantic.into_iter().enumerate() {
        let rank = idx + 1;
        let contribution = 1.0 / (rrf_k as f64 + rank as f64);
        let entry = merged.entry(memory.id).or_insert_with(|| Entry {
            memory,
            semantic_rank: None,
            keyword_rank: None,
            score: 0.0,
        });
        entry.semantic_rank = Some(rank);
        entry.score += contribution;
    }

    for (idx, memory) in keyword.into_iter().enumerate() {
        let rank = idx + 1;
        let contribution = 1.0 / (rrf_k as f64 + rank as f64);
        let entry = merged.entry(memory.id).or_insert_with(|| Entry {
            memory,
            semantic_rank: None,
            keyword_rank: None,
            score: 0.0,
        });
        entry.keyword_rank = Some(rank);
        entry.score += contribution;
    }

    let mut results: Vec<MemorySearchResult> = merged
        .into_values()
        .map(|entry| {
            let source = match (entry.semantic_rank, entry.keyword_rank) {
                (Some(_), Some(_)) => SearchSource::Hybrid,
                (Some(_), None) => SearchSource::Semantic,
                (None, Some(_)) => SearchSource::Keyword,
                (None, None) => SearchSource::Hybrid,
            };
            MemorySearchResult { memory: entry.memory, score: entry.score, source }
        })
        .collect();

    results.sort_by(|a, b| b.score.total_cmp(&a.score));
    results.truncate(top_k);
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory(id: i64, text: &str) -> Memory {
        Memory {
            id,
            conversation_id: id,
            root_uri: format!("at://root/{id}"),
            content: text.to_string(),
            embedding: None,
            author_did: "did:plc:test".to_string(),
            metadata: None,
            content_hash: None,
            created_at: "2026-03-10T00:00:00Z".to_string(),
            expires_at: None,
            distance: None,
        }
    }

    #[test]
    fn test_rrf_prefers_cross_source_result() {
        let semantic = vec![memory(1, "rust async"), memory(2, "jetstream cursor")];
        let keyword = vec![memory(2, "jetstream cursor"), memory(3, "embedding gemma")];

        let results = fuse_rrf(semantic, keyword, 60, 3);

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].memory.id, 2);
        assert_eq!(results[0].source, SearchSource::Hybrid);
    }

    #[test]
    fn test_rrf_respects_top_k() {
        let semantic = vec![memory(1, "a"), memory(2, "b"), memory(3, "c")];
        let keyword = vec![memory(4, "d"), memory(5, "e"), memory(6, "f")];
        let results = fuse_rrf(semantic, keyword, 60, 2);
        assert_eq!(results.len(), 2);
    }
}
