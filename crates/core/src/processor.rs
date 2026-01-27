use crate::db::types::ActivityLogRow;
use crate::db::{ConversationRow, Db, ThreadContextBuilder};
use crate::jetstream::event::{JetstreamEvent, PostRecord};

use chrono::Utc;
use tokio::sync::mpsc;
use uuid::Uuid;

#[derive(Clone)]
#[allow(dead_code)]
pub struct EventProcessor {
    event_tx: mpsc::Sender<JetstreamEvent>,
    db: Db,
}

impl EventProcessor {
    pub fn new(buffer_size: usize, db: Db) -> Self {
        let (event_tx, mut event_rx) = mpsc::channel(buffer_size);
        let db_clone = db.clone();

        tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                if let Err(e) = Self::process_event(event, &db_clone).await {
                    tracing::error!("Error processing event: {}", e);
                }
            }
        });

        Self { event_tx, db }
    }

    pub async fn send(&self, event: JetstreamEvent) -> anyhow::Result<()> {
        self.event_tx.send(event).await?;
        Ok(())
    }

    async fn process_event(event: JetstreamEvent, db: &Db) -> anyhow::Result<()> {
        let commit = match event {
            JetstreamEvent::Commit(commit) => commit,
            _ => return Ok(()),
        };

        let record = if let Some(record_value) = commit.commit.record {
            serde_json::from_value::<PostRecord>(record_value)?
        } else {
            return Ok(());
        };

        let post_uri = format!("at://{}/app.bsky.feed.post/{}", commit.did, commit.commit.rkey);

        let root_uri = ThreadContextBuilder::determine_root_uri(&post_uri, record.reply.as_ref());
        let parent_uri = ThreadContextBuilder::extract_parent_uri(record.reply.as_ref());

        let conversation_row = ConversationRow {
            id: Uuid::new_v4().to_string(),
            thread_root_uri: root_uri.clone(),
            post_uri: post_uri.clone(),
            parent_uri,
            author_did: commit.did.clone(),
            role: "user".to_string(),
            content: record.text.clone(),
            created_at: Utc::now(),
        };

        db.save_conversation(conversation_row.clone()).await?;

        let activity = ActivityLogRow {
            id: Uuid::new_v4().to_string(),
            action_type: "ingest".to_string(),
            description: format!("Ingested post from {}", commit.did),
            thread_uri: Some(root_uri),
            metadata_json: Some(
                serde_json::json!({"post_uri": post_uri, "author_did": commit.did, "content_length": record.text.len()})
                    .to_string(),
            ),
            created_at: Utc::now(),
        };
        if let Err(e) = db.log_activity(activity).await {
            tracing::warn!("Failed to log activity: {}", e);
        }

        tracing::info!("Saved conversation for DID: {}", commit.did);

        Ok(())
    }
}
