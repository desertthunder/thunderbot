use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;

#[derive(Debug)]
pub struct RuntimeState {
    started_at: Instant,
    paused: AtomicBool,
    last_jetstream_event_us: AtomicI64,
    events_in_flight: AtomicUsize,
    events_processed: AtomicU64,
    events_failed: AtomicU64,
    last_model_latency_ms: AtomicU64,
}

impl Default for RuntimeState {
    fn default() -> Self {
        Self {
            started_at: Instant::now(),
            paused: AtomicBool::new(false),
            last_jetstream_event_us: AtomicI64::new(0),
            events_in_flight: AtomicUsize::new(0),
            events_processed: AtomicU64::new(0),
            events_failed: AtomicU64::new(0),
            last_model_latency_ms: AtomicU64::new(0),
        }
    }
}

impl RuntimeState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn started_at(&self) -> Instant {
        self.started_at
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
    }

    pub fn record_jetstream_event(&self, time_us: i64) {
        self.last_jetstream_event_us.store(time_us, Ordering::Relaxed);
    }

    pub fn last_jetstream_event_us(&self) -> i64 {
        self.last_jetstream_event_us.load(Ordering::Relaxed)
    }

    pub fn begin_processing(&self) {
        self.events_in_flight.fetch_add(1, Ordering::Relaxed);
    }

    pub fn finish_processing(&self, success: bool, latency_ms: Option<u64>) {
        self.events_in_flight.fetch_sub(1, Ordering::Relaxed);

        if success {
            self.events_processed.fetch_add(1, Ordering::Relaxed);
            if let Some(ms) = latency_ms {
                self.last_model_latency_ms.store(ms, Ordering::Relaxed);
            }
        } else {
            self.events_failed.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn events_in_flight(&self) -> usize {
        self.events_in_flight.load(Ordering::Relaxed)
    }

    pub fn events_processed(&self) -> u64 {
        self.events_processed.load(Ordering::Relaxed)
    }

    pub fn events_failed(&self) -> u64 {
        self.events_failed.load(Ordering::Relaxed)
    }

    pub fn last_model_latency_ms(&self) -> u64 {
        self.last_model_latency_ms.load(Ordering::Relaxed)
    }
}

pub type SharedRuntimeState = Arc<RuntimeState>;

pub fn new_shared_runtime() -> SharedRuntimeState {
    Arc::new(RuntimeState::new())
}
