use serde::{Deserialize, Serialize};

/// Request body for Ollama embedding endpoint
#[derive(Debug, Serialize)]
pub struct OllamaEmbedRequest {
    pub model: String,
    pub input: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<OllamaEmbedOptions>,
}

/// Options for Ollama embedding request
#[derive(Debug, Serialize)]
pub struct OllamaEmbedOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
}

/// Response from Ollama embedding endpoint
#[derive(Debug, Deserialize)]
pub struct OllamaEmbedResponse {
    pub embeddings: Vec<Vec<f32>>,
}

/// Single text embedding request (for non-batch API)
#[derive(Debug, Serialize)]
pub struct OllamaSingleEmbedRequest {
    pub model: String,
    pub prompt: String,
}

/// Single text embedding response (for non-batch API)
#[derive(Debug, Deserialize)]
pub struct OllamaSingleEmbedResponse {
    pub embedding: Vec<f32>,
}
