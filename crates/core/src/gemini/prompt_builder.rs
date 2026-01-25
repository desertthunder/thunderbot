use super::types::*;
use crate::db::{IdentityResolver, ThreadContextBuilder};
use anyhow::Result;

pub struct PromptBuilder {
    thread_builder: ThreadContextBuilder,
    identity_resolver: IdentityResolver,
    system_instruction: String,
}

impl PromptBuilder {
    const DEFAULT_SYSTEM_INSTRUCTION: &'static str = "You are a helpful, stateful AI agent on Bluesky. You are persistent, friendly, and engage in meaningful conversations. Keep your responses concise (under 280 characters when possible). If you choose not to respond, use <SILENT_THOUGHT> as your entire response.";

    pub fn new(
        thread_builder: ThreadContextBuilder, identity_resolver: IdentityResolver, system_instruction: Option<String>,
    ) -> Self {
        Self {
            thread_builder,
            identity_resolver,
            system_instruction: system_instruction.unwrap_or_else(|| Self::DEFAULT_SYSTEM_INSTRUCTION.to_string()),
        }
    }

    pub async fn build_for_thread(&self, root_uri: &str) -> Result<Prompt> {
        let context = self.thread_builder.build(root_uri).await?;

        let mut history = Vec::new();

        for msg in context.messages {
            let handle = self
                .identity_resolver
                .resolve_did_to_handle(&msg.author_did)
                .await
                .unwrap_or_else(|_| msg.author_did.clone());

            let formatted_content =
                if msg.role == "model" { msg.content.clone() } else { format!("[@{}]: {}", handle, msg.content) };

            history.push(ChatMessage { role: msg.role.clone(), content: formatted_content });
        }

        Ok(Prompt { system_instruction: self.system_instruction.clone(), history })
    }

    pub fn build_for_text(&self, text: &str) -> Result<Prompt> {
        Ok(Prompt {
            system_instruction: self.system_instruction.clone(),
            history: vec![ChatMessage { role: "user".to_string(), content: text.to_string() }],
        })
    }

    pub async fn build_for_conversation_rows(&self, rows: &[crate::db::ConversationRow]) -> Result<Prompt> {
        let mut history = Vec::new();

        for row in rows {
            let handle = self
                .identity_resolver
                .resolve_did_to_handle(&row.author_did)
                .await
                .unwrap_or_else(|_| row.author_did.clone());

            let formatted_content =
                if row.role == "model" { row.content.clone() } else { format!("[@{}]: {}", handle, row.content) };

            history.push(ChatMessage { role: row.role.clone(), content: formatted_content });
        }

        Ok(Prompt { system_instruction: self.system_instruction.clone(), history })
    }

    pub fn to_gemini_request(&self, prompt: &Prompt) -> Result<GenerateContentRequest> {
        let mut contents = Vec::new();

        let system_content = Content {
            parts: vec![Part::Text { text: prompt.system_instruction.clone() }],
            role: Some("system".to_string()),
        };
        contents.push(system_content);

        for msg in &prompt.history {
            contents
                .push(Content { parts: vec![Part::Text { text: msg.content.clone() }], role: Some(msg.role.clone()) });
        }

        let generation_config = Some(GenerationConfig {
            temperature: Some(0.7),
            top_p: Some(0.9),
            top_k: Some(40),
            max_output_tokens: Some(1024),
            thinking_config: Some(ThinkingConfig { include_thoughts: false }),
        });

        Ok(GenerateContentRequest { contents, generation_config, system_instruction: None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_serialization() {
        let prompt = Prompt {
            system_instruction: "Test instruction".to_string(),
            history: vec![
                ChatMessage { role: "user".to_string(), content: "Hello".to_string() },
                ChatMessage { role: "model".to_string(), content: "Hi there!".to_string() },
            ],
        };

        let json = serde_json::to_string(&prompt).unwrap();
        assert!(json.contains("system_instruction"));
        assert!(json.contains("history"));
    }
}
