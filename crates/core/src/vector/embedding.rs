use super::types::*;
use anyhow::{Context, Result, anyhow};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const GEMINI_EMBEDDING_API_BASE: &str = "https://generativelanguage.googleapis.com/v1beta/models/";
const MAX_RETRIES: u32 = 3;

#[derive(Debug, Serialize)]
struct EmbeddingRequest {
    model: String,
    content: EmbeddingContent,
}

#[derive(Debug, Serialize)]
struct EmbeddingContent {
    parts: Vec<EmbeddingPart>,
}

#[derive(Debug, Serialize)]
struct EmbeddingPart {
    text: String,
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    embedding: EmbeddingValue,
}

#[derive(Debug, Deserialize)]
struct EmbeddingValue {
    values: Vec<f32>,
}

#[derive(Debug, Deserialize)]
struct GeminiError {
    error: GeminiErrorDetail,
}

#[derive(Debug, Deserialize)]
struct GeminiErrorDetail {
    code: i32,
    message: String,
    status: String,
}

pub struct GeminiEmbeddingProvider {
    client: Client,
    api_key: String,
    model: String,
}

impl GeminiEmbeddingProvider {
    pub fn new(api_key: String) -> Result<Self> {
        Ok(Self {
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .context("Failed to create HTTP client")?,
            api_key,
            model: "text-embedding-004".to_string(),
        })
    }

    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("GEMINI_API_KEY").context("GEMINI_API_KEY environment variable not set")?;
        Self::new(api_key)
    }

    async fn make_request(&self, text: &str) -> Result<Vec<f32>> {
        let url = format!(
            "{}{}:embedContent?key={}",
            GEMINI_EMBEDDING_API_BASE, self.model, self.api_key
        );

        let request = EmbeddingRequest {
            model: format!("models/{}", self.model),
            content: EmbeddingContent { parts: vec![EmbeddingPart { text: text.to_string() }] },
        };

        tracing::debug!("Requesting embedding for text ({} chars)", text.len());

        let mut last_error = None;

        for attempt in 1..=MAX_RETRIES {
            if attempt > 1 {
                let backoff = Duration::from_millis(1000 * 2u64.pow(attempt - 1));
                tracing::warn!("Retry attempt {} after {:?}", attempt, backoff);
                tokio::time::sleep(backoff).await;
            }

            let response = self
                .client
                .post(&url)
                .header("Content-Type", "application/json")
                .json(&request)
                .send()
                .await;

            match response {
                Ok(resp) => {
                    let status = resp.status();

                    if status.is_success() {
                        let gemini_response: EmbeddingResponse =
                            resp.json().await.context("Failed to parse embedding response")?;

                        tracing::debug!(
                            "Received embedding with {} dimensions",
                            gemini_response.embedding.values.len()
                        );

                        return Ok(gemini_response.embedding.values);
                    } else if status.is_client_error() {
                        let error_text = resp.text().await.unwrap_or_else(|_| "Unknown error".to_string());

                        if let Ok(error_response) = serde_json::from_str::<GeminiError>(&error_text) {
                            return Err(anyhow!(
                                "Gemini API error ({}): {} - {} (code: {})",
                                status,
                                error_response.error.status,
                                error_response.error.message,
                                error_response.error.code
                            ));
                        } else {
                            return Err(anyhow!("Gemini API error ({}): {}", status, error_text));
                        }
                    } else {
                        let error_text = resp.text().await.unwrap_or_else(|_| "Unknown error".to_string());
                        last_error = Some(anyhow!("Server error ({}): {}", status, error_text));
                    }
                }
                Err(e) => last_error = Some(anyhow!("Request failed: {}", e)),
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow!("Max retries exceeded")))
    }
}

#[async_trait::async_trait]
impl EmbeddingProvider for GeminiEmbeddingProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let clean_text = text.trim().to_string();

        if clean_text.is_empty() {
            return Err(anyhow!("Cannot embed empty text"));
        }

        self.make_request(&clean_text).await
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let mut embeddings = Vec::with_capacity(texts.len());

        for (i, text) in texts.iter().enumerate() {
            tracing::debug!("Embedding {}/{}", i + 1, texts.len());

            let embedding = self.embed(text).await?;

            embeddings.push(embedding);
        }

        Ok(embeddings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_creation() {
        let provider = GeminiEmbeddingProvider::new("test-key".to_string()).unwrap();
        assert_eq!(provider.api_key, "test-key");
        assert_eq!(provider.model, "text-embedding-004");
    }

    #[test]
    fn test_batch_empty() {
        let provider = GeminiEmbeddingProvider::new("test-key".to_string()).unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let embeddings = rt.block_on(provider.embed_batch(&[])).unwrap();

        assert!(embeddings.is_empty());
    }
}
