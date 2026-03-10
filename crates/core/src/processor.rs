//! Database event processor - writes incoming mentions to the database
//!
//! This processor implements the EventProcessor trait and stores incoming
//! mention events to the conversations table with proper thread tracking.

use crate::db::models::{CreateConversationParams, Role};
use crate::db::repository::{ConversationRepository, LibsqlRepository};
use crate::jetstream::filter::FilteredEvent;
use crate::jetstream::pipeline::{EventProcessor, ProcessedEvent};
use crate::jetstream::types::JetstreamEvent;
use crate::services::thread::{extract_created_at, extract_parent_uri, extract_root_uri, extract_text};
use std::sync::Arc;

/// Event processor that stores mentions to the database
pub struct DatabaseEventProcessor {
    repo: Arc<LibsqlRepository>,
}

impl DatabaseEventProcessor {
    /// Create a new database event processor
    pub fn new(repo: Arc<LibsqlRepository>) -> Self {
        Self { repo }
    }

    /// Get the repository reference
    pub fn repo(&self) -> &Arc<LibsqlRepository> {
        &self.repo
    }
}

#[async_trait::async_trait]
impl EventProcessor for DatabaseEventProcessor {
    async fn process(
        &self, mut event: FilteredEvent,
    ) -> Result<ProcessedEvent, Box<dyn std::error::Error + Send + Sync>> {
        match &event.event {
            JetstreamEvent::Commit { did: author_did, commit, .. } => {
                let post_uri = format!("at://{}/app.bsky.feed.post/{}", author_did, commit.rkey);

                let record = match &commit.record {
                    Some(r) => r,
                    None => {
                        tracing::warn!("Commit has no record, skipping");
                        return Ok(ProcessedEvent {
                            event,
                            success: false,
                            error: Some("No record in commit".to_string()),
                        });
                    }
                };

                let root_uri = extract_root_uri(&post_uri, record);
                let parent_uri = extract_parent_uri(record);
                let content = extract_text(record);
                let created_at = extract_created_at(record).unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
                let cid = commit.cid.clone();

                tracing::debug!(
                    post_uri = %post_uri,
                    root_uri = %root_uri,
                    author_did = %author_did,
                    "Processing mention for database storage"
                );

                let params = CreateConversationParams {
                    root_uri,
                    post_uri: post_uri.clone(),
                    parent_uri,
                    author_did: author_did.clone(),
                    role: Role::User,
                    content,
                    cid,
                    created_at,
                };

                match self.repo.create_conversation(params).await {
                    Ok(inserted) => {
                        if inserted {
                            tracing::info!(post_uri = %post_uri, "Stored new mention in database");
                        } else {
                            tracing::debug!(post_uri = %post_uri, "Mention already exists in database");
                        }
                        event.acknowledge();
                        Ok(ProcessedEvent { event, success: true, error: None })
                    }
                    Err(e) => {
                        tracing::error!(post_uri = %post_uri, error = %e, "Failed to store mention");
                        Ok(ProcessedEvent { event, success: false, error: Some(e.to_string()) })
                    }
                }
            }
            _ => {
                tracing::warn!("Received non-commit event in database processor");
                Ok(ProcessedEvent { event, success: false, error: Some("Non-commit event received".to_string()) })
            }
        }
    }
}

/// Create a shared database event processor
pub fn create_database_processor(repo: Arc<LibsqlRepository>) -> Arc<DatabaseEventProcessor> {
    Arc::new(DatabaseEventProcessor::new(repo))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{migrations, repository::ConversationRepository};
    use crate::jetstream::filter::EventFilter;
    use crate::jetstream::types::{CommitData, CommitOperation};
    use std::time::{SystemTime, UNIX_EPOCH};
    use tempfile::TempDir;

    async fn setup_test_db() -> (Arc<LibsqlRepository>, libsql::Database, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let db_path = temp_dir.path().join(format!("test_{}.db", timestamp));

        let db = libsql::Builder::new_local(db_path.to_str().unwrap())
            .build()
            .await
            .unwrap();

        migrations::run_migrations(&db).await.unwrap();

        let conn = db.connect().unwrap();
        let repo = Arc::new(LibsqlRepository::new(conn));

        (repo, db, temp_dir)
    }

    fn create_mention_event(author_did: &str, rkey: &str, text: &str) -> FilteredEvent {
        let record = serde_json::json!({
            "text": text,
            "facets": [
                {
                    "index": { "byteStart": 0, "byteEnd": 4 },
                    "features": [
                        {
                            "$type": "app.bsky.richtext.facet#mention",
                            "did": "did:plc:bot123"
                        }
                    ]
                }
            ],
            "createdAt": "2024-01-01T00:00:00.000Z"
        });

        let event = JetstreamEvent::Commit {
            did: author_did.to_string(),
            time_us: 1234567890,
            commit: CommitData {
                rev: "test".to_string(),
                operation: CommitOperation::Create,
                collection: "app.bsky.feed.post".to_string(),
                rkey: rkey.to_string(),
                record: Some(record),
                cid: Some("bafyrei...".to_string()),
            },
        };

        let filter = EventFilter::new("did:plc:bot123");
        filter.filter(event).unwrap()
    }

    #[tokio::test]
    async fn test_database_processor_stores_mention() {
        let (repo, _, _) = setup_test_db().await;
        let processor = DatabaseEventProcessor::new(repo.clone());

        let event = create_mention_event("did:plc:user456", "test123", "@bot hello");
        let result = processor.process(event).await.unwrap();

        assert!(result.success, "Expected success but got error: {:?}", result.error);
        assert!(result.event.is_acknowledged());

        let post_uri = "at://did:plc:user456/app.bsky.feed.post/test123";
        let stored = repo.get_by_post_uri(post_uri).await.unwrap();
        assert!(stored.is_some());

        let conv = stored.unwrap();
        assert_eq!(conv.author_did, "did:plc:user456");
        assert_eq!(conv.content, "@bot hello");
        assert_eq!(conv.role, Role::User);
    }

    #[tokio::test]
    async fn test_database_processor_idempotent() {
        let (repo, _, _) = setup_test_db().await;
        let processor = DatabaseEventProcessor::new(repo.clone());

        let event1 = create_mention_event("did:plc:user789", "test456", "@bot hello");
        let result1 = processor.process(event1).await.unwrap();
        assert!(result1.success);

        let event2 = create_mention_event("did:plc:user789", "test456", "@bot hello");
        let result2 = processor.process(event2).await.unwrap();
        assert!(result2.success);

        let count = repo.count().await.unwrap();
        assert_eq!(count, 1);
    }
}
