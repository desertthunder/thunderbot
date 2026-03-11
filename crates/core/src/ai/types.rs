//! GLM-5 AI Client types
//!
//! OpenAI-compatible API types for Z.ai's GLM-5 model.
//! See: https://api.z.ai/api/coding/paas/v4/chat/completions

use serde::{Deserialize, Serialize};

/// Request body for chat completions
#[derive(Debug, Clone, Serialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<ResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<ThinkingConfig>,
}

/// A message in the conversation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Message {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// Tool/function definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: FunctionDef,
}

/// Function definition for tool calling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// A tool call from the assistant
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: FunctionCall,
}

/// Function call details
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

/// Response format configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseFormat {
    #[serde(rename = "type")]
    pub format_type: String,
}

/// Thinking mode configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingConfig {
    #[serde(rename = "type")]
    pub thinking_type: String,
}

/// Chat completion response
#[derive(Debug, Clone, Deserialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    #[serde(rename = "object")]
    pub object_type: String,
    pub model: String,
    pub choices: Vec<Choice>,
    pub usage: Usage,
}

/// A choice from the model
#[derive(Debug, Clone, Deserialize)]
pub struct Choice {
    pub index: u32,
    pub message: Message,
    pub finish_reason: Option<String>,
}

/// Token usage information
#[derive(Debug, Clone, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Streaming chunk for SSE responses
#[derive(Debug, Clone, Deserialize)]
pub struct ChatCompletionChunk {
    pub id: String,
    #[serde(rename = "object")]
    pub object_type: String,
    pub model: String,
    pub choices: Vec<ChunkChoice>,
}

/// A choice in a streaming chunk
#[derive(Debug, Clone, Deserialize)]
pub struct ChunkChoice {
    pub index: u32,
    pub delta: DeltaMessage,
    pub finish_reason: Option<String>,
}

/// Delta message for streaming
#[derive(Debug, Clone, Deserialize, Default)]
pub struct DeltaMessage {
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<ToolCall>>,
}

impl Message {
    /// Create a system message
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: "system".to_string(), content: Some(content.into()), tool_calls: None, tool_call_id: None }
    }

    /// Create a user message
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: "user".to_string(), content: Some(content.into()), tool_calls: None, tool_call_id: None }
    }

    /// Create an assistant message
    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: "assistant".to_string(), content: Some(content.into()), tool_calls: None, tool_call_id: None }
    }

    /// Create a tool result message
    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".to_string(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
        }
    }

    /// Check if this message contains the silent thought marker
    pub fn is_silent(&self) -> bool {
        self.content
            .as_ref()
            .map(|c| c.contains("<SILENT_THOUGHT>"))
            .unwrap_or(false)
    }
}

impl ChatCompletionRequest {
    /// Create a new chat completion request with the specified model and messages
    pub fn new(model: impl Into<String>, messages: Vec<Message>) -> Self {
        Self {
            model: model.into(),
            messages,
            temperature: None,
            max_tokens: None,
            stream: None,
            tools: None,
            tool_choice: None,
            tool_stream: None,
            response_format: None,
            thinking: None,
        }
    }

    /// Set the temperature parameter
    pub fn with_temperature(mut self, temperature: f64) -> Self {
        self.temperature = Some(temperature);
        self
    }

    /// Set the max tokens parameter
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// Enable streaming
    pub fn with_streaming(mut self) -> Self {
        self.stream = Some(true);
        self
    }

    /// Set tools for function calling
    pub fn with_tools(mut self, tools: Vec<Tool>) -> Self {
        self.tools = Some(tools);
        self
    }

    /// Enable thinking mode
    pub fn with_thinking(mut self) -> Self {
        self.thinking = Some(ThinkingConfig { thinking_type: "enabled".to_string() });
        self
    }

    /// Set response format to JSON
    pub fn with_json_response(mut self) -> Self {
        self.response_format = Some(ResponseFormat { format_type: "json_object".to_string() });
        self
    }
}

impl Tool {
    /// Create a new function tool
    pub fn function(name: impl Into<String>, description: impl Into<String>, parameters: serde_json::Value) -> Self {
        Self {
            tool_type: "function".to_string(),
            function: FunctionDef { name: name.into(), description: description.into(), parameters },
        }
    }
}

impl ChatCompletionResponse {
    /// Get the content of the first choice, if any
    pub fn content(&self) -> Option<&str> {
        self.choices.first().and_then(|c| c.message.content.as_deref())
    }

    /// Get the finish reason of the first choice
    pub fn finish_reason(&self) -> Option<&str> {
        self.choices.first().and_then(|c| c.finish_reason.as_deref())
    }

    /// Check if the response contains tool calls
    pub fn has_tool_calls(&self) -> bool {
        self.choices
            .first()
            .and_then(|c| c.message.tool_calls.as_ref())
            .map(|t| !t.is_empty())
            .unwrap_or(false)
    }

    /// Get tool calls from the first choice
    pub fn tool_calls(&self) -> Option<&[ToolCall]> {
        self.choices.first().and_then(|c| c.message.tool_calls.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_message_constructors() {
        let system = Message::system("You are a helpful assistant");
        assert_eq!(system.role, "system");
        assert_eq!(system.content, Some("You are a helpful assistant".to_string()));

        let user = Message::user("Hello!");
        assert_eq!(user.role, "user");
        assert_eq!(user.content, Some("Hello!".to_string()));

        let assistant = Message::assistant("Hi there!");
        assert_eq!(assistant.role, "assistant");
        assert_eq!(assistant.content, Some("Hi there!".to_string()));
    }

    #[test]
    fn test_message_is_silent() {
        let silent = Message::assistant("I think... <SILENT_THOUGHT>");
        assert!(silent.is_silent());

        let normal = Message::assistant("Hello!");
        assert!(!normal.is_silent());
    }

    #[test]
    fn test_chat_completion_request_builder() {
        let request = ChatCompletionRequest::new("glm-5", vec![Message::user("Hello")])
            .with_temperature(0.7)
            .with_max_tokens(300)
            .with_thinking();

        assert_eq!(request.model, "glm-5");
        assert_eq!(request.temperature, Some(0.7));
        assert_eq!(request.max_tokens, Some(300));
        assert!(request.thinking.is_some());
    }

    #[test]
    fn test_tool_creation() {
        let tool = Tool::function(
            "get_weather",
            "Get the current weather",
            json!({
                "type": "object",
                "properties": {
                    "location": {"type": "string"}
                },
                "required": ["location"]
            }),
        );

        assert_eq!(tool.tool_type, "function");
        assert_eq!(tool.function.name, "get_weather");
    }

    #[test]
    fn test_response_parsing() {
        let json = json!({
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "model": "glm-5",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Hello!"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            }
        });

        let response: ChatCompletionResponse = serde_json::from_value(json).unwrap();
        assert_eq!(response.id, "chatcmpl-123");
        assert_eq!(response.model, "glm-5");
        assert_eq!(response.content(), Some("Hello!"));
        assert_eq!(response.finish_reason(), Some("stop"));
        assert!(!response.has_tool_calls());
    }

    #[test]
    fn test_tool_call_response_parsing() {
        let json = json!({
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "model": "glm-5",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_abc",
                        "type": "function",
                        "function": {
                            "name": "get_weather",
                            "arguments": "{\"location\":\"NYC\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {
                "prompt_tokens": 20,
                "completion_tokens": 10,
                "total_tokens": 30
            }
        });

        let response: ChatCompletionResponse = serde_json::from_value(json).unwrap();
        assert!(response.has_tool_calls());
        let tool_calls = response.tool_calls().unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].function.name, "get_weather");
    }
}
