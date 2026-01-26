use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct SlidingWindow {
    samples: Arc<RwLock<Vec<f64>>>,
    max_size: usize,
}

impl SlidingWindow {
    pub fn new(max_size: usize) -> Self {
        Self { samples: Arc::new(RwLock::new(Vec::with_capacity(max_size))), max_size }
    }

    pub async fn record(&self, value: f64) {
        let mut samples = self.samples.write().await;
        samples.push(value);
        if samples.len() > self.max_size {
            samples.remove(0);
        }
    }

    pub async fn quantile(&self, percentile: f64) -> f64 {
        let samples = self.samples.read().await;
        if samples.is_empty() {
            return 0.0;
        }

        let mut sorted: Vec<f64> = samples.iter().cloned().collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let index = ((sorted.len() - 1) as f64 * percentile / 100.0) as usize;
        sorted[index]
    }

    pub async fn count(&self) -> usize {
        let samples = self.samples.read().await;
        samples.len()
    }

    pub async fn sum(&self) -> f64 {
        let samples = self.samples.read().await;
        samples.iter().sum()
    }

    pub async fn average(&self) -> f64 {
        let count = self.count().await;
        if count == 0 {
            return 0.0;
        }
        self.sum().await / count as f64
    }
}

#[derive(Clone)]
pub struct Metrics {
    events_received: Arc<AtomicU64>,
    events_processed: Arc<AtomicU64>,
    events_failed: Arc<AtomicU64>,
    processing_lag_samples: Arc<SlidingWindow>,
    gemini_requests_success: Arc<AtomicU64>,
    gemini_requests_error: Arc<AtomicU64>,
    gemini_latency_samples: Arc<SlidingWindow>,
    bluesky_posts_reply: Arc<AtomicU64>,
    bluesky_posts_original: Arc<AtomicU64>,
    bluesky_rate_limit_remaining: Arc<AtomicU64>,
    start_time: Instant,
    jetstream_state: Arc<AtomicU64>,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            events_received: Arc::new(AtomicU64::new(0)),
            events_processed: Arc::new(AtomicU64::new(0)),
            events_failed: Arc::new(AtomicU64::new(0)),
            processing_lag_samples: Arc::new(SlidingWindow::new(300)),
            gemini_requests_success: Arc::new(AtomicU64::new(0)),
            gemini_requests_error: Arc::new(AtomicU64::new(0)),
            gemini_latency_samples: Arc::new(SlidingWindow::new(300)),
            bluesky_posts_reply: Arc::new(AtomicU64::new(0)),
            bluesky_posts_original: Arc::new(AtomicU64::new(0)),
            bluesky_rate_limit_remaining: Arc::new(AtomicU64::new(u64::MAX)),
            start_time: Instant::now(),
            jetstream_state: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn events_received(&self) -> u64 {
        self.events_received.load(Ordering::Relaxed)
    }

    pub fn increment_events_received(&self) {
        self.events_received.fetch_add(1, Ordering::Relaxed);
    }

    pub fn events_processed(&self) -> u64 {
        self.events_processed.load(Ordering::Relaxed)
    }

    pub fn increment_events_processed(&self) {
        self.events_processed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn events_failed(&self) -> u64 {
        self.events_failed.load(Ordering::Relaxed)
    }

    pub fn increment_events_failed(&self) {
        self.events_failed.fetch_add(1, Ordering::Relaxed);
    }

    pub async fn record_processing_lag(&self, lag_ms: u64) {
        self.processing_lag_samples.record(lag_ms as f64).await;
    }

    pub fn gemini_requests_success(&self) -> u64 {
        self.gemini_requests_success.load(Ordering::Relaxed)
    }

    pub fn increment_gemini_success(&self) {
        self.gemini_requests_success.fetch_add(1, Ordering::Relaxed);
    }

    pub fn gemini_requests_error(&self) -> u64 {
        self.gemini_requests_error.load(Ordering::Relaxed)
    }

    pub fn increment_gemini_error(&self) {
        self.gemini_requests_error.fetch_add(1, Ordering::Relaxed);
    }

    pub async fn record_gemini_latency(&self, latency_ms: u64) {
        self.gemini_latency_samples.record(latency_ms as f64).await;
    }

    pub async fn gemini_latency_quantiles(&self) -> (f64, f64, f64) {
        let p50 = self.gemini_latency_samples.quantile(50.0).await;
        let p90 = self.gemini_latency_samples.quantile(90.0).await;
        let p99 = self.gemini_latency_samples.quantile(99.0).await;
        (p50, p90, p99)
    }

    pub async fn gemini_latency_sum(&self) -> f64 {
        self.gemini_latency_samples.sum().await
    }

    pub async fn gemini_latency_count(&self) -> usize {
        self.gemini_latency_samples.count().await
    }

    pub fn bluesky_posts_reply(&self) -> u64 {
        self.bluesky_posts_reply.load(Ordering::Relaxed)
    }

    pub fn increment_bluesky_reply(&self) {
        self.bluesky_posts_reply.fetch_add(1, Ordering::Relaxed);
    }

    pub fn bluesky_posts_original(&self) -> u64 {
        self.bluesky_posts_original.load(Ordering::Relaxed)
    }

    pub fn increment_bluesky_original(&self) {
        self.bluesky_posts_original.fetch_add(1, Ordering::Relaxed);
    }

    pub fn bluesky_rate_limit_remaining(&self) -> u64 {
        self.bluesky_rate_limit_remaining.load(Ordering::Relaxed)
    }

    pub fn set_rate_limit_remaining(&self, remaining: u64) {
        self.bluesky_rate_limit_remaining.store(remaining, Ordering::Relaxed);
    }

    pub fn uptime(&self) -> Duration {
        self.start_time.elapsed()
    }

    pub fn set_jetstream_state(&self, state: u8) {
        self.jetstream_state.store(state as u64, Ordering::Relaxed);
    }

    pub fn jetstream_state(&self) -> u8 {
        self.jetstream_state.load(Ordering::Relaxed) as u8
    }

    pub async fn render_prometheus(&self, version: &str) -> String {
        let uptime_secs = self.uptime().as_secs_f64();
        let processing_lag_secs = self.processing_lag_samples.average().await / 1000.0;
        let (p50, p90, p99) = self.gemini_latency_quantiles().await;
        let latency_sum = self.gemini_latency_sum().await / 1000.0;
        let latency_count = self.gemini_latency_count().await;

        let metrics = format!(
            r#"# HELP thunderbot_version ThunderBot version info
# TYPE thunderbot_version gauge
thunderbot_version{{version="{version}"}} 1

# HELP thunderbot_events_received_total Total Jetstream events received
# TYPE thunderbot_events_received_total counter
thunderbot_events_received_total {}

# HELP thunderbot_events_processed_total Events successfully processed
# TYPE thunderbot_events_processed_total counter
thunderbot_events_processed_total {}

# HELP thunderbot_events_failed_total Events that failed processing
# TYPE thunderbot_events_failed_total counter
thunderbot_events_failed_total {}

# HELP thunderbot_processing_lag_seconds Time from event receipt to completion
# TYPE thunderbot_processing_lag_seconds gauge
thunderbot_processing_lag_seconds {}

# HELP thunderbot_gemini_requests_total Total Gemini API calls
# TYPE thunderbot_gemini_requests_total counter
thunderbot_gemini_requests_total{{status="success"}} {}
thunderbot_gemini_requests_total{{status="error"}} {}

# HELP thunderbot_gemini_latency_seconds Gemini API response time
# TYPE thunderbot_gemini_latency_seconds summary
thunderbot_gemini_latency_seconds{{quantile="0.5"}} {}
thunderbot_gemini_latency_seconds{{quantile="0.9"}} {}
thunderbot_gemini_latency_seconds{{quantile="0.99"}} {}
thunderbot_gemini_latency_seconds_sum {}
thunderbot_gemini_latency_seconds_count {}

# HELP thunderbot_bluesky_posts_total Posts created on Bluesky
# TYPE thunderbot_bluesky_posts_total counter
thunderbot_bluesky_posts_total{{type="reply"}} {}
thunderbot_bluesky_posts_total{{type="post"}} {}

# HELP thunderbot_bluesky_rate_limit_remaining Remaining API calls in current window
# TYPE thunderbot_bluesky_rate_limit_remaining gauge
thunderbot_bluesky_rate_limit_remaining {}

# HELP thunderbot_uptime_seconds Seconds since server start
# TYPE thunderbot_uptime_seconds gauge
thunderbot_uptime_seconds {}

# HELP thunderbot_jetstream_state WebSocket connection state (0=disconnected, 1=reconnecting, 2=connected)
# TYPE thunderbot_jetstream_state gauge
thunderbot_jetstream_state {}
"#,
            self.events_received(),
            self.events_processed(),
            self.events_failed(),
            processing_lag_secs,
            self.gemini_requests_success(),
            self.gemini_requests_error(),
            p50,
            p90,
            p99,
            latency_sum,
            latency_count,
            self.bluesky_posts_reply(),
            self.bluesky_posts_original(),
            self.bluesky_rate_limit_remaining(),
            uptime_secs,
            self.jetstream_state(),
        );

        metrics
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}
