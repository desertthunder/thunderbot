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
    jetstream_state: std::sync::Arc<tokio::sync::RwLock<crate::health::JetstreamState>>,
}

impl EventProcessor {
    pub fn new(
        buffer_size: usize, db: Db, jetstream_state: std::sync::Arc<tokio::sync::RwLock<crate::health::JetstreamState>>,
    ) -> Self {
        let (event_tx, mut event_rx) = mpsc::channel(buffer_size);
        let db_clone = db.clone();
        let state_clone = jetstream_state.clone();

        tokio::spawn(async move {
            let mut last_calc = std::time::Instant::now();
            let mut processed_in_window = 0;

            while let Some(event) = event_rx.recv().await {
                {
                    let mut state = state_clone.write().await;
                    state.queue_depth = event_rx.len();
                    state.record_event();
                }

                if let Err(e) = Self::process_event(&event, &db_clone).await {
                    tracing::error!("Error processing event: {}", e);
                    let dlq_item = crate::control::DeadLetterItem {
                        id: uuid::Uuid::new_v4().to_string(),
                        event_json: serde_json::to_string(&event).unwrap_or_default(),
                        error_message: e.to_string(),
                        retry_count: 0,
                        last_retry_at: None,
                        created_at: chrono::Utc::now(),
                    };
                    if let Err(dlq_err) = db_clone.add_to_dlq(dlq_item).await {
                        tracing::error!("Failed to send to DLQ: {}", dlq_err);
                    }
                }

                processed_in_window += 1;

                if last_calc.elapsed().as_secs() >= 1 {
                    let mut state = state_clone.write().await;
                    state.events_per_second = processed_in_window as f64 / last_calc.elapsed().as_secs_f64();
                    processed_in_window = 0;
                    last_calc = std::time::Instant::now();
                }
            }
        });

        Self { event_tx, db, jetstream_state }
    }

    pub async fn send(&self, event: JetstreamEvent) -> anyhow::Result<()> {
        self.event_tx.send(event).await?;
        Ok(())
    }

    async fn process_event(event: &JetstreamEvent, db: &Db) -> anyhow::Result<()> {
        let commit = match event {
            JetstreamEvent::Commit(commit) => commit,
            _ => return Ok(()),
        };

        if db.is_blocked(&commit.did).await.unwrap_or(false) {
            tracing::debug!(" skipping blocked author: {}", commit.did);
            return Ok(());
        }

        let record = if let Some(ref record_value) = commit.commit.record {
            serde_json::from_value::<PostRecord>(record_value.clone())?
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
