//! GLM-5 AI Client module
//!
//! Provides OpenAI-compatible API client for Z.ai's GLM-5 model.
//!
//! # Example
//!
//! ```rust,no_run
//! use tnbot_core::ai::{Glm5Client, Message};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let client = Glm5Client::from_env()?;
//!
//! let messages = vec![
//!     Message::system("You are a helpful assistant."),
//!     Message::user("Hello, how are you?"),
//! ];
//!
//! let response = client.chat(messages).await?;
//! println!("Response: {}", response);
//! # Ok(())
//! # }
//! ```

pub mod client;
pub mod types;

pub use client::{Glm5Client, Glm5Config};
pub use types::{
    ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse, Choice, ChunkChoice, DeltaMessage,
    FunctionCall, FunctionDef, Message, ResponseFormat, ThinkingConfig, Tool, ToolCall, Usage,
};
