use super::types::*;

use anyhow::{Context, Result, anyhow};
use reqwest::Client;
use std::time::Duration;

const GEMINI_API_BASE: &str = "https://generativelanguage.googleapis.com/v1beta/models/";
const MAX_RETRIES: u32 = 3;

#[derive(Clone)]
pub struct GeminiClient {
    client: Client,
    api_key: String,
    model: String,
}

impl GeminiClient {
    pub fn new(api_key: String, model: Option<String>) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(60))
                .build()
                .expect("Failed to create HTTP client"),
            api_key,
            model: model.unwrap_or_else(|| "gemini-3-pro-preview".to_string()),
        }
    }

    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("GEMINI_API_KEY").context("GEMINI_API_KEY environment variable not set")?;
        let model = std::env::var("GEMINI_MODEL").ok();
        Ok(Self::new(api_key, model))
    }

    pub async fn generate_content(&self, request: GenerateContentRequest) -> Result<String> {
        let url = format!("{}{}:generateContent?key={}", GEMINI_API_BASE, self.model, self.api_key);

        tracing::debug!("Calling Gemini API with model: {}", self.model);

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
                        let gemini_response: GenerateContentResponse = resp.json().await?;

                        if let Some(candidate) = gemini_response.candidates.first() {
                            let text = self
                                .extract_text_from_parts(&candidate.content.parts)
                                .context("Failed to extract text from response")?;

                            if let Some(usage) = &gemini_response.usage_metadata {
                                tracing::info!(
                                    "Tokens: {} prompt, {} candidates, {} total",
                                    usage.prompt_token_count,
                                    usage.candidates_token_count,
                                    usage.total_token_count
                                );
                            }

                            return Ok(text);
                        } else {
                            return Err(anyhow!("No candidates in response"));
                        }
                    } else if status.is_client_error() {
                        let error_text = resp.text().await.unwrap_or_else(|_| "Unknown error".to_string());

                        if let Ok(error_response) = serde_json::from_str::<ErrorResponse>(&error_text) {
                            return Err(anyhow!(
                                "Gemini API error ({}): {} - {}",
                                status,
                                error_response.error.status,
                                error_response.error.message
                            ));
                        } else {
                            return Err(anyhow!("Gemini API error ({}): {}", status, error_text));
                        }
                    } else {
                        let error_text = resp.text().await.unwrap_or_else(|_| "Unknown error".to_string());
                        last_error = Some(anyhow!("Server error ({}): {}", status, error_text));
                    }
                }
                Err(e) => {
                    last_error = Some(anyhow!("Request failed: {}", e));
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow!("Max retries exceeded")))
    }

    fn extract_text_from_parts(&self, parts: &[Part]) -> Result<String> {
        let text: Vec<String> = parts
            .iter()
            .filter_map(|part| match part {
                Part::Text { text } => Some(text.clone()),
                Part::Thought { .. } => None,
            })
            .collect();

        if text.is_empty() { Err(anyhow!("No text content in response")) } else { Ok(text.join("\n")) }
    }

    pub async fn prompt(&self, text: &str, system_instruction: Option<String>) -> Result<String> {
        let content = Content { parts: vec![Part::Text { text: text.to_string() }], role: Some("user".to_string()) };

        let system_content = system_instruction.map(|instruction| Content {
            parts: vec![Part::Text { text: instruction }],
            role: Some("system".to_string()),
        });

        let mut contents = Vec::new();
        if let Some(sys) = system_content {
            contents.push(sys);
        }
        contents.push(content);

        let request = GenerateContentRequest {
            contents,
            generation_config: Some(GenerationConfig {
                temperature: Some(0.7),
                top_p: Some(0.9),
                top_k: Some(40),
                max_output_tokens: Some(1024),
                thinking_config: Some(ThinkingConfig { include_thoughts: false }),
            }),
            system_instruction: None,
        };

        self.generate_content(request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = GeminiClient::new("test-key".to_string(), None);
        assert_eq!(client.api_key, "test-key");
        assert_eq!(client.model, "gemini-3.0-pro");
    }

    #[test]
    fn test_extract_text_from_parts() {
        let client = GeminiClient::new("test-key".to_string(), None);
        let parts = vec![
            Part::Text { text: "Hello".to_string() },
            Part::Text { text: "World".to_string() },
        ];
        let text = client.extract_text_from_parts(&parts).unwrap();
        assert_eq!(text, "Hello\nWorld");
    }

    #[test]
    fn test_extract_text_from_empty_parts() {
        let client = GeminiClient::new("test-key".to_string(), None);
        let parts = vec![];
        let result = client.extract_text_from_parts(&parts);
        assert!(result.is_err());
    }
}
