//! GLM-5 API Client
//!
//! Async HTTP client for Z.ai's GLM-5 model using OpenAI-compatible API.
//! Supports chat completions, streaming, function calling, and thinking mode.

use crate::ai::types::*;
use crate::error::{BotError, Result};
use rand::RngExt;
use reqwest::StatusCode;
use std::time::Duration;

const DEFAULT_BASE_URL: &str = "https://api.z.ai/api/paas/v4";
const DEFAULT_MODEL: &str = "glm-5";
const MAX_RETRIES: usize = 3;
const INITIAL_BACKOFF_MS: u64 = 1000;
const MAX_BACKOFF_MS: u64 = 60000;

/// GLM-5 API Client
///
/// Provides methods for chat completions with retry logic and error handling.
#[derive(Clone)]
pub struct Glm5Client {
    http: reqwest::Client,
    api_key: String,
    base_url: String,
    default_model: String,
    default_temperature: f64,
    default_max_tokens: u32,
}

/// Configuration for the GLM-5 client
#[derive(Debug, Clone)]
pub struct Glm5Config {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub temperature: f64,
    pub max_tokens: u32,
}

impl Default for Glm5Config {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: DEFAULT_BASE_URL.to_string(),
            model: DEFAULT_MODEL.to_string(),
            temperature: 0.7,
            max_tokens: 300,
        }
    }
}

impl Glm5Client {
    /// Create a new GLM-5 client with an API key
    pub fn new(api_key: impl Into<String>) -> Self {
        Self::with_config(Glm5Config { api_key: api_key.into(), ..Default::default() })
    }

    /// Create a client from environment variables
    ///
    /// Checks `GLM_5_API_KEY` or `TNBOT_AI__API_KEY` environment variables.
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("GLM_5_API_KEY")
            .or_else(|_| std::env::var("TNBOT_AI__API_KEY"))
            .map_err(|_| {
                BotError::AiConfig("GLM_5_API_KEY or TNBOT_AI__API_KEY environment variable not set".to_string())
            })?;

        let base_url = std::env::var("GLM_5_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());
        let model = std::env::var("GLM_5_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string());
        let temperature = std::env::var("GLM_5_TEMPERATURE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.7);
        let max_tokens = std::env::var("GLM_5_MAX_TOKENS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(300);

        Ok(Self::with_config(Glm5Config {
            api_key,
            base_url,
            model,
            temperature,
            max_tokens,
        }))
    }

    /// Create a client with full configuration
    pub fn with_config(config: Glm5Config) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .connect_timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to build HTTP client");

        Self {
            http,
            api_key: config.api_key,
            base_url: config.base_url,
            default_model: config.model,
            default_temperature: config.temperature,
            default_max_tokens: config.max_tokens,
        }
    }

    /// Get the configured model name
    pub fn model(&self) -> &str {
        &self.default_model
    }

    /// Send a chat completion request
    ///
    /// Automatically retries on 5xx errors and rate limits with exponential backoff.
    pub async fn chat_completion(&self, mut request: ChatCompletionRequest) -> Result<ChatCompletionResponse> {
        if request.model.is_empty() {
            request.model = self.default_model.clone();
        }
        if request.temperature.is_none() {
            request.temperature = Some(self.default_temperature);
        }
        if request.max_tokens.is_none() {
            request.max_tokens = Some(self.default_max_tokens);
        }

        let url = format!("{}/chat/completions", self.base_url);

        for attempt in 0..MAX_RETRIES {
            let response = self
                .http
                .post(&url)
                .header("Authorization", format!("Bearer {}", self.api_key))
                .header("Content-Type", "application/json")
                .json(&request)
                .send()
                .await
                .map_err(|e| BotError::AiHttp(format!("Request failed: {}", e)))?;

            let status = response.status();

            if status.is_success() {
                return response
                    .json::<ChatCompletionResponse>()
                    .await
                    .map_err(|e| BotError::AiSerialization(format!("Failed to parse response: {}", e)));
            }

            if (status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()) && attempt + 1 < MAX_RETRIES {
                let backoff = calculate_backoff(attempt);
                tracing::warn!(
                    "GLM-5 API returned {} (attempt {}/{}), retrying after {}ms",
                    status,
                    attempt + 1,
                    MAX_RETRIES,
                    backoff
                );
                tokio::time::sleep(Duration::from_millis(backoff)).await;
                continue;
            }

            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());

            return Err(match status.as_u16() {
                401 => BotError::AiAuthentication(format!("Invalid API key: {}", error_text)),
                429 => BotError::AiRateLimit(format!("Rate limit exceeded: {}", error_text)),
                400 => BotError::AiInvalidRequest(format!("Invalid request: {}", error_text)),
                500..=599 => BotError::AiServerError(format!("Server error: {}", error_text)),
                _ => BotError::AiHttp(format!("HTTP {}: {}", status, error_text)),
            });
        }

        Err(BotError::AiHttp("Retry loop exhausted".to_string()))
    }

    /// Send a simple chat message and get the response text
    pub async fn chat(&self, messages: Vec<Message>) -> Result<String> {
        let request = ChatCompletionRequest::new(&self.default_model, messages);
        let response = self.chat_completion(request).await?;

        response
            .content()
            .map(|s| s.to_string())
            .ok_or_else(|| BotError::AiResponse("Empty response from model".to_string()))
    }

    /// Send a simple prompt and get the response
    pub async fn prompt(&self, system: impl Into<String>, user: impl Into<String>) -> Result<String> {
        let messages = vec![Message::system(system), Message::user(user)];
        self.chat(messages).await
    }

    /// Send a one-shot user message and get the response
    pub async fn ask(&self, message: impl Into<String>) -> Result<String> {
        let messages = vec![Message::user(message)];
        self.chat(messages).await
    }

    /// Check if the API is accessible
    pub async fn health_check(&self) -> Result<bool> {
        let request = ChatCompletionRequest::new(&self.default_model, vec![Message::user("Hi")]).with_max_tokens(1);

        match self.chat_completion(request).await {
            Ok(_) => Ok(true),
            Err(BotError::AiAuthentication(_)) => Err(BotError::AiAuthentication(
                "API key is invalid. Please check your GLM_5_API_KEY.".to_string(),
            )),
            Err(e) => {
                tracing::warn!("Health check failed: {}", e);
                Ok(false)
            }
        }
    }
}

/// Calculate exponential backoff with jitter
fn calculate_backoff(attempt: usize) -> u64 {
    let factor = 2_u64.saturating_pow(attempt as u32);
    let base = INITIAL_BACKOFF_MS.saturating_mul(factor);
    let capped = base.min(MAX_BACKOFF_MS);
    let jitter = rand::rng().random_range(0..=1000_u64);
    capped.saturating_add(jitter)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glm5_client_new() {
        let client = Glm5Client::new("test-key");
        assert_eq!(client.api_key, "test-key");
        assert_eq!(client.base_url, DEFAULT_BASE_URL);
        assert_eq!(client.default_model, DEFAULT_MODEL);
    }

    #[test]
    fn test_glm5_client_with_config() {
        let config = Glm5Config {
            api_key: "custom-key".to_string(),
            base_url: "https://custom.api.com".to_string(),
            model: "custom-model".to_string(),
            temperature: 0.5,
            max_tokens: 100,
        };

        let client = Glm5Client::with_config(config);
        assert_eq!(client.api_key, "custom-key");
        assert_eq!(client.base_url, "https://custom.api.com");
        assert_eq!(client.default_model, "custom-model");
        assert_eq!(client.default_temperature, 0.5);
        assert_eq!(client.default_max_tokens, 100);
    }

    #[test]
    fn test_backoff_calculation() {
        let b0 = calculate_backoff(0);
        assert!((1000..=2000).contains(&b0));

        let b1 = calculate_backoff(1);
        assert!((2000..=3000).contains(&b1));

        let b5 = calculate_backoff(5);
        assert!(b5 <= MAX_BACKOFF_MS + 1000);
    }

    #[test]
    fn test_default_config() {
        let config = Glm5Config::default();
        assert!(config.api_key.is_empty());
        assert_eq!(config.base_url, DEFAULT_BASE_URL);
        assert_eq!(config.model, DEFAULT_MODEL);
        assert_eq!(config.temperature, 0.7);
        assert_eq!(config.max_tokens, 300);
    }
}
