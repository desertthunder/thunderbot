use crate::bsky::BskyClient;
use crate::db::DatabaseRepository;
use crate::health::{HealthRegistry, JetstreamState};
use crate::web::handlers::get_metrics;
use crate::web::handlers::{
    WebAppState, get_activity_timeline, get_admin, get_chat, get_config, get_dashboard, get_export_csv,
    get_export_json, get_filtered_threads, get_health, get_identities, get_landing, get_login, get_search, get_status,
    get_thread_detail, get_threads, post_bulk_delete, post_chat_send, post_cleanup_old, post_clear_thread, post_login,
    post_logout, post_mute_author, post_pause, post_post, post_resume, post_save_preset, post_search,
    post_unmute_author,
};

use anyhow::Result;
use axum::Router;
use axum::routing::{get, post};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

pub struct Server {
    app_state: WebAppState,
    address: String,
    port: u16,
    dry_run: bool,
}

impl Server {
    pub fn new(db: Arc<dyn DatabaseRepository>, bsky_client: Arc<BskyClient>, health: Arc<HealthRegistry>) -> Self {
        let jetstream_state = Arc::new(tokio::sync::RwLock::new(JetstreamState::new()));
        Self {
            app_state: WebAppState { db, bsky_client, health, jetstream_state },
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

    pub fn build_router(&self) -> Router {
        Router::new()
            .route("/", get(get_landing))
            .route("/dashboard", get(get_dashboard))
            .route("/login", get(get_login))
            .route("/logout", post(post_logout))
            .route("/chat", get(get_chat))
            .route("/threads", get(get_threads))
            .route("/thread/:thread_id", get(get_thread_detail))
            .route("/identities", get(get_identities))
            .route("/admin", get(get_admin))
            .route("/config", get(get_config))
            .route("/search", get(get_search))
            .route("/api/search", post(post_search))
            .route("/api/export/conversations.json", get(get_export_json))
            .route("/api/export/conversations.csv", get(get_export_csv))
            .route("/api/bulk/delete", post(post_bulk_delete))
            .route("/api/cleanup/old", post(post_cleanup_old))
            .route("/api/filter/mute", post(post_mute_author))
            .route("/api/filter/unmute", post(post_unmute_author))
            .route("/api/filter/preset/save", post(post_save_preset))
            .route("/threads/filtered", get(get_filtered_threads))
            .route("/activity", get(get_activity_timeline))
            .route("/api/status", get(get_status))
            .route("/api/health", get(get_health))
            .route("/api/metrics", get(get_metrics))
            .route("/api/post", post(post_post))
            .route("/api/pause", post(post_pause))
            .route("/api/resume", post(post_resume))
            .route("/api/clear-thread", post(post_clear_thread))
            .route("/api/login", post(post_login))
            .route("/api/chat/send", post(post_chat_send))
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
