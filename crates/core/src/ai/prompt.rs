//! Prompt construction module
//!
//! Builds OpenAI-compatible message arrays from conversation history.
//! Handles:
//! - System instruction injection
//! - Multi-user handle prefixing: `[@handle]: message`
//! - Role mapping (user/model -> user/assistant)

use crate::ai::types::Message;
use crate::db::models::{Conversation, Role};

/// Builds prompts for the GLM-5 API from conversation history
#[derive(Debug, Clone)]
pub struct PromptBuilder {
    system_instruction: String,
}

impl PromptBuilder {
    /// Create a new prompt builder with a system instruction
    pub fn new(system_instruction: impl Into<String>) -> Self {
        Self { system_instruction: system_instruction.into() }
    }

    /// Build the message array for the GLM-5 API
    ///
    /// # Arguments
    /// * `thread` - Conversation history from the database
    /// * `resolve_handle` - Function to resolve DIDs to handles
    ///
    /// # Returns
    /// Vec of messages ready for the chat completion API
    pub fn build<F>(&self, thread: &[Conversation], resolve_handle: F) -> Vec<Message>
    where
        F: Fn(&str) -> String,
    {
        let mut messages = vec![Message::system(&self.system_instruction)];

        for row in thread {
            let content = match row.role {
                Role::User => {
                    let handle = resolve_handle(&row.author_did);
                    format!("[@{}]: {}", handle, row.content)
                }
                Role::Model => row.content.clone(),
            };

            let role = match row.role {
                Role::User => "user",
                Role::Model => "assistant",
            };

            messages.push(Message {
                role: role.to_string(),
                content: Some(content),
                tool_calls: None,
                tool_call_id: None,
            });
        }

        messages
    }

    /// Build with an additional user message appended
    ///
    /// Used when processing a new incoming mention that hasn't been stored yet
    pub fn build_with_user_message<F>(
        &self, thread: &[Conversation], new_message: &str, author_did: &str, resolve_handle: F,
    ) -> Vec<Message>
    where
        F: Fn(&str) -> String,
    {
        let handle = resolve_handle(author_did);
        let mut messages = self.build(thread, resolve_handle);
        messages.push(Message::user(format!("[@{}]: {}", handle, new_message)));
        messages
    }

    /// Get the system instruction
    pub fn system_instruction(&self) -> &str {
        &self.system_instruction
    }

    /// Update the system instruction
    pub fn set_system_instruction(&mut self, instruction: impl Into<String>) {
        self.system_instruction = instruction.into();
    }
}

/// Default system instruction for the bot
pub const DEFAULT_CONSTITUTION: &str = r#"You are ThunderBot, a helpful AI assistant on Bluesky.

Your characteristics:
- Stateful and persistent: You remember conversations across replies
- Helpful and friendly: You assist users with their questions
- Concise: You keep responses brief (max 300 characters for Bluesky)
- Multi-user aware: You can distinguish between different users in a thread

When responding:
- Be natural and conversational
- Reference context from earlier in the thread when relevant
- You may choose not to reply by including <SILENT_THOUGHT> in your response

You are participating in a public social media conversation. Keep responses appropriate."#;

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_conversation(role: Role, author_did: &str, content: &str) -> Conversation {
        Conversation {
            id: 1,
            root_uri: "at://root".to_string(),
            post_uri: "at://post".to_string(),
            parent_uri: None,
            author_did: author_did.to_string(),
            role,
            content: content.to_string(),
            cid: None,
            created_at: "2024-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn test_prompt_builder_basic() {
        let builder = PromptBuilder::new("You are a test bot.");
        let thread = vec![create_test_conversation(Role::User, "did:plc:alice", "Hello!")];

        let resolve = |did: &str| {
            if did == "did:plc:alice" { "alice.bsky.social".to_string() } else { did.to_string() }
        };

        let messages = builder.build(&thread, resolve);

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "system");
        assert_eq!(messages[0].content, Some("You are a test bot.".to_string()));
        assert_eq!(messages[1].role, "user");
        assert_eq!(messages[1].content, Some("[@alice.bsky.social]: Hello!".to_string()));
    }

    #[test]
    fn test_prompt_builder_multi_user() {
        let builder = PromptBuilder::new("You are a test bot.");
        let thread = vec![
            create_test_conversation(Role::User, "did:plc:alice", "Hello!"),
            create_test_conversation(Role::Model, "did:plc:bot", "Hi Alice!"),
            create_test_conversation(Role::User, "did:plc:bob", "What did Alice say?"),
        ];

        let resolve = |did: &str| match did {
            "did:plc:alice" => "alice.bsky.social".to_string(),
            "did:plc:bob" => "bob.bsky.social".to_string(),
            "did:plc:bot" => "bot.bsky.social".to_string(),
            _ => did.to_string(),
        };

        let messages = builder.build(&thread, resolve);

        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0].role, "system");
        assert_eq!(messages[1].role, "user");
        assert_eq!(messages[1].content, Some("[@alice.bsky.social]: Hello!".to_string()));
        assert_eq!(messages[2].role, "assistant");
        assert_eq!(messages[2].content, Some("Hi Alice!".to_string()));
        assert_eq!(messages[3].role, "user");
        assert_eq!(
            messages[3].content,
            Some("[@bob.bsky.social]: What did Alice say?".to_string())
        );
    }

    #[test]
    fn test_prompt_builder_model_role_no_prefix() {
        let builder = PromptBuilder::new("Test.");
        let thread = vec![create_test_conversation(Role::Model, "did:plc:bot", "I am the bot.")];

        let messages = builder.build(&thread, |_| "bot.bsky.social".to_string());

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[1].role, "assistant");
        assert_eq!(messages[1].content, Some("I am the bot.".to_string()));
    }

    #[test]
    fn test_prompt_builder_fallback_to_did() {
        let builder = PromptBuilder::new("Test.");
        let thread = vec![create_test_conversation(Role::User, "did:plc:unknown", "Hello")];

        let resolve = |did: &str| did.to_string();
        let messages = builder.build(&thread, resolve);

        assert_eq!(messages[1].content, Some("[@did:plc:unknown]: Hello".to_string()));
    }

    #[test]
    fn test_build_with_user_message() {
        let builder = PromptBuilder::new("Test.");
        let thread = vec![create_test_conversation(Role::User, "did:plc:alice", "First message")];

        let resolve = |did: &str| {
            if did == "did:plc:alice" {
                "alice.bsky.social".to_string()
            } else if did == "did:plc:bob" {
                "bob.bsky.social".to_string()
            } else {
                did.to_string()
            }
        };

        let messages = builder.build_with_user_message(&thread, "New message", "did:plc:bob", resolve);

        assert_eq!(messages.len(), 3);
        assert_eq!(messages[2].role, "user");
        assert_eq!(messages[2].content, Some("[@bob.bsky.social]: New message".to_string()));
    }

    #[test]
    fn test_empty_thread() {
        let builder = PromptBuilder::new("You are a bot.");
        let thread: Vec<Conversation> = vec![];

        let messages = builder.build(&thread, |_| panic!("should not be called"));

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, "system");
    }

    #[test]
    fn test_set_system_instruction() {
        let mut builder = PromptBuilder::new("Original.");
        assert_eq!(builder.system_instruction(), "Original.");

        builder.set_system_instruction("Updated.");
        assert_eq!(builder.system_instruction(), "Updated.");
    }
}
