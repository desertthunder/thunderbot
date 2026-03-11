use crate::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

#[derive(Debug, Serialize)]
struct HealthContext {
    status: &'static str,
    paused: bool,
    queue_depth: usize,
    last_jetstream_event_us: i64,
    uptime_seconds: u64,
    processed_events: u64,
    failed_events: u64,
}

pub(crate) fn public_routes() -> Router<AppState> {
    Router::new().route("/health", get(health))
}

async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let context = HealthContext::from_state(&state);
    health_view(context)
}

impl HealthContext {
    fn from_state(state: &AppState) -> Self {
        let degraded = state.runtime.events_failed() > 0 && state.runtime.events_in_flight() > 40;

        Self {
            status: if degraded { "degraded" } else { "ok" },
            paused: state.runtime.is_paused(),
            queue_depth: state.runtime.events_in_flight(),
            last_jetstream_event_us: state.runtime.last_jetstream_event_us(),
            uptime_seconds: state.runtime.started_at().elapsed().as_secs(),
            processed_events: state.runtime.events_processed(),
            failed_events: state.runtime.events_failed(),
        }
    }
}

fn health_view(context: HealthContext) -> impl IntoResponse {
    let status = if context.status == "degraded" { StatusCode::SERVICE_UNAVAILABLE } else { StatusCode::OK };

    (status, Json(context))
}
