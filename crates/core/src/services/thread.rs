//! Thread context reconstruction module
//!
//! This module provides functionality to:
//! - Extract root_uri from incoming posts
//! - Reconstruct conversation threads from the database
//! - Handle orphaned replies

use crate::db::models::{Conversation, CreateConversationParams, Role};
use crate::db::repository::ConversationRepository;
use crate::error::BotError;
use serde_json::Value;

/// Extract the root URI from a post record
///
/// If the post is a reply, returns the root URI from the reply structure.
/// If the post is not a reply, returns the post's own URI (it is the root).
pub fn extract_root_uri(post_uri: &str, record: &Value) -> String {
    if let Some(reply) = record.get("reply") {
        if let Some(root) = reply.get("root")
            && let Some(uri) = root.get("uri").and_then(|u| u.as_str())
        {
            tracing::debug!("Post {} is a reply, root_uri: {}", post_uri, uri);
            return uri.to_string();
        }

        if let Some(parent) = reply.get("parent")
            && let Some(uri) = parent.get("uri").and_then(|u| u.as_str())
        {
            tracing::debug!(
                "Post {} is a reply without root.uri, using parent.uri as root: {}",
                post_uri,
                uri
            );
            return uri.to_string();
        }
    }

    tracing::debug!("Post {} is a root post", post_uri);
    post_uri.to_string()
}

/// Extract the parent URI from a post record
pub fn extract_parent_uri(record: &Value) -> Option<String> {
    if let Some(reply) = record.get("reply")
        && let Some(parent) = reply.get("parent")
        && let Some(uri) = parent.get("uri").and_then(|u| u.as_str())
    {
        Some(uri.to_string())
    } else {
        None
    }
}

/// Extract the root CID from a post record reply structure
pub fn extract_root_cid(record: &Value) -> Option<String> {
    if let Some(reply) = record.get("reply")
        && let Some(root) = reply.get("root")
        && let Some(cid) = root.get("cid").and_then(|c| c.as_str())
    {
        Some(cid.to_string())
    } else {
        None
    }
}

/// Extract the parent CID from a post record reply structure
pub fn extract_parent_cid(record: &Value) -> Option<String> {
    if let Some(reply) = record.get("reply")
        && let Some(parent) = reply.get("parent")
        && let Some(cid) = parent.get("cid").and_then(|c| c.as_str())
    {
        Some(cid.to_string())
    } else {
        None
    }
}

/// Extract text content from a post record
pub fn extract_text(record: &Value) -> String {
    record.get("text").and_then(|t| t.as_str()).unwrap_or("").to_string()
}

/// Extract created_at timestamp from a post record
pub fn extract_created_at(record: &Value) -> Option<String> {
    record.get("createdAt").and_then(|t| t.as_str()).map(|s| s.to_string())
}

/// Represents a reconstructed thread context
#[derive(Debug, Clone)]
pub struct ThreadContext {
    pub root_uri: String,
    pub messages: Vec<Conversation>,
    pub is_orphaned: bool,
    pub missing_parent: Option<String>,
}

/// Service for thread context reconstruction
pub struct ThreadReconstructor<R: ConversationRepository> {
    repo: R,
}

impl<R: ConversationRepository> ThreadReconstructor<R> {
    /// Create a new thread reconstructor
    pub fn new(repo: R) -> Self {
        Self { repo }
    }

    /// Reconstruct a thread from a root URI
    ///
    /// Fetches all messages in the thread ordered by created_at.
    /// Returns a ThreadContext containing the full conversation history.
    pub async fn reconstruct_thread(&self, root_uri: &str) -> Result<ThreadContext, BotError> {
        let messages = self.repo.get_thread_by_root(root_uri).await?;

        Ok(ThreadContext { root_uri: root_uri.to_string(), messages, is_orphaned: false, missing_parent: None })
    }

    /// Process a new incoming post and add it to the conversation
    ///
    /// This method:
    /// 1. Extracts the root_uri and parent_uri from the post
    /// 2. Stores the post in the database
    /// 3. Returns the reconstructed thread context
    /// 4. Optionally checks for orphaned parents
    pub async fn process_incoming_post(
        &self, post_uri: &str, author_did: &str, cid: Option<&str>, record: &Value, check_orphans: bool,
    ) -> Result<ThreadContext, BotError> {
        let root_uri = extract_root_uri(post_uri, record);
        let parent_uri = extract_parent_uri(record);
        let content = extract_text(record);
        let created_at = extract_created_at(record).unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

        tracing::debug!(
            "Processing post {} with root_uri: {}, parent_uri: {:?}",
            post_uri,
            root_uri,
            parent_uri
        );

        let params = CreateConversationParams {
            root_uri: root_uri.clone(),
            post_uri: post_uri.to_string(),
            parent_uri: parent_uri.clone(),
            author_did: author_did.to_string(),
            role: Role::User,
            content,
            cid: cid.map(|s| s.to_string()),
            created_at,
        };

        let _ = self.repo.create_conversation(params).await?;

        let mut missing_parent = None;
        let mut is_orphaned = false;

        if check_orphans && let Some(ref parent) = parent_uri {
            let parent_exists = self.repo.get_by_post_uri(parent).await?.is_some();
            if !parent_exists {
                tracing::warn!(
                    "Orphaned reply detected: post {} references missing parent {}",
                    post_uri,
                    parent
                );
                missing_parent = Some(parent.clone());
                is_orphaned = true;
            }
        }

        let messages = self.repo.get_thread_by_root(&root_uri).await?;

        Ok(ThreadContext { root_uri, messages, is_orphaned, missing_parent })
    }

    /// Get the linear conversation history formatted for AI context
    ///
    /// Returns messages in chronological order (oldest first) with proper role information.
    pub fn format_thread_for_prompt(
        &self, thread: &ThreadContext, resolve_handles: &dyn Fn(&str) -> String,
    ) -> Vec<(ConversationRole, String, String)> {
        thread
            .messages
            .iter()
            .map(|msg| {
                let role = match msg.role {
                    Role::User => ConversationRole::User,
                    Role::Model => ConversationRole::Model,
                };
                let author = resolve_handles(&msg.author_did);
                (role, author, msg.content.clone())
            })
            .collect()
    }

    /// Check if a thread contains messages from the bot (model role)
    pub fn thread_has_bot_participation(thread: &ThreadContext) -> bool {
        thread.messages.iter().any(|msg| msg.role == Role::Model)
    }
}

/// Role in the conversation for prompt formatting
#[derive(Debug, Clone, PartialEq)]
pub enum ConversationRole {
    User,
    Model,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_extract_root_uri_from_root_post() {
        let post_uri = "at://did:plc:abc/app.bsky.feed.post/123";
        let record = json!({
            "text": "Hello world",
            "createdAt": "2024-01-01T00:00:00Z"
        });

        let root_uri = extract_root_uri(post_uri, &record);
        assert_eq!(root_uri, post_uri);
    }

    #[test]
    fn test_extract_root_uri_from_reply() {
        let post_uri = "at://did:plc:abc/app.bsky.feed.post/456";
        let root_uri_expected = "at://did:plc:def/app.bsky.feed.post/123";
        let record = json!({
            "text": "Reply text",
            "reply": {
                "root": {
                    "uri": root_uri_expected,
                    "cid": "bafyrei..."
                },
                "parent": {
                    "uri": "at://did:plc:def/app.bsky.feed.post/123",
                    "cid": "bafyrei..."
                }
            },
            "createdAt": "2024-01-01T00:00:00Z"
        });

        let root_uri = extract_root_uri(post_uri, &record);
        assert_eq!(root_uri, root_uri_expected);
    }

    #[test]
    fn test_extract_parent_uri() {
        let parent_uri = "at://did:plc:def/app.bsky.feed.post/123";
        let record = json!({
            "text": "Reply text",
            "reply": {
                "root": {
                    "uri": "at://did:plc:def/app.bsky.feed.post/111",
                    "cid": "bafyrei..."
                },
                "parent": {
                    "uri": parent_uri,
                    "cid": "bafyrei..."
                }
            },
            "createdAt": "2024-01-01T00:00:00Z"
        });

        assert_eq!(extract_parent_uri(&record), Some(parent_uri.to_string()));
    }

    #[test]
    fn test_extract_root_and_parent_cid() {
        let record = json!({
            "text": "Reply text",
            "reply": {
                "root": {
                    "uri": "at://did:plc:def/app.bsky.feed.post/111",
                    "cid": "bafyroot"
                },
                "parent": {
                    "uri": "at://did:plc:def/app.bsky.feed.post/123",
                    "cid": "bafyparent"
                }
            },
            "createdAt": "2024-01-01T00:00:00Z"
        });

        assert_eq!(extract_root_cid(&record), Some("bafyroot".to_string()));
        assert_eq!(extract_parent_cid(&record), Some("bafyparent".to_string()));
    }

    #[test]
    fn test_extract_root_uri_falls_back_to_parent_uri() {
        let post_uri = "at://did:plc:abc/app.bsky.feed.post/456";
        let parent_uri = "at://did:plc:def/app.bsky.feed.post/123";
        let record = json!({
            "text": "Reply text",
            "reply": {
                "parent": {
                    "uri": parent_uri,
                    "cid": "bafyrei..."
                }
            },
            "createdAt": "2024-01-01T00:00:00Z"
        });

        let root_uri = extract_root_uri(post_uri, &record);
        assert_eq!(root_uri, parent_uri);
    }

    #[test]
    fn test_extract_parent_uri_from_root() {
        let record = json!({
            "text": "Root post",
            "createdAt": "2024-01-01T00:00:00Z"
        });

        assert_eq!(extract_parent_uri(&record), None);
    }

    #[test]
    fn test_extract_text() {
        let record = json!({
            "text": "Hello world",
            "createdAt": "2024-01-01T00:00:00Z"
        });

        assert_eq!(extract_text(&record), "Hello world");
    }

    #[test]
    fn test_extract_text_missing() {
        let record = json!({
            "createdAt": "2024-01-01T00:00:00Z"
        });

        assert_eq!(extract_text(&record), "");
    }

    #[test]
    fn test_extract_created_at() {
        let record = json!({
            "text": "Hello",
            "createdAt": "2024-01-01T00:00:00Z"
        });

        assert_eq!(extract_created_at(&record), Some("2024-01-01T00:00:00Z".to_string()));
    }
}
