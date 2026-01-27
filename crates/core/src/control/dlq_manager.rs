//! Dead letter queue management with auto-retry and purge.
use crate::db::DatabaseRepository;
use crate::jetstream::event::JetstreamEvent;

use anyhow::Result;
use chrono::Utc;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Manages the dead letter queue with automatic retry and purge.
pub struct DlqManager {
    db: Arc<dyn DatabaseRepository>,
    event_sender: mpsc::Sender<JetstreamEvent>,
    max_retries: u32,
}

impl DlqManager {
    /// Create a new DLQ manager.
    pub fn new(db: Arc<dyn DatabaseRepository>, sender: mpsc::Sender<JetstreamEvent>) -> Self {
        Self { db, event_sender: sender, max_retries: 5 }
    }

    /// Start automatic retry background task.
    pub fn start_auto_retry(&self) {
        let manager = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60)); // Check every minute

            loop {
                interval.tick().await;

                match manager.db.get_dlq_items(100).await {
                    Ok(items) => {
                        for item in items {
                            if item.retry_count >= manager.max_retries {
                                continue;
                            }

                            let elapsed = Utc::now() - item.created_at;
                            let backoff_minutes = 2u64.pow(item.retry_count);

                            if elapsed.num_minutes() >= backoff_minutes as i64 {
                                tracing::info!("Retrying DLQ item: {} (attempt {})", item.id, item.retry_count + 1);

                                if let Ok(event) = serde_json::from_str::<JetstreamEvent>(&item.event_json)
                                    && manager.event_sender.send(event).await.is_ok()
                                {
                                    let _ = manager.db.remove_from_dlq(&item.id).await;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to fetch DLQ items: {}", e);
                    }
                }
            }
        });
    }

    /// Start automatic purge task (daily cleanup of old items).
    pub fn start_purge_task(&self) {
        let db = self.db.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(86400)); // Daily

            loop {
                interval.tick().await;

                match db.purge_old_dlq_items(10).await {
                    Ok(count) => {
                        if count > 0 {
                            tracing::info!("Purged {} old DLQ items (>10 days)", count);
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to purge old DLQ items: {}", e);
                    }
                }
            }
        });
    }

    /// Retry a specific DLQ item immediately.
    pub async fn retry_item(&self, id: &str) -> Result<()> {
        let item = self.db.get_dlq_item(id).await?;

        if let Ok(event) = serde_json::from_str::<JetstreamEvent>(&item.event_json) {
            self.event_sender.send(event).await?;
            self.db.remove_from_dlq(id).await?;
            tracing::info!("Manually retried DLQ item: {}", id);
        }

        Ok(())
    }
}

impl Clone for DlqManager {
    fn clone(&self) -> Self {
        Self { db: Arc::clone(&self.db), event_sender: self.event_sender.clone(), max_retries: self.max_retries }
    }
}
