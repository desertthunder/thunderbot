use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Pass,
    Warn,
    Fail,
}

impl HealthStatus {
    pub fn is_healthy(&self) -> bool {
        matches!(self, Self::Pass | Self::Warn)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ComponentHealth {
    pub component_id: String,
    pub status: HealthStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
    pub observed_unit: String,
    pub observed_value: i64,
}

impl ComponentHealth {
    pub fn healthy(component_id: impl Into<String>) -> Self {
        Self {
            component_id: component_id.into(),
            status: HealthStatus::Pass,
            output: None,
            error: None,
            time: Some(Utc::now().to_rfc3339()),
            observed_unit: "ms".to_string(),
            observed_value: 0,
        }
    }

    pub fn degraded(component_id: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            component_id: component_id.into(),
            status: HealthStatus::Warn,
            output: Some(reason.into()),
            error: None,
            time: Some(Utc::now().to_rfc3339()),
            observed_unit: "ms".to_string(),
            observed_value: 0,
        }
    }

    pub fn unhealthy(component_id: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            component_id: component_id.into(),
            status: HealthStatus::Fail,
            output: None,
            error: Some(error.into()),
            time: Some(Utc::now().to_rfc3339()),
            observed_unit: "ms".to_string(),
            observed_value: 0,
        }
    }

    pub fn with_latency(mut self, latency_ms: u64) -> Self {
        self.observed_value = latency_ms as i64;
        self
    }

    pub fn with_output(mut self, output: impl Into<String>) -> Self {
        self.output = Some(output.into());
        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HealthReport {
    pub status: HealthStatus,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_id: Option<String>,
    pub notes: Vec<String>,
    pub output: String,
    pub checks: HashMap<String, ComponentHealth>,
    pub details: HashMap<String, serde_json::Value>,
}

impl HealthReport {
    pub fn new(version: String) -> Self {
        let checks = HashMap::new();
        let overall_status = Self::compute_overall_status(&checks);
        Self {
            status: overall_status,
            version,
            release_id: None,
            service_id: None,
            notes: Vec::new(),
            output: String::new(),
            checks,
            details: HashMap::new(),
        }
    }

    fn compute_overall_status(checks: &HashMap<String, ComponentHealth>) -> HealthStatus {
        if checks.is_empty() {
            return HealthStatus::Pass;
        }

        let has_fail = checks.values().any(|c| c.status == HealthStatus::Fail);
        if has_fail {
            return HealthStatus::Fail;
        }

        let has_warn = checks.values().any(|c| c.status == HealthStatus::Warn);
        if has_warn {
            return HealthStatus::Warn;
        }

        HealthStatus::Pass
    }

    pub fn with_check(mut self, component_id: impl Into<String>, health: ComponentHealth) -> Self {
        let id = component_id.into();
        self.checks.insert(id, health);
        self.status = Self::compute_overall_status(&self.checks);
        self
    }

    pub fn with_service_id(mut self, service_id: impl Into<String>) -> Self {
        self.service_id = Some(service_id.into());
        self
    }

    pub fn with_notes(mut self, notes: Vec<String>) -> Self {
        self.notes = notes;
        self
    }

    pub fn with_details(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.details.insert(key.into(), value);
        self
    }

    pub fn http_status(&self) -> u16 {
        match self.status {
            HealthStatus::Pass => 200,
            HealthStatus::Warn | HealthStatus::Fail => 503,
        }
    }
}

#[derive(Clone)]
pub struct HealthRegistry {
    checks: Arc<RwLock<HashMap<String, ComponentHealth>>>,
}

impl HealthRegistry {
    pub fn new() -> Self {
        Self { checks: Arc::new(RwLock::new(HashMap::new())) }
    }

    pub async fn update_component(&self, component_id: impl Into<String>, health: ComponentHealth) {
        let mut checks = self.checks.write().await;
        checks.insert(component_id.into(), health);
    }

    pub async fn get_component(&self, component_id: &str) -> Option<ComponentHealth> {
        let checks = self.checks.read().await;
        checks.get(component_id).cloned()
    }

    pub async fn all_components(&self) -> Vec<ComponentHealth> {
        let checks = self.checks.read().await;
        checks.values().cloned().collect()
    }

    pub async fn generate_report(&self, version: String) -> HealthReport {
        let checks = self.checks.read().await;
        let mut report = HealthReport::new(version);
        for (id, health) in checks.iter() {
            report = report.with_check(id.clone(), health.clone());
        }
        report
    }
}

impl Default for HealthRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
pub trait HealthCheck: Send + Sync {
    async fn check(&self) -> Result<ComponentHealth>;
}

pub struct DatabaseHealthCheck {
    db: Arc<dyn crate::DatabaseRepository>,
}

impl DatabaseHealthCheck {
    pub fn new(db: Arc<dyn crate::DatabaseRepository>) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl HealthCheck for DatabaseHealthCheck {
    async fn check(&self) -> Result<ComponentHealth> {
        let start = std::time::Instant::now();
        let result = self.db.ping().await;
        let latency = start.elapsed().as_millis() as u64;

        match result {
            Ok(_) => Ok(ComponentHealth::healthy("database").with_latency(latency)),
            Err(e) => Ok(ComponentHealth::unhealthy("database", e.to_string()).with_latency(latency)),
        }
    }
}

pub struct JetstreamHealthCheck {
    state: Arc<RwLock<JetstreamState>>,
}

#[derive(Clone, Debug, Default)]
pub struct JetstreamState {
    pub connected: bool,
    pub last_event: Option<chrono::DateTime<Utc>>,
    pub reconnect_count: u32,
    /// Current queue depth (events awaiting processing)
    pub queue_depth: usize,
    /// Events processed per second (rolling average)
    pub events_per_second: f64,
    /// Whether event processing is paused
    pub is_paused: bool,
    /// Threshold for queue depth alert
    pub backlog_alert_threshold: usize,
}

impl JetstreamState {
    pub fn new() -> Self {
        Self { backlog_alert_threshold: 1000, ..Default::default() }
    }

    pub fn set_connected(&mut self, connected: bool) {
        self.connected = connected;
        if connected {
            self.last_event = Some(Utc::now());
        }
    }

    pub fn record_event(&mut self) {
        self.last_event = Some(Utc::now());
    }

    pub fn increment_reconnects(&mut self) {
        self.reconnect_count += 1;
    }

    pub fn time_since_last_event(&self) -> Option<chrono::Duration> {
        self.last_event.map(|t| Utc::now() - t)
    }

    /// Check if queue is at backlog alert threshold.
    pub fn is_backlogged(&self) -> bool {
        self.queue_depth >= self.backlog_alert_threshold
    }

    /// Update queue depth.
    pub fn set_queue_depth(&mut self, depth: usize) {
        self.queue_depth = depth;
    }

    /// Update events per second metric.
    pub fn set_events_per_second(&mut self, eps: f64) {
        self.events_per_second = eps;
    }

    /// Pause or resume event processing.
    pub fn set_paused(&mut self, paused: bool) {
        self.is_paused = paused;
    }
}

impl JetstreamHealthCheck {
    pub fn new(state: Arc<RwLock<JetstreamState>>) -> Self {
        Self { state }
    }
}

#[async_trait::async_trait]
impl HealthCheck for JetstreamHealthCheck {
    async fn check(&self) -> Result<ComponentHealth> {
        let state = self.state.read().await;
        let output = if state.connected {
            format!("Connected ({} reconnects)", state.reconnect_count)
        } else {
            "Disconnected".to_string()
        };

        if state.connected {
            Ok(ComponentHealth::healthy("jetstream").with_output(output))
        } else {
            Ok(ComponentHealth::unhealthy("jetstream", "Not connected"))
        }
    }
}

pub struct BlueskyHealthCheck {
    client: Arc<crate::BskyClient>,
}

impl BlueskyHealthCheck {
    pub fn new(client: Arc<crate::BskyClient>) -> Self {
        Self { client }
    }
}

#[async_trait::async_trait]
impl HealthCheck for BlueskyHealthCheck {
    async fn check(&self) -> Result<ComponentHealth> {
        let session = self.client.get_session().await;
        match session {
            Some(_) => Ok(ComponentHealth::healthy("bluesky").with_output("Session valid")),
            None => Ok(ComponentHealth::unhealthy("bluesky", "No active session")),
        }
    }
}

pub struct GeminiHealthCheck {
    api_key: String,
    last_call: Arc<RwLock<Option<chrono::DateTime<Utc>>>>,
}

impl GeminiHealthCheck {
    pub fn new(api_key: String, last_call: Arc<RwLock<Option<chrono::DateTime<Utc>>>>) -> Self {
        Self { api_key, last_call }
    }
}

#[async_trait::async_trait]
impl HealthCheck for GeminiHealthCheck {
    async fn check(&self) -> Result<ComponentHealth> {
        if self.api_key.is_empty() {
            return Ok(ComponentHealth::unhealthy("gemini", "No API key configured"));
        }

        let last_call = self.last_call.read().await;
        if let Some(call_time) = *last_call {
            Ok(ComponentHealth::healthy("gemini").with_output(format!(
                "Last successful call: {}",
                call_time.format("%Y-%m-%d %H:%M:%S UTC")
            )))
        } else {
            Ok(ComponentHealth::degraded("gemini", "No successful calls yet"))
        }
    }
}
