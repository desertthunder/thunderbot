use crate::bsky::BskyClient;
use crate::db::DatabaseRepository;
use crate::web::handlers::{
    WebAppState, get_admin, get_chat, get_config, get_dashboard, get_identities, get_landing, get_login, get_status,
    get_thread_detail, get_threads, post_chat_send, post_clear_thread, post_login, post_logout, post_pause, post_post,
    post_resume,
};
use anyhow::Result;
use axum::Router;
use axum::routing::{get, post};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

pub struct Server {
    app_state: WebAppState,
    address: String,
    port: u16,
}

impl Server {
    pub fn new(db: Arc<dyn DatabaseRepository>, bsky_client: Arc<BskyClient>) -> Self {
        Self { app_state: WebAppState { db, bsky_client }, address: "127.0.0.1".to_string(), port: 3000 }
    }

    pub fn with_address(mut self, address: impl Into<String>) -> Self {
        self.address = address.into();
        self
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
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
            .route("/api/status", get(get_status))
            .route("/api/post", post(post_post))
            .route("/api/pause", post(post_pause))
            .route("/api/resume", post(post_resume))
            .route("/api/clear-thread", post(post_clear_thread))
            .route("/api/login", post(post_login))
            .route("/api/chat/send", post(post_chat_send))
            .layer(CorsLayer::permissive())
            .layer(TraceLayer::new_for_http())
            .with_state(self.app_state.clone())
    }

    pub async fn serve(self) -> Result<()> {
        let app = self.build_router();
        let addr = format!("{}:{}", self.address, self.port);
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        tracing::info!("Web server listening on http://{}", addr);

        axum::serve(listener, app).await?;

        Ok(())
    }
}
