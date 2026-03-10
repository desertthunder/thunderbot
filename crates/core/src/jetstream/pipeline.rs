use crate::jetstream::filter::{FilteredEvent, SharedFilter};
use crate::jetstream::types::JetstreamEvent;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tokio::sync::{RwLock, mpsc};
use tokio::task::JoinHandle;

/// Configuration for the event processing pipeline
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// Number of worker tasks to process events
    pub num_workers: usize,
    /// Channel buffer size between ingestion and workers
    pub channel_buffer_size: usize,
    /// Maximum number of unacknowledged events before backpressure
    pub max_in_flight: usize,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self { num_workers: 4, channel_buffer_size: 1000, max_in_flight: 100 }
    }
}

/// Stats for the pipeline
#[derive(Debug, Default)]
pub struct PipelineStats {
    pub events_received: AtomicUsize,
    pub events_filtered: AtomicUsize,
    pub events_processed: AtomicUsize,
    pub events_failed: AtomicUsize,
    pub events_in_flight: AtomicUsize,
}

impl PipelineStats {
    pub fn snapshot(&self) -> PipelineStatsSnapshot {
        PipelineStatsSnapshot {
            events_received: self.events_received.load(Ordering::Relaxed),
            events_filtered: self.events_filtered.load(Ordering::Relaxed),
            events_processed: self.events_processed.load(Ordering::Relaxed),
            events_failed: self.events_failed.load(Ordering::Relaxed),
            events_in_flight: self.events_in_flight.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PipelineStatsSnapshot {
    pub events_received: usize,
    pub events_filtered: usize,
    pub events_processed: usize,
    pub events_failed: usize,
    pub events_in_flight: usize,
}

/// A processed event result
#[derive(Debug)]
pub struct ProcessedEvent {
    pub event: FilteredEvent,
    pub success: bool,
    pub error: Option<String>,
}

/// Event processor trait - implement this to process filtered events
#[async_trait::async_trait]
pub trait EventProcessor: Send + Sync {
    async fn process(&self, event: FilteredEvent) -> Result<ProcessedEvent, Box<dyn std::error::Error + Send + Sync>>;
}

/// The event processing pipeline
pub struct EventPipeline<P: EventProcessor> {
    config: PipelineConfig,
    filter: SharedFilter,
    processor: Arc<P>,
    stats: Arc<PipelineStats>,
    shutdown_signal: Arc<AtomicBool>,
    event_tx: mpsc::Sender<JetstreamEvent>,
    event_rx: Arc<RwLock<mpsc::Receiver<JetstreamEvent>>>,
    worker_handles: Arc<RwLock<Vec<JoinHandle<()>>>>,
}

impl<P: EventProcessor + 'static> EventPipeline<P> {
    pub fn new(config: PipelineConfig, filter: SharedFilter, processor: P) -> Self {
        let (event_tx, event_rx) = mpsc::channel(config.channel_buffer_size);

        Self {
            config,
            filter,
            processor: Arc::new(processor),
            stats: Arc::new(PipelineStats::default()),
            shutdown_signal: Arc::new(AtomicBool::new(false)),
            event_tx,
            event_rx: Arc::new(RwLock::new(event_rx)),
            worker_handles: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Start the pipeline workers
    pub async fn start(&self) {
        tracing::info!(
            num_workers = self.config.num_workers,
            "Starting event processing pipeline"
        );

        let mut handles = self.worker_handles.write().await;

        for worker_id in 0..self.config.num_workers {
            let worker = PipelineWorker {
                worker_id,
                filter: self.filter.clone(),
                processor: self.processor.clone(),
                stats: self.stats.clone(),
                shutdown_signal: self.shutdown_signal.clone(),
                event_rx: self.event_rx.clone(),
                max_in_flight: self.config.max_in_flight,
            };

            let handle = tokio::spawn(worker.run());
            handles.push(handle);
        }

        tracing::info!("All {} workers started", self.config.num_workers);
    }

    /// Get a sender to submit events to the pipeline
    pub fn event_sender(&self) -> mpsc::Sender<JetstreamEvent> {
        self.event_tx.clone()
    }

    /// Get current pipeline stats
    pub fn stats(&self) -> PipelineStatsSnapshot {
        self.stats.snapshot()
    }

    /// Request graceful shutdown
    pub async fn shutdown(&self, timeout_secs: u64) {
        tracing::info!("Requesting pipeline shutdown...");

        self.shutdown_signal.store(true, Ordering::SeqCst);

        let mut handles = self.worker_handles.write().await;
        let mut completed = 0;

        for (i, handle) in handles.iter_mut().enumerate() {
            match tokio::time::timeout(tokio::time::Duration::from_secs(timeout_secs), handle).await {
                Ok(_) => {
                    completed += 1;
                    tracing::debug!("Worker {} completed", i);
                }
                Err(_) => tracing::warn!("Worker {} shutdown timed out", i),
            }
        }

        tracing::info!(
            completed = completed,
            total = handles.len(),
            "Pipeline shutdown complete"
        );
    }

    /// Check if shutdown has been requested
    pub fn is_shutdown_requested(&self) -> bool {
        self.shutdown_signal.load(Ordering::Relaxed)
    }
}

/// Individual worker in the pipeline
struct PipelineWorker<P: EventProcessor> {
    worker_id: usize,
    filter: SharedFilter,
    processor: Arc<P>,
    stats: Arc<PipelineStats>,
    shutdown_signal: Arc<AtomicBool>,
    event_rx: Arc<RwLock<mpsc::Receiver<JetstreamEvent>>>,
    max_in_flight: usize,
}

impl<P: EventProcessor + 'static> PipelineWorker<P> {
    async fn run(self) {
        tracing::trace!(worker_id = self.worker_id, "Worker started");

        let mut in_flight = 0usize;

        loop {
            if self.shutdown_signal.load(Ordering::Relaxed) {
                if in_flight > 0 {
                    tracing::trace!(
                        worker_id = self.worker_id,
                        in_flight = in_flight,
                        "Draining events before shutdown"
                    );
                    tokio::task::yield_now().await;
                    continue;
                }
                break;
            }

            if in_flight >= self.max_in_flight {
                tokio::task::yield_now().await;
                continue;
            }

            let event = {
                let mut rx = self.event_rx.write().await;
                match rx.recv().await {
                    Some(event) => event,
                    None => {
                        tracing::trace!(worker_id = self.worker_id, "Event channel closed");
                        break;
                    }
                }
            };

            self.stats.events_received.fetch_add(1, Ordering::Relaxed);

            let filtered = match self.filter.filter(event) {
                Some(f) => f,
                None => continue,
            };

            self.stats.events_filtered.fetch_add(1, Ordering::Relaxed);
            in_flight += 1;
            self.stats.events_in_flight.fetch_add(1, Ordering::Relaxed);

            let processed = match self.processor.process(filtered).await {
                Ok(p) => p,
                Err(e) => {
                    tracing::error!(worker_id = self.worker_id, error = %e, "Event processing failed");
                    self.stats.events_failed.fetch_add(1, Ordering::Relaxed);
                    in_flight -= 1;
                    self.stats.events_in_flight.fetch_sub(1, Ordering::Relaxed);
                    continue;
                }
            };

            if processed.success {
                if processed.event.is_acknowledged() {
                    self.stats.events_processed.fetch_add(1, Ordering::Relaxed);
                    tracing::trace!(worker_id = self.worker_id, "Event processed and acknowledged");
                } else {
                    self.stats.events_failed.fetch_add(1, Ordering::Relaxed);
                    tracing::warn!(
                        worker_id = self.worker_id,
                        cursor = processed.event.cursor(),
                        "Event processing succeeded but was not acknowledged"
                    );
                }
            } else {
                self.stats.events_failed.fetch_add(1, Ordering::Relaxed);
                tracing::warn!(
                    worker_id = self.worker_id,
                    error = ?processed.error,
                    "Event processing reported failure"
                );
            }

            in_flight -= 1;
            self.stats.events_in_flight.fetch_sub(1, Ordering::Relaxed);
        }

        tracing::trace!(worker_id = self.worker_id, "Worker stopped");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jetstream::filter::EventFilter;
    use crate::jetstream::types::{CommitData, CommitOperation};

    struct TestProcessor;

    #[async_trait::async_trait]
    impl EventProcessor for TestProcessor {
        async fn process(
            &self, mut event: FilteredEvent,
        ) -> Result<ProcessedEvent, Box<dyn std::error::Error + Send + Sync>> {
            event.acknowledge();

            Ok(ProcessedEvent { event, success: true, error: None })
        }
    }

    fn create_test_event(mentions_bot: bool, bot_did: &str) -> JetstreamEvent {
        let record = if mentions_bot {
            serde_json::json!({
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
            })
        } else {
            serde_json::json!({"text": "Just a post"})
        };

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

    #[tokio::test]
    async fn test_pipeline_processes_mentions() {
        let bot_did = "did:plc:bot123";
        let filter = SharedFilter::new(EventFilter::new(bot_did));
        let config = PipelineConfig { num_workers: 1, channel_buffer_size: 10, max_in_flight: 5 };
        let pipeline = EventPipeline::new(config, filter, TestProcessor);
        pipeline.start().await;

        let sender = pipeline.event_sender();

        let event = create_test_event(true, bot_did);
        sender.send(event).await.unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let stats = pipeline.stats();
        assert_eq!(stats.events_received, 1);
        assert_eq!(stats.events_filtered, 1);
        assert_eq!(stats.events_processed, 1);

        pipeline.shutdown(5).await;
    }

    #[tokio::test]
    async fn test_pipeline_filters_non_mentions() {
        let bot_did = "did:plc:bot123";
        let filter = SharedFilter::new(EventFilter::new(bot_did));
        let config = PipelineConfig::default();
        let pipeline = EventPipeline::new(config, filter, TestProcessor);
        pipeline.start().await;

        let sender = pipeline.event_sender();
        let event = create_test_event(false, bot_did);
        sender.send(event).await.unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let stats = pipeline.stats();
        assert_eq!(stats.events_received, 1);
        assert_eq!(stats.events_filtered, 0);
        assert_eq!(stats.events_processed, 0);

        pipeline.shutdown(5).await;
    }

    #[tokio::test]
    async fn test_pipeline_graceful_shutdown() {
        let bot_did = "did:plc:bot123";
        let filter = SharedFilter::new(EventFilter::new(bot_did));
        let config = PipelineConfig::default();

        let pipeline = EventPipeline::new(config, filter, TestProcessor);
        pipeline.start().await;

        let sender = pipeline.event_sender();
        for _ in 0..5 {
            let event = create_test_event(true, bot_did);
            sender.send(event).await.unwrap();
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        pipeline.shutdown(5).await;

        let stats = pipeline.stats();
        assert_eq!(stats.events_received, 5);
        assert!(pipeline.is_shutdown_requested());
    }
}
