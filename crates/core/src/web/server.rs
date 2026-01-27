use crate::bsky::BskyClient;
use crate::control::{PolicyEnforcer, SessionManager, StatusBroadcaster};
use crate::db::DatabaseRepository;
use crate::health::{HealthRegistry, JetstreamState};
use crate::web::{controls, handlers};

use anyhow::Result;
use axum::Router;
use axum::routing::{get, post};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

pub struct Server {
    app_state: handlers::WebAppState,
    address: String,
    port: u16,
    dry_run: bool,
}

impl Server {
    pub fn new(
        db: Arc<dyn DatabaseRepository>, bsky_client: Arc<BskyClient>, agent: Arc<crate::Agent>,
        health: Arc<HealthRegistry>,
    ) -> Self {
        let jetstream_state = Arc::new(tokio::sync::RwLock::new(JetstreamState::new()));
        let session_manager = Arc::new(SessionManager::new(bsky_client.clone(), db.clone()));
        let policy_enforcer = Arc::new(PolicyEnforcer::new(db.clone()));
        let broadcaster = Arc::new(StatusBroadcaster::new(bsky_client.clone()));
        let event_sender = None;

        Self {
            app_state: handlers::WebAppState {
                db,
                bsky_client,
                health,
                jetstream_state,
                session_manager,
                policy_enforcer,
                broadcaster,
                event_sender,
                agent,
            },
            address: "127.0.0.1".to_string(),
            port: 3000,
            dry_run: false,
        }
    }

    pub fn with_address(mut self, address: impl Into<String>) -> Self {
        self.address = address.into();
        self
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    pub fn with_dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    pub fn is_dry_run(&self) -> bool {
        self.dry_run
    }

    /// Set the event sender for DLQ retry functionality.
    /// Should be called when jetstream listener starts.
    pub fn set_event_sender(&mut self, sender: tokio::sync::mpsc::Sender<crate::jetstream::event::JetstreamEvent>) {
        self.app_state.event_sender = Some(sender);
    }

    /// Get the jetstream state for monitoring and metrics.
    pub fn get_jetstream_state(&self) -> Arc<tokio::sync::RwLock<JetstreamState>> {
        self.app_state.jetstream_state.clone()
    }

    /// Get references to control components for use in other parts of the application.
    pub fn get_control_components(&self) -> (Arc<SessionManager>, Arc<PolicyEnforcer>, Arc<StatusBroadcaster>) {
        (
            self.app_state.session_manager.clone(),
            self.app_state.policy_enforcer.clone(),
            self.app_state.broadcaster.clone(),
        )
    }

    pub fn build_router(&self) -> Router {
        Router::new()
            .route("/", get(handlers::get_landing))
            .route("/dashboard", get(handlers::get_dashboard))
            .route("/login", get(handlers::get_login))
            .route("/logout", post(handlers::post_logout))
            .route("/chat", get(handlers::get_chat))
            .route("/threads", get(handlers::get_threads))
            .route("/thread/:thread_id", get(handlers::get_thread_detail))
            .route("/identities", get(handlers::get_identities))
            .route("/admin", get(handlers::get_admin))
            .route("/config", get(handlers::get_config))
            .route("/search", get(handlers::get_search))
            .route("/api/search", post(handlers::post_search))
            .route("/api/export/conversations.json", get(handlers::get_export_json))
            .route("/api/export/conversations.csv", get(handlers::get_export_csv))
            .route("/api/bulk/delete", post(handlers::post_bulk_delete))
            .route("/api/cleanup/old", post(handlers::post_cleanup_old))
            .route("/api/filter/mute", post(handlers::post_mute_author))
            .route("/api/filter/unmute", post(handlers::post_unmute_author))
            .route("/api/filter/preset/save", post(handlers::post_save_preset))
            .route("/threads/filtered", get(handlers::get_filtered_threads))
            .route("/activity", get(handlers::get_activity_timeline))
            .route("/api/status", get(handlers::get_status))
            .route("/api/health", get(handlers::get_health))
            .route("/api/metrics", get(handlers::get_metrics))
            .route("/api/post", post(handlers::post_post))
            .route("/api/pause", post(handlers::post_pause))
            .route("/api/resume", post(handlers::post_resume))
            .route("/api/clear-thread", post(handlers::post_clear_thread))
            .route("/api/login", post(handlers::post_login))
            .route("/api/chat/send", post(handlers::post_chat_send))
            .route("/controls", get(controls::get_controls_landing))
            .route("/controls/rate-limits", get(controls::get_rate_limits))
            .route("/api/rate-limit-history", get(controls::get_rate_limit_history_data))
            .route("/controls/event-queue", get(controls::get_event_queue_status))
            .route("/api/pause-events", post(controls::post_pause_events))
            .route("/api/resume-events", post(controls::post_resume_events))
            .route("/controls/session", get(controls::get_session_info))
            .route("/api/session/refresh", post(controls::post_refresh_session))
            .route("/controls/preview", get(controls::get_pending_responses))
            .route("/api/preview/approve", post(controls::post_approve_response))
            .route("/api/preview/edit", post(controls::post_edit_response))
            .route("/api/preview/discard", post(controls::post_discard_response))
            .route("/api/preview/toggle", post(controls::post_toggle_preview_mode))
            .route("/controls/quiet-hours", get(controls::get_quiet_hours))
            .route("/api/quiet-hours/save", post(controls::post_save_quiet_hours))
            .route("/api/quiet-hours/delete", post(controls::post_delete_quiet_hours))
            .route("/controls/reply-limits", get(controls::get_reply_limits))
            .route("/api/reply-limits/update", post(controls::post_update_reply_limits))
            .route("/controls/blocklist", get(controls::get_blocklist))
            .route("/api/blocklist/add", post(controls::post_block_author))
            .route("/api/blocklist/remove", post(controls::post_unblock_author))
            .route("/api/blocklist/export", get(controls::get_export_blocklist))
            .route("/api/blocklist/import", post(controls::post_import_blocklist))
            .route("/controls/status-broadcast", get(controls::get_status_broadcast))
            .route("/api/status/post", post(controls::post_status_update))
            .route("/api/status/bio", post(controls::post_update_bio))
            .route("/api/status/maintenance", post(controls::post_maintenance_announcement))
            .route("/controls/dlq", get(controls::get_dead_letter_queue))
            .route("/api/dlq/retry", post(controls::post_retry_dlq_item))
            .route("/api/dlq/bulk-retry", post(controls::post_bulk_retry_dlq))
            .route("/api/dlq/purge", post(controls::post_purge_dlq))
            .nest_service("/static", ServeDir::new("crates/core/src/web/static"))
            .layer(CorsLayer::permissive())
            .layer(TraceLayer::new_for_http())
            .with_state(self.app_state.clone())
    }

    pub async fn serve(self) -> Result<()> {
        let app = self.build_router();
        let addr = format!("{}:{}", self.address, self.port);
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        tracing::info!("Web server listening on http://{}", addr);

        let shutdown_signal = async {
            let ctrl_c = async {
                tokio::signal::ctrl_c().await.ok();
                tracing::info!("Received Ctrl+C, initiating graceful shutdown");
            };

            #[cfg(unix)]
            let terminate = async {
                match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
                    Ok(mut signal) => {
                        signal.recv().await;
                        tracing::info!("Received SIGTERM, initiating graceful shutdown");
                    }
                    Err(e) => {
                        tracing::warn!("Failed to setup SIGTERM handler: {}", e);
                    }
                }
            };

            #[cfg(unix)]
            tokio::select! {
                _ = ctrl_c => {},
                _ = terminate => {},
            }

            #[cfg(not(unix))]
            ctrl_c.await;
        };

        let graceful = axum::serve(listener, app).with_graceful_shutdown(shutdown_signal);

        tracing::info!("Graceful shutdown complete");
        graceful.await?;
        Ok(())
    }
}
