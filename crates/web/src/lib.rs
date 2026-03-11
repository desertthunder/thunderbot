mod formatters;
mod pages;
mod partials;
mod views;

pub mod runtime;

use anyhow::Context;
use axum::Router;
use axum::http::header;
use axum::middleware;
use axum::response::{IntoResponse, Redirect};
use axum::routing::get;
use chrono::Utc;
use runtime::SharedRuntimeState;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tnbot_core::Settings;
use tnbot_core::bsky::BskyClient;
use tnbot_core::db::connection::DatabaseManager;
use tnbot_core::db::models::{Conversation, FailedEvent, Identity};
use tnbot_core::db::repository::{ConversationRepository, FailedEventRepository, IdentityRepository, LibsqlRepository};
use tokio::sync::RwLock;

const CSS: &str = include_str!("assets/stylesheet.css");

#[derive(Clone)]
struct AppState {
    settings: Settings,
    db_path: PathBuf,
    runtime: SharedRuntimeState,
    auth: Arc<SessionStore>,
    web: WebRuntimeInfo,
    bsky_client: Option<BskyClient>,
    dry_run: bool,
}

#[derive(Clone, Debug)]
struct WebRuntimeInfo {
    bind_addr: SocketAddr,
    username: String,
    generated_password: bool,
}

#[derive(Clone, Debug)]
struct WebConfig {
    bind_addr: SocketAddr,
    username: String,
    password: String,
    generated_password: bool,
}

impl WebConfig {
    fn from_env() -> Self {
        let bind_addr = std::env::var("TNBOT_WEB__BIND")
            .unwrap_or_else(|_| "127.0.0.1:3000".to_string())
            .parse()
            .unwrap_or_else(|_| SocketAddr::from(([127, 0, 0, 1], 3000)));

        let username = std::env::var("TNBOT_WEB__USERNAME").unwrap_or_else(|_| "admin".to_string());

        match std::env::var("TNBOT_WEB__PASSWORD") {
            Ok(password) if !password.trim().is_empty() => {
                Self { bind_addr, username, password, generated_password: false }
            }
            _ => {
                let password = format!("{:032x}{:032x}", rand::random::<u128>(), rand::random::<u128>());
                Self { bind_addr, username, password, generated_password: true }
            }
        }
    }
}

#[derive(Debug)]
struct SessionStore {
    username: String,
    password: String,
    sessions: RwLock<HashMap<String, Instant>>,
    ttl: Duration,
}

impl SessionStore {
    fn new(username: String, password: String) -> Self {
        Self { username, password, sessions: RwLock::new(HashMap::new()), ttl: Duration::from_secs(60 * 60 * 12) }
    }

    fn verify_credentials(&self, username: &str, password: &str) -> bool {
        username == self.username && password == self.password
    }

    async fn issue_session(&self) -> String {
        let token = format!("{:032x}{:032x}", rand::random::<u128>(), rand::random::<u128>());
        let expires_at = Instant::now() + self.ttl;

        let mut sessions = self.sessions.write().await;
        sessions.insert(token.clone(), expires_at);
        token
    }

    async fn validate_session(&self, token: &str) -> bool {
        let mut sessions = self.sessions.write().await;
        let now = Instant::now();
        sessions.retain(|_, expires_at| *expires_at > now);
        sessions.get(token).is_some_and(|expires_at| *expires_at > now)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NavItem {
    Dashboard,
    Logs,
    Chat,
    Config,
}

impl NavItem {
    fn items() -> Vec<NavItem> {
        vec![NavItem::Dashboard, NavItem::Logs, NavItem::Chat, NavItem::Config]
    }
}

#[derive(Debug)]
struct DashboardSnapshot {
    paused: bool,
    uptime: String,
    last_event_label: String,
    last_event_absolute: String,
    queue_depth: usize,
    pending_embeddings: i64,
    monthly_tokens: i64,
    conversation_count: i64,
    identity_count: i64,
    processed_events: u64,
    failed_events: u64,
    last_model_latency_ms: u64,
}

pub async fn run(settings: Settings, runtime: SharedRuntimeState, dry_run: bool) -> anyhow::Result<()> {
    let config = WebConfig::from_env();

    if config.generated_password {
        tracing::warn!(
            username = %config.username,
            password = %config.password,
            "TNBOT_WEB__PASSWORD was not set. Generated an ephemeral dashboard password."
        );
    }

    let bsky_client = if dry_run {
        None
    } else if settings.bluesky.handle.trim().is_empty() || settings.bluesky.app_password.trim().is_empty() {
        tracing::warn!("Bluesky credentials are missing; manual broadcast/reply controls will run in preview mode");
        None
    } else {
        Some(BskyClient::with_credentials(
            &settings.bluesky.pds_host,
            &settings.bluesky.handle,
            &settings.bluesky.app_password,
        ))
    };

    let state = AppState {
        db_path: settings.database.path.clone(),
        settings,
        runtime,
        auth: Arc::new(SessionStore::new(config.username.clone(), config.password.clone())),
        web: WebRuntimeInfo {
            bind_addr: config.bind_addr,
            username: config.username.clone(),
            generated_password: config.generated_password,
        },
        bsky_client,
        dry_run,
    };

    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(config.bind_addr)
        .await
        .with_context(|| format!("Failed to bind web dashboard to {}", config.bind_addr))?;

    tracing::info!(
        "Control deck running at http://{}/dashboard (username: {})",
        config.bind_addr,
        config.username
    );

    axum::serve(listener, app)
        .await
        .context("Web dashboard server terminated unexpectedly")
}

fn build_router(state: AppState) -> Router {
    let protected = Router::new()
        .merge(pages::dashboard::protected_routes())
        .merge(pages::logs::protected_routes())
        .merge(pages::config::protected_routes())
        .merge(pages::admin::protected_routes())
        .merge(pages::chat::protected_routes())
        .merge(pages::login::protected_routes())
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            pages::login::require_auth,
        ));

    Router::new()
        .route("/", get(root_redirect))
        .route("/assets/app.css", get(stylesheet))
        .merge(pages::health::public_routes())
        .merge(pages::login::public_routes())
        .merge(protected)
        .with_state(state)
}

async fn root_redirect() -> Redirect {
    Redirect::to("/dashboard")
}

async fn stylesheet() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/css; charset=utf-8")], CSS)
}

async fn dashboard_snapshot_or_default(state: &AppState) -> DashboardSnapshot {
    load_dashboard_snapshot(state).await.unwrap_or_else(|e| {
        tracing::error!(error = %e, "Failed to build dashboard snapshot");
        DashboardSnapshot {
            paused: state.runtime.is_paused(),
            uptime: formatters::fuptime(state.runtime.started_at()),
            last_event_label: "unavailable".to_string(),
            last_event_absolute: "unavailable".to_string(),
            queue_depth: state.runtime.events_in_flight(),
            pending_embeddings: 0,
            monthly_tokens: 0,
            conversation_count: 0,
            identity_count: 0,
            processed_events: state.runtime.events_processed(),
            failed_events: state.runtime.events_failed(),
            last_model_latency_ms: state.runtime.last_model_latency_ms(),
        }
    })
}

async fn load_dashboard_snapshot(state: &AppState) -> anyhow::Result<DashboardSnapshot> {
    let manager = DatabaseManager::open(&state.db_path).await?;
    let stats = manager.stats().await?;
    let conn = manager.db().connect()?;

    let mut pending_rows = conn
        .query("SELECT COUNT(*) FROM embedding_jobs WHERE status = 'pending'", ())
        .await?;
    let pending_embeddings =
        if let Ok(Some(row)) = pending_rows.next().await { row.get::<i64>(0).unwrap_or(0) } else { 0 };

    let month = Utc::now().format("%Y-%m").to_string();
    let mut token_rows = conn
        .query(
            "SELECT COALESCE(SUM((LENGTH(content) + 3) / 4), 0)
             FROM conversations
             WHERE role = 'model' AND substr(created_at, 1, 7) = ?1",
            [month.as_str()],
        )
        .await?;
    let monthly_tokens = if let Ok(Some(row)) = token_rows.next().await { row.get::<i64>(0).unwrap_or(0) } else { 0 };

    let last_event_us = state.runtime.last_jetstream_event_us();

    Ok(DashboardSnapshot {
        paused: state.runtime.is_paused(),
        uptime: formatters::fuptime(state.runtime.started_at()),
        last_event_label: formatters::rel_event(last_event_us),
        last_event_absolute: formatters::abs_event(last_event_us),
        queue_depth: state.runtime.events_in_flight(),
        pending_embeddings,
        monthly_tokens,
        conversation_count: stats.conversations_count,
        identity_count: stats.identities_count,
        processed_events: state.runtime.events_processed(),
        failed_events: state.runtime.events_failed(),
        last_model_latency_ms: state.runtime.last_model_latency_ms(),
    })
}

async fn open_repo(db_path: &Path) -> anyhow::Result<LibsqlRepository> {
    let manager = DatabaseManager::open(db_path).await?;
    let conn = manager.db().connect()?;
    Ok(LibsqlRepository::new(conn))
}

async fn load_recent_conversations(db_path: &Path, limit: i64) -> anyhow::Result<Vec<Conversation>> {
    let repo = open_repo(db_path).await?;
    let rows = ConversationRepository::get_recent(&repo, limit, 0).await?;
    Ok(rows)
}

async fn load_recent_failed_events(db_path: &Path, limit: i64) -> anyhow::Result<Vec<FailedEvent>> {
    let repo = open_repo(db_path).await?;
    let rows = FailedEventRepository::get_recent(&repo, limit).await?;
    Ok(rows)
}

async fn load_identities(db_path: &Path, limit: usize) -> anyhow::Result<Vec<Identity>> {
    let repo = open_repo(db_path).await?;
    let mut identities = repo.list_all().await?;
    identities.sort_by(|a, b| b.last_updated.cmp(&a.last_updated));
    identities.truncate(limit);
    Ok(identities)
}

fn bot_did_or_fallback(settings: &Settings) -> String {
    if settings.bot.did.trim().is_empty() {
        "did:unknown".to_string()
    } else {
        settings.bot.did.clone()
    }
}
