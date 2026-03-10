pub mod ollama;
pub mod pipeline;
pub mod types;

pub use ollama::OllamaEmbeddingProvider;
pub use pipeline::{EmbeddingPipeline, EmbeddingPipelineConfig, EmbeddingPipelineMessage};

use crate::error::BotError;
use async_trait::async_trait;

/// Trait for embedding generation providers
///
/// Implement this trait to add support for different embedding services
/// (Ollama, OpenAI, HuggingFace, etc.)
#[async_trait]
pub trait EmbeddingProvider: Send + Sync + std::fmt::Debug {
    /// Embed a single text. Returns vector of f32.
    async fn embed(&self, text: &str) -> Result<Vec<f32>, BotError>;

    /// Embed a batch of texts efficiently.
    ///
    /// Default implementation calls `embed` for each text sequentially,
    /// but providers should override this for batch API support.
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, BotError>;

    /// Return the dimensionality (no. of dimensions in the embedding vector) of the embedding output.
    fn dimensions(&self) -> usize;
}

/// Configuration for embedding providers
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EmbeddingConfig {
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_dimensions")]
    pub dimensions: usize,
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
}

fn default_provider() -> String {
    "ollama".to_string()
}

fn default_base_url() -> String {
    "http://localhost:11434".to_string()
}

fn default_model() -> String {
    "embeddinggemma".to_string()
}

fn default_dimensions() -> usize {
    768
}

fn default_batch_size() -> usize {
    32
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            provider: default_provider(),
            base_url: default_base_url(),
            model: default_model(),
            dimensions: default_dimensions(),
            batch_size: default_batch_size(),
        }
    }
}

impl EmbeddingConfig {
    /// Create an embedding provider based on this configuration
    ///
    /// # Returns
    /// A boxed embedding provider
    pub fn create_provider(&self) -> Box<dyn EmbeddingProvider> {
        match self.provider.as_str() {
            "ollama" => Box::new(OllamaEmbeddingProvider::new(
                &self.base_url,
                &self.model,
                self.dimensions,
                self.batch_size,
            )),
            _ => {
                tracing::warn!("Unknown embedding provider '{}', defaulting to Ollama", self.provider);
                Box::new(OllamaEmbeddingProvider::default_embeddinggemma(&self.base_url))
            }
        }
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.provider.is_empty() {
            return Err("embedding.provider must not be empty".to_string());
        }

        if self.base_url.is_empty() {
            return Err("embedding.base_url must not be empty".to_string());
        }

        if !self.base_url.starts_with("http://") && !self.base_url.starts_with("https://") {
            return Err("embedding.base_url must start with http:// or https://".to_string());
        }

        if self.model.is_empty() {
            return Err("embedding.model must not be empty".to_string());
        }

        if self.dimensions == 0 {
            return Err("embedding.dimensions must be greater than 0".to_string());
        }

        if self.batch_size == 0 {
            return Err("embedding.batch_size must be greater than 0".to_string());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_config_default() {
        let config = EmbeddingConfig::default();
        assert_eq!(config.provider, "ollama");
        assert_eq!(config.base_url, "http://localhost:11434");
        assert_eq!(config.model, "embeddinggemma");
        assert_eq!(config.dimensions, 768);
        assert_eq!(config.batch_size, 32);
    }

    #[test]
    fn test_embedding_config_validation() {
        let mut config = EmbeddingConfig::default();
        assert!(config.validate().is_ok());

        config.provider = "".to_string();
        assert!(config.validate().is_err());

        config.provider = "ollama".to_string();
        config.base_url = "".to_string();
        assert!(config.validate().is_err());

        config.base_url = "http://localhost:11434".to_string();
        config.model = "".to_string();
        assert!(config.validate().is_err());

        config.model = "embeddinggemma".to_string();
        config.dimensions = 0;
        assert!(config.validate().is_err());

        config.dimensions = 768;
        config.batch_size = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_embedding_config_validation_invalid_url() {
        let config = EmbeddingConfig { base_url: "ftp://localhost:11434".to_string(), ..Default::default() };
        assert!(config.validate().is_err());
    }
}
