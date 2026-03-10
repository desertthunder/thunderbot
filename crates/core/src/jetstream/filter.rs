use crate::jetstream::types::{CommitOperation, JetstreamEvent};
use std::sync::Arc;

/// Filter configuration for Jetstream events
#[derive(Debug, Clone)]
pub struct EventFilter {
    /// The bot's DID - only events mentioning this DID will pass through
    pub bot_did: String,
    /// Whether to log all events for debugging (99% should be discarded)
    pub log_discarded: bool,
}

impl EventFilter {
    pub fn new(bot_did: impl Into<String>) -> Self {
        Self { bot_did: bot_did.into(), log_discarded: false }
    }

    /// Check if an event should be processed
    pub fn filter(&self, event: JetstreamEvent) -> Option<FilteredEvent> {
        match &event {
            JetstreamEvent::Commit { did, time_us, commit } => {
                if commit.operation != CommitOperation::Create || commit.collection != "app.bsky.feed.post" {
                    self.log_discarded_event(&event, "not a post create operation");
                    return None;
                }

                if !commit.is_mention_of(&self.bot_did) {
                    self.log_discarded_event(&event, "not mentioning bot");
                    return None;
                }

                tracing::trace!(
                    time_us = time_us,
                    author_did = %did,
                    rkey = %commit.rkey,
                    "Matched mention event"
                );

                Some(FilteredEvent { event, acknowledged: false })
            }
            JetstreamEvent::Identity { .. } => {
                self.log_discarded_event(&event, "identity event (not processed)");
                None
            }
            JetstreamEvent::Account { .. } => {
                self.log_discarded_event(&event, "account event (not processed)");
                None
            }
        }
    }

    fn log_discarded_event(&self, event: &JetstreamEvent, reason: &str) {
        if self.log_discarded {
            match event {
                JetstreamEvent::Commit { did, time_us, commit } => tracing::trace!(
                    time_us = time_us,
                    author_did = %did,
                    collection = %commit.collection,
                    operation = %commit.operation,
                    reason = %reason,
                    "Discarded event"
                ),

                JetstreamEvent::Identity { did, time_us, .. } => tracing::trace!(
                    time_us = time_us,
                    did = %did,
                    reason = %reason,
                    "Discarded identity event"
                ),

                JetstreamEvent::Account { did, time_us, account } => tracing::trace!(
                    time_us = time_us,
                    did = %did,
                    active = account.active,
                    reason = %reason,
                    "Discarded account event"
                ),
            }
        }
    }
}

/// A filtered event that needs to be processed
/// Includes acknowledgment tracking for at-least-once semantics
#[derive(Debug)]
pub struct FilteredEvent {
    pub event: JetstreamEvent,
    acknowledged: bool,
}

impl FilteredEvent {
    /// Mark this event as acknowledged
    /// Should be called after successful processing
    pub fn acknowledge(&mut self) {
        self.acknowledged = true;
        if let JetstreamEvent::Commit { time_us, .. } = &self.event {
            tracing::trace!(time_us = time_us, "Event acknowledged");
        }
    }

    pub fn is_acknowledged(&self) -> bool {
        self.acknowledged
    }

    /// Get the cursor (time_us) for this event
    pub fn cursor(&self) -> i64 {
        match &self.event {
            JetstreamEvent::Commit { time_us, .. }
            | JetstreamEvent::Identity { time_us, .. }
            | JetstreamEvent::Account { time_us, .. } => *time_us,
        }
    }
}

/// A filter that wraps an Arc for shared use across tasks
#[derive(Debug, Clone)]
pub struct SharedFilter {
    inner: Arc<EventFilter>,
}

impl SharedFilter {
    pub fn new(filter: EventFilter) -> Self {
        Self { inner: Arc::new(filter) }
    }

    pub fn filter(&self, event: JetstreamEvent) -> Option<FilteredEvent> {
        self.inner.filter(event)
    }
}

impl std::ops::Deref for SharedFilter {
    type Target = EventFilter;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jetstream::types::CommitData;

    fn create_mention_event(bot_did: &str) -> JetstreamEvent {
        let record = serde_json::json!({
            "text": "@bot hello",
            "facets": [
                {
                    "index": { "byteStart": 0, "byteEnd": 4 },
                    "features": [
                        {
                            "$type": "app.bsky.richtext.facet#mention",
                            "did": bot_did
                        }
                    ]
                }
            ]
        });

        JetstreamEvent::Commit {
            did: "did:plc:user123".to_string(),
            time_us: 1234567890,
            commit: CommitData {
                rev: "test".to_string(),
                operation: CommitOperation::Create,
                collection: "app.bsky.feed.post".to_string(),
                rkey: "test123".to_string(),
                record: Some(record),
                cid: Some("bafyrei...".to_string()),
            },
        }
    }

    fn create_non_mention_event() -> JetstreamEvent {
        let record = serde_json::json!({
            "text": "Just a regular post"
        });

        JetstreamEvent::Commit {
            did: "did:plc:user123".to_string(),
            time_us: 1234567890,
            commit: CommitData {
                rev: "test".to_string(),
                operation: CommitOperation::Create,
                collection: "app.bsky.feed.post".to_string(),
                rkey: "test123".to_string(),
                record: Some(record),
                cid: Some("bafyrei...".to_string()),
            },
        }
    }

    #[test]
    fn test_filter_passes_mention() {
        let bot_did = "did:plc:bot123";
        let filter = EventFilter::new(bot_did);
        let event = create_mention_event(bot_did);

        let result = filter.filter(event);
        assert!(result.is_some());

        let filtered = result.unwrap();
        assert_eq!(filtered.cursor(), 1234567890);
        assert!(!filtered.is_acknowledged());
    }

    #[test]
    fn test_filter_rejects_non_mention() {
        let filter = EventFilter::new("did:plc:bot123");
        let event = create_non_mention_event();

        let result = filter.filter(event);
        assert!(result.is_none());
    }

    #[test]
    fn test_filter_rejects_identity_event() {
        let filter = EventFilter::new("did:plc:bot123");
        let event = JetstreamEvent::Identity {
            did: "did:plc:user123".to_string(),
            time_us: 1234567890,
            identity: crate::jetstream::types::IdentityData {
                did: "did:plc:user123".to_string(),
                handle: "user.bsky.social".to_string(),
                seq: 1,
                time: "2024-01-01T00:00:00.000Z".to_string(),
            },
        };

        let result = filter.filter(event);
        assert!(result.is_none());
    }

    #[test]
    fn test_filtered_event_acknowledge() {
        let bot_did = "did:plc:bot123";
        let filter = EventFilter::new(bot_did);
        let event = create_mention_event(bot_did);

        let mut filtered = filter.filter(event).unwrap();
        assert!(!filtered.is_acknowledged());

        filtered.acknowledge();
        assert!(filtered.is_acknowledged());
    }

    #[test]
    fn test_shared_filter() {
        let bot_did = "did:plc:bot123";
        let filter = EventFilter::new(bot_did);
        let shared = SharedFilter::new(filter);
        let shared2 = shared.clone();

        let event = create_mention_event(bot_did);
        assert!(shared.filter(event.clone()).is_some());
        assert!(shared2.filter(event).is_some());
    }

    #[test]
    fn test_filter_rejects_delete_operation() {
        let bot_did = "did:plc:bot123";
        let filter = EventFilter::new(bot_did);
        let record = serde_json::json!({
            "facets": [
                {
                    "index": { "byteStart": 0, "byteEnd": 4 },
                    "features": [
                        {
                            "$type": "app.bsky.richtext.facet#mention",
                            "did": bot_did
                        }
                    ]
                }
            ]
        });

        let event = JetstreamEvent::Commit {
            did: "did:plc:user123".to_string(),
            time_us: 1234567890,
            commit: CommitData {
                rev: "test".to_string(),
                operation: CommitOperation::Delete,
                collection: "app.bsky.feed.post".to_string(),
                rkey: "test123".to_string(),
                record: Some(record),
                cid: None,
            },
        };

        let result = filter.filter(event);
        assert!(result.is_none(), "Should reject delete operations");
    }
}
