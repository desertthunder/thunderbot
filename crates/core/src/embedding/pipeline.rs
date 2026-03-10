//! Embedding Pipeline - Background worker for generating embeddings
//!
//! This module provides the embedding pipeline that:
//! 1. Creates embedding jobs when conversations are inserted
//! 2. Processes pending jobs asynchronously via a background worker
//! 3. Handles deduplication using content hash and cosine distance
//! 4. Retries failed jobs up to 3 times

use crate::db::repository::{ConversationRepository, LibsqlRepository, MemoryRepository};
use crate::embedding::EmbeddingProvider;
use crate::error::BotError;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{Duration, interval};

/// Message types for the embedding pipeline
#[derive(Debug)]
pub enum EmbeddingPipelineMessage {
    /// Create an embedding job for a conversation
    CreateJob {
        conversation_id: i64,
        content: String,
        root_uri: String,
        author_did: String,
    },
    /// Trigger processing of pending jobs
    ProcessPending,
    /// Shutdown the pipeline
    Shutdown,
}

/// Configuration for the embedding pipeline
#[derive(Debug, Clone)]
pub struct EmbeddingPipelineConfig {
    /// How often to poll for pending jobs (in seconds)
    pub poll_interval_secs: u64,
    /// Maximum number of jobs to process per batch
    pub batch_size: usize,
    /// Deduplication threshold for cosine distance
    pub dedup_threshold: f64,
    /// Maximum number of retry attempts
    pub max_retries: u32,
}

impl Default for EmbeddingPipelineConfig {
    fn default() -> Self {
        Self { poll_interval_secs: 30, batch_size: 32, dedup_threshold: 0.05, max_retries: 3 }
    }
}

/// Embedding pipeline worker that processes embedding jobs
pub struct EmbeddingPipeline {
    repo: Arc<LibsqlRepository>,
    provider: Arc<dyn EmbeddingProvider>,
    config: EmbeddingPipelineConfig,
    tx: mpsc::Sender<EmbeddingPipelineMessage>,
}

impl EmbeddingPipeline {
    /// Create a new embedding pipeline
    pub fn new(
        repo: Arc<LibsqlRepository>, provider: Arc<dyn EmbeddingProvider>, config: EmbeddingPipelineConfig,
    ) -> (Self, mpsc::Receiver<EmbeddingPipelineMessage>) {
        let (tx, rx) = mpsc::channel(100);

        let pipeline = Self { repo, provider, config, tx };

        (pipeline, rx)
    }

    /// Get the sender handle for the pipeline
    pub fn sender(&self) -> mpsc::Sender<EmbeddingPipelineMessage> {
        self.tx.clone()
    }

    /// Start the background worker
    pub async fn run(self, mut rx: mpsc::Receiver<EmbeddingPipelineMessage>) {
        tracing::info!("Starting embedding pipeline worker");

        let mut poll_interval = interval(Duration::from_secs(self.config.poll_interval_secs));

        loop {
            tokio::select! {
                _ = poll_interval.tick() => {
                    if let Err(e) = self.process_pending_jobs().await {
                        tracing::error!("Failed to process pending jobs: {}", e);
                    }
                }
                Some(msg) = rx.recv() => {
                    match msg {
                        EmbeddingPipelineMessage::CreateJob { conversation_id, content, root_uri, author_did } => {
                            if let Err(e) = self.create_embedding_job(conversation_id, &content, &root_uri, &author_did).await {
                                tracing::error!("Failed to create embedding job: {}", e);
                            }
                        }
                        EmbeddingPipelineMessage::ProcessPending => {
                            if let Err(e) = self.process_pending_jobs().await {
                                tracing::error!("Failed to process pending jobs: {}", e);
                            }
                        }
                        EmbeddingPipelineMessage::Shutdown => {
                            tracing::info!("Shutting down embedding pipeline");
                            break;
                        }
                    }
                }
            }
        }
    }

    /// Create an embedding job for a conversation
    ///
    /// This checks if a job already exists for this conversation and creates one if not.
    /// It also checks for near-duplicate content using cosine distance.
    async fn create_embedding_job(
        &self, conversation_id: i64, content: &str, root_uri: &str, _: &str,
    ) -> Result<(), BotError> {
        if let Some(existing) = self.check_for_duplicate(root_uri, content).await? {
            tracing::debug!(
                conversation_id = conversation_id,
                existing_id = existing.id,
                "Skipping duplicate content"
            );
            return Ok(());
        }

        let created_at = chrono::Utc::now().to_rfc3339();

        match self.repo.create_embedding_job(conversation_id, &created_at).await {
            Ok(job_id) => {
                if job_id > 0 {
                    tracing::debug!(
                        conversation_id = conversation_id,
                        job_id = job_id,
                        "Created embedding job"
                    );
                } else {
                    tracing::debug!(conversation_id = conversation_id, "Embedding job already exists");
                }
                Ok(())
            }
            Err(e) => {
                tracing::error!(
                    conversation_id = conversation_id,
                    error = %e,
                    "Failed to create embedding job"
                );
                Err(e)
            }
        }
    }

    /// Check for near-duplicate content in the same thread
    ///
    /// Returns the existing memory if a duplicate is found (cosine distance < threshold)
    async fn check_for_duplicate(
        &self, root_uri: &str, content: &str,
    ) -> Result<Option<crate::db::models::Memory>, BotError> {
        let memories = self.repo.get_memories_by_root(root_uri).await?;

        if memories.is_empty() {
            return Ok(None);
        }

        let new_embedding = match self.provider.embed(content).await {
            Ok(emb) => emb,
            Err(e) => {
                tracing::warn!("Failed to generate embedding for deduplication: {}", e);
                return Ok(None);
            }
        };

        for memory in memories {
            if let Some(ref existing_embedding) = memory.embedding {
                let distance = cosine_distance(&new_embedding, existing_embedding);
                if distance < self.config.dedup_threshold {
                    tracing::debug!(
                        distance = distance,
                        threshold = self.config.dedup_threshold,
                        "Found duplicate memory"
                    );
                    return Ok(Some(memory));
                }
            }
        }

        Ok(None)
    }

    /// Process pending embedding jobs
    async fn process_pending_jobs(&self) -> Result<(), BotError> {
        let pending_jobs = self.repo.get_pending_jobs(self.config.batch_size as i64).await?;

        if pending_jobs.is_empty() {
            return Ok(());
        }

        tracing::info!(count = pending_jobs.len(), "Processing pending embedding jobs");

        for (job_id, conversation_id, _created_at) in pending_jobs {
            match self.process_job(job_id, conversation_id).await {
                Ok(()) => {
                    tracing::debug!(job_id = job_id, "Successfully processed embedding job");
                }
                Err(e) => {
                    tracing::error!(job_id = job_id, error = %e, "Failed to process embedding job");

                    let error_msg = e.to_string();
                    if let Err(update_err) = self.repo.update_embedding_job(job_id, "failed", Some(&error_msg)).await {
                        tracing::error!(job_id = job_id, error = %update_err, "Failed to update job status");
                    }
                }
            }
        }

        Ok(())
    }

    /// Process a single embedding job
    async fn process_job(&self, job_id: i64, conversation_id: i64) -> Result<(), BotError> {
        let conversation_opt: Option<crate::db::models::Conversation> = self.repo.get_by_id(conversation_id).await?;
        let conversation = match conversation_opt {
            Some(c) => c,
            None => {
                return Err(BotError::Database(format!(
                    "Conversation {} not found",
                    conversation_id
                )));
            }
        };

        let embedding = self.provider.embed(&conversation.content).await?;

        self.repo
            .create_memory(
                conversation_id,
                &conversation.root_uri,
                &conversation.content,
                &embedding,
                &conversation.author_did,
                &conversation.created_at,
            )
            .await?;

        self.repo.update_embedding_job(job_id, "complete", None).await?;

        Ok(())
    }

    /// Backfill embeddings for all conversations that don't have them
    pub async fn backfill(&self, batch_size: Option<usize>) -> Result<usize, BotError> {
        let limit = batch_size.unwrap_or(self.config.batch_size) as i64;

        let mut rows = self
            .repo
            .conn()
            .query(
                "SELECT c.id, c.content, c.root_uri, c.author_did, c.created_at
             FROM conversations c
             LEFT JOIN embedding_jobs ej ON c.id = ej.conversation_id
             WHERE ej.id IS NULL
             LIMIT ?1",
                [limit],
            )
            .await
            .map_err(|e| BotError::Database(format!("Failed to get conversations for backfill: {}", e)))?;

        let mut count = 0;
        while let Ok(Some(row)) = rows.next().await {
            let id: i64 = row
                .get(0)
                .map_err(|e| BotError::Database(format!("Failed to parse id: {}", e)))?;
            let content: String = row
                .get(1)
                .map_err(|e| BotError::Database(format!("Failed to parse content: {}", e)))?;
            let root_uri: String = row
                .get(2)
                .map_err(|e| BotError::Database(format!("Failed to parse root_uri: {}", e)))?;
            let author_did: String = row
                .get(3)
                .map_err(|e| BotError::Database(format!("Failed to parse author_did: {}", e)))?;

            if let Err(e) = self.create_embedding_job(id, &content, &root_uri, &author_did).await {
                tracing::warn!(conversation_id = id, error = %e, "Failed to create embedding job during backfill");
            } else {
                count += 1;
            }
        }

        tracing::info!(count = count, "Created embedding jobs for backfill");
        Ok(count)
    }
}

/// Calculate cosine distance between two vectors
///
/// Returns a value between 0.0 (identical) and 2.0 (opposite)
fn cosine_distance(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 2.0;
    }

    let mut dot_product = 0.0f64;
    let mut norm_a = 0.0f64;
    let mut norm_b = 0.0f64;

    for (x, y) in a.iter().zip(b.iter()) {
        let x_f64 = *x as f64;
        let y_f64 = *y as f64;
        dot_product += x_f64 * y_f64;
        norm_a += x_f64 * x_f64;
        norm_b += y_f64 * y_f64;
    }

    let norm_product = norm_a.sqrt() * norm_b.sqrt();
    if norm_product == 0.0 {
        return 2.0;
    }

    let cosine = dot_product / norm_product;
    (1.0 - cosine).abs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_distance_identical() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let dist = cosine_distance(&a, &b);
        assert!(
            dist < 0.0001,
            "Expected near-zero distance for identical vectors, got {}",
            dist
        );
    }

    #[test]
    fn test_cosine_distance_opposite() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![-1.0, 0.0, 0.0];
        let dist = cosine_distance(&a, &b);
        assert!(
            dist > 1.999,
            "Expected near-2.0 distance for opposite vectors, got {}",
            dist
        );
    }

    #[test]
    fn test_cosine_distance_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let dist = cosine_distance(&a, &b);
        assert!(
            (dist - 1.0).abs() < 0.0001,
            "Expected 1.0 distance for orthogonal vectors, got {}",
            dist
        );
    }

    #[test]
    fn test_cosine_distance_different_lengths() {
        let a = vec![1.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let dist = cosine_distance(&a, &b);
        assert_eq!(dist, 2.0, "Expected maximum distance for different length vectors");
    }
}
