use super::types::{OllamaEmbedRequest, OllamaEmbedResponse};
use crate::embedding::EmbeddingProvider;
use crate::error::BotError;
use async_trait::async_trait;
use std::time::Duration;
use tokio::time::sleep;

/// Ollama embedding provider implementation
///
/// Supports embedding models available via Ollama:
/// - `embeddinggemma` (default): 308M params, 768 dims, multilingual
/// - `nomic-embed-text`: 137M params, 768 dims, 8K context
/// - `mxbai-embed-large`: 335M params, 1024 dims
/// - `snowflake-arctic-embed2`: <1B params, 1024 dims
/// - `all-MiniLM-L6-v2`: 22M params, 384 dims
#[derive(Debug, Clone)]
pub struct OllamaEmbeddingProvider {
    client: reqwest::Client,
    base_url: String,
    model: String,
    dimensions: usize,
    batch_size: usize,
    max_retries: u32,
    base_delay_ms: u64,
}

impl OllamaEmbeddingProvider {
    /// Create a new Ollama embedding provider
    ///
    /// # Arguments
    /// * `base_url` - Ollama API base URL (e.g., "http://localhost:11434")
    /// * `model` - Model name to use for embeddings
    /// * `dimensions` - Expected embedding dimensions
    /// * `batch_size` - Maximum batch size for embedding requests
    pub fn new(base_url: impl Into<String>, model: impl Into<String>, dimensions: usize, batch_size: usize) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("Failed to build reqwest client");

        Self {
            client,
            base_url: base_url.into(),
            model: model.into(),
            dimensions,
            batch_size,
            max_retries: 3,
            base_delay_ms: 1000,
        }
    }

    /// Create with default settings for embeddinggemma
    pub fn default_embeddinggemma(base_url: impl Into<String>) -> Self {
        Self::new(base_url, "embeddinggemma", 768, 32)
    }

    /// Create with default settings for nomic-embed-text
    pub fn default_nomic(base_url: impl Into<String>) -> Self {
        Self::new(base_url, "nomic-embed-text", 768, 32)
    }

    /// Set the maximum number of retries
    pub fn with_max_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }

    /// Set the base delay for exponential backoff (in milliseconds)
    pub fn with_base_delay(mut self, delay_ms: u64) -> Self {
        self.base_delay_ms = delay_ms;
        self
    }

    /// Execute embedding request with retry logic
    async fn embed_with_retry(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, BotError> {
        let url = format!("{}/api/embed", self.base_url);
        let inputs: Vec<String> = texts.iter().map(|s| s.to_string()).collect();
        let request = OllamaEmbedRequest { model: self.model.clone(), input: inputs, options: None };

        let mut last_error: Option<BotError> = None;

        for attempt in 0..self.max_retries {
            match self.client.post(&url).json(&request).send().await {
                Ok(response) => {
                    let status = response.status();

                    if status.is_success() {
                        match response.json::<OllamaEmbedResponse>().await {
                            Ok(embed_response) => {
                                for (i, embedding) in embed_response.embeddings.iter().enumerate() {
                                    if embedding.len() != self.dimensions {
                                        return Err(BotError::Embedding(format!(
                                            "Embedding {} has {} dimensions, expected {}",
                                            i,
                                            embedding.len(),
                                            self.dimensions
                                        )));
                                    }
                                }
                                return Ok(embed_response.embeddings);
                            }
                            Err(e) => {
                                last_error = Some(BotError::Embedding(format!(
                                    "Failed to parse embedding response: {}",
                                    e
                                )));
                            }
                        }
                    } else {
                        let error_text = response.text().await.unwrap_or_else(|_| format!("HTTP {}", status));
                        last_error = Some(BotError::Embedding(format!(
                            "Ollama API error (attempt {}): {} - {}",
                            attempt + 1,
                            status,
                            error_text
                        )));
                    }
                }
                Err(e) => {
                    if e.is_timeout() {
                        last_error = Some(BotError::Embedding(format!(
                            "Ollama connection timeout (attempt {})",
                            attempt + 1
                        )));
                    } else if e.is_connect() {
                        last_error = Some(BotError::Embedding(format!(
                            "Ollama connection failed (attempt {}): {}",
                            attempt + 1,
                            e
                        )));
                    } else {
                        last_error = Some(BotError::Embedding(format!(
                            "Ollama request failed (attempt {}): {}",
                            attempt + 1,
                            e
                        )));
                    }
                }
            }

            if attempt < self.max_retries - 1 {
                let delay = self.base_delay_ms * 2_u64.pow(attempt);
                let jitter = rand::random::<u64>() % (delay / 2);
                sleep(Duration::from_millis(delay + jitter)).await;
            }
        }

        Err(last_error.unwrap_or_else(|| BotError::Embedding("Max retries exceeded for embedding request".to_string())))
    }
}

#[async_trait]
impl EmbeddingProvider for OllamaEmbeddingProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, BotError> {
        let embeddings = self.embed_batch(&[text]).await?;
        embeddings
            .into_iter()
            .next()
            .ok_or_else(|| BotError::Embedding("No embedding returned".to_string()))
    }

    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, BotError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let mut all_embeddings = Vec::with_capacity(texts.len());

        for chunk in texts.chunks(self.batch_size) {
            let chunk_embeddings = self.embed_with_retry(chunk).await?;
            all_embeddings.extend(chunk_embeddings);
        }

        Ok(all_embeddings)
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ollama_provider_creation() {
        let provider = OllamaEmbeddingProvider::default_embeddinggemma("http://localhost:11434");
        assert_eq!(provider.dimensions(), 768);
        assert_eq!(provider.model, "embeddinggemma");
    }

    #[test]
    fn test_ollama_provider_nomic() {
        let provider = OllamaEmbeddingProvider::default_nomic("http://localhost:11434");
        assert_eq!(provider.dimensions(), 768);
        assert_eq!(provider.model, "nomic-embed-text");
    }

    #[test]
    fn test_ollama_provider_custom() {
        let provider = OllamaEmbeddingProvider::new("http://ollama:11434", "mxbai-embed-large", 1024, 64);
        assert_eq!(provider.dimensions(), 1024);
        assert_eq!(provider.model, "mxbai-embed-large");
        assert_eq!(provider.batch_size, 64);
    }
}
