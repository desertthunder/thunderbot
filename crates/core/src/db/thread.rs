use super::repository::Db;
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadContext {
    pub root_uri: String,
    pub messages: Vec<ContextMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextMessage {
    pub post_uri: String,
    pub author_did: String,
    pub role: String,
    pub content: String,
    pub created_at: String,
}

pub struct ThreadContextBuilder {
    db: Db,
}

impl ThreadContextBuilder {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn build(&self, root_uri: &str) -> Result<ThreadContext> {
        let rows = self.db.get_thread_history(root_uri).await?;

        let messages = rows
            .into_iter()
            .map(|row| ContextMessage {
                post_uri: row.post_uri,
                author_did: row.author_did,
                role: row.role,
                content: row.content,
                created_at: row.created_at.to_rfc3339(),
            })
            .collect();

        Ok(ThreadContext { root_uri: root_uri.to_string(), messages })
    }

    pub async fn build_with_handle_context(
        &self, root_uri: &str, identity_resolver: &crate::db::IdentityResolver,
    ) -> Result<String> {
        let context = self.build(root_uri).await?;

        let mut formatted_messages = Vec::new();

        for msg in context.messages {
            let handle = identity_resolver
                .resolve_did_to_handle(&msg.author_did)
                .await
                .unwrap_or_else(|_| msg.author_did.clone());

            let formatted = if msg.role == "model" {
                format!("[Bot]: {}", msg.content)
            } else {
                format!("[@{}]: {}", handle, msg.content)
            };

            formatted_messages.push(formatted);
        }

        Ok(formatted_messages.join("\n"))
    }

    pub fn determine_root_uri(post_uri: &str, reply_ref: Option<&crate::jetstream::event::ReplyRef>) -> String {
        if let Some(reply) = reply_ref { reply.root.uri.clone() } else { post_uri.to_string() }
    }

    pub fn extract_parent_uri(reply_ref: Option<&crate::jetstream::event::ReplyRef>) -> Option<String> {
        reply_ref.map(|r| r.parent.uri.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_determine_root_uri_with_reply() {
        let reply_ref = crate::jetstream::event::ReplyRef {
            root: crate::jetstream::event::StrongRef {
                uri: "at://did:plc:xxx/app.bsky.feed.post/root".to_string(),
                cid: "root_cid".to_string(),
            },
            parent: crate::jetstream::event::StrongRef {
                uri: "at://did:plc:xxx/app.bsky.feed.post/parent".to_string(),
                cid: "parent_cid".to_string(),
            },
        };

        let root_uri =
            ThreadContextBuilder::determine_root_uri("at://did:plc:xxx/app.bsky.feed.post/child", Some(&reply_ref));

        assert_eq!(root_uri, "at://did:plc:xxx/app.bsky.feed.post/root");
    }

    #[test]
    fn test_determine_root_uri_without_reply() {
        let root_uri = ThreadContextBuilder::determine_root_uri("at://did:plc:xxx/app.bsky.feed.post/new", None);

        assert_eq!(root_uri, "at://did:plc:xxx/app.bsky.feed.post/new");
    }
}
