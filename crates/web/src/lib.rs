use anyhow::Context;
use axum::extract::{Form, Query, Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tnbot_core::Settings;
use tnbot_core::bsky::BskyClient;
use tnbot_core::db::connection::DatabaseManager;
use tnbot_core::db::models::{Conversation, CreateConversationParams, Identity, Role};
use tnbot_core::db::repository::{ConversationRepository, IdentityRepository, LibsqlRepository, MemoryRepository};
use tokio::sync::RwLock;

pub mod runtime;
mod views;

use runtime::SharedRuntimeState;

const SESSION_COOKIE: &str = "tnbot_session";
const CSS: &str = include_str!("assets.css");

#[derive(Clone)]
struct AppState {
    settings: Settings,
    db_path: PathBuf,
    runtime: SharedRuntimeState,
    auth: Arc<SessionStore>,
    bsky_client: Option<BskyClient>,
    dry_run: bool,
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
    Chat,
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

#[derive(Debug)]
struct ThreadSummary {
    root_uri: String,
    handle: String,
    preview: String,
    last_seen: String,
    message_count: usize,
}

#[derive(Debug)]
struct ThreadMessageView {
    role: Role,
    author: String,
    content: String,
    timestamp: String,
    latency: Option<String>,
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
        .route("/dashboard", get(dashboard_page))
        .route("/dashboard/live", get(dashboard_live))
        .route("/chat", get(chat_page))
        .route("/admin/pause", post(set_pause))
        .route("/admin/broadcast", post(broadcast_post))
        .route("/admin/reply", post(reply_in_thread))
        .route("/admin/clear-thread", post(clear_thread_context))
        .route("/logout", post(logout))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));

    Router::new()
        .route("/", get(root_redirect))
        .route("/health", get(health))
        .route("/assets/app.css", get(stylesheet))
        .route("/login", get(login_page).post(login_submit))
        .merge(protected)
        .with_state(state)
}

async fn root_redirect() -> Redirect {
    Redirect::to("/dashboard")
}

async fn stylesheet() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/css; charset=utf-8")], CSS)
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    paused: bool,
    queue_depth: usize,
    last_jetstream_event_us: i64,
    uptime_seconds: u64,
    processed_events: u64,
    failed_events: u64,
}

async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let degraded = state.runtime.events_failed() > 0 && state.runtime.events_in_flight() > 40;
    let response = HealthResponse {
        status: if degraded { "degraded" } else { "ok" },
        paused: state.runtime.is_paused(),
        queue_depth: state.runtime.events_in_flight(),
        last_jetstream_event_us: state.runtime.last_jetstream_event_us(),
        uptime_seconds: state.runtime.started_at().elapsed().as_secs(),
        processed_events: state.runtime.events_processed(),
        failed_events: state.runtime.events_failed(),
    };

    let status = if degraded { StatusCode::SERVICE_UNAVAILABLE } else { StatusCode::OK };

    (status, Json(response))
}

async fn require_auth(State(state): State<AppState>, jar: CookieJar, request: Request, next: Next) -> Response {
    if let Some(cookie) = jar.get(SESSION_COOKIE)
        && state.auth.validate_session(cookie.value()).await
    {
        return next.run(request).await;
    }

    if request.headers().contains_key("HX-Request") {
        return (StatusCode::UNAUTHORIZED, [("HX-Redirect", "/login")]).into_response();
    }

    Redirect::to("/login").into_response()
}

#[derive(Debug, Deserialize, Default)]
struct LoginQuery {
    error: Option<String>,
    notice: Option<String>,
}

async fn login_page(
    State(state): State<AppState>, jar: CookieJar, Query(query): Query<LoginQuery>,
) -> impl IntoResponse {
    if let Some(cookie) = jar.get(SESSION_COOKIE)
        && state.auth.validate_session(cookie.value()).await
    {
        return Redirect::to("/dashboard").into_response();
    }

    Html(views::login_page(query.error.as_deref(), query.notice.as_deref()).into_string()).into_response()
}

#[derive(Debug, Deserialize)]
struct LoginFormData {
    username: String,
    password: String,
}

async fn login_submit(
    State(state): State<AppState>, jar: CookieJar, Form(form): Form<LoginFormData>,
) -> impl IntoResponse {
    if !state
        .auth
        .verify_credentials(form.username.trim(), form.password.trim())
    {
        return Redirect::to("/login?error=Invalid%20credentials").into_response();
    }

    let token = state.auth.issue_session().await;
    let cookie = Cookie::build((SESSION_COOKIE, token))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .build();

    (jar.add(cookie), Redirect::to("/dashboard")).into_response()
}

async fn logout(jar: CookieJar) -> impl IntoResponse {
    let cookie = Cookie::build((SESSION_COOKIE, ""))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .build();

    (jar.remove(cookie), Redirect::to("/login?notice=Signed%20out")).into_response()
}

#[derive(Debug, Deserialize, Default)]
struct DashboardQuery {
    notice: Option<String>,
    error: Option<String>,
}

async fn dashboard_page(State(state): State<AppState>, Query(query): Query<DashboardQuery>) -> impl IntoResponse {
    let snapshot = load_dashboard_snapshot(&state).await.unwrap_or_else(|e| {
        tracing::error!(error = %e, "Failed to build dashboard snapshot");
        DashboardSnapshot {
            paused: state.runtime.is_paused(),
            uptime: format_uptime(state.runtime.started_at()),
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
    });

    let conversations = load_recent_conversations(&state.db_path, 25).await.unwrap_or_default();
    let identities = load_identities(&state.db_path, 25).await.unwrap_or_default();

    let body = views::dashboard_content(
        query.notice.as_deref(),
        query.error.as_deref(),
        &snapshot,
        state.dry_run,
        &conversations,
        &identities,
    );

    Html(
        views::shell(
            &state,
            NavItem::Dashboard,
            "Dashboard",
            snapshot.paused,
            &snapshot.uptime,
            body,
        )
        .into_string(),
    )
}

async fn dashboard_live(State(state): State<AppState>) -> impl IntoResponse {
    match load_dashboard_snapshot(&state).await {
        Ok(snapshot) => Html(views::live_status_cards(&snapshot).into_string()).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "Failed to refresh dashboard live panel");
            Html("<div class=\"alert error\">Live status unavailable</div>".to_string()).into_response()
        }
    }
}

#[derive(Debug, Deserialize, Default)]
struct ChatQuery {
    root: Option<String>,
    q: Option<String>,
    notice: Option<String>,
    error: Option<String>,
}

async fn chat_page(State(state): State<AppState>, Query(query): Query<ChatQuery>) -> Response {
    let repo = match open_repo(&state.db_path).await {
        Ok(repo) => repo,
        Err(e) => {
            let content = views::chat_error_content(&e.to_string());

            return Html(
                views::shell(
                    &state,
                    NavItem::Chat,
                    "Chat",
                    state.runtime.is_paused(),
                    &format_uptime(state.runtime.started_at()),
                    content,
                )
                .into_string(),
            )
            .into_response();
        }
    };

    let thread_summaries = load_thread_summaries(&repo, query.q.as_deref())
        .await
        .unwrap_or_default();

    let selected_root = query
        .root
        .clone()
        .or_else(|| thread_summaries.first().map(|thread| thread.root_uri.clone()));

    let thread_messages = if let Some(root_uri) = selected_root.as_deref() {
        load_thread_messages(&repo, root_uri).await.unwrap_or_default()
    } else {
        Vec::new()
    };

    let content = views::chat_content(&query, &thread_summaries, selected_root.as_deref(), &thread_messages);

    Html(
        views::shell(
            &state,
            NavItem::Chat,
            "Chat",
            state.runtime.is_paused(),
            &format_uptime(state.runtime.started_at()),
            content,
        )
        .into_string(),
    )
    .into_response()
}

#[derive(Debug, Deserialize)]
struct PauseForm {
    paused: bool,
}

async fn set_pause(State(state): State<AppState>, Form(form): Form<PauseForm>) -> impl IntoResponse {
    state.runtime.set_paused(form.paused);

    if form.paused {
        redirect_with_message("/dashboard", "notice", "Bot paused")
    } else {
        redirect_with_message("/dashboard", "notice", "Bot resumed")
    }
}

#[derive(Debug, Deserialize)]
struct BroadcastForm {
    text: String,
}

async fn broadcast_post(State(state): State<AppState>, Form(form): Form<BroadcastForm>) -> impl IntoResponse {
    let text = form.text.trim();

    if text.is_empty() {
        return redirect_with_message("/dashboard", "error", "Broadcast text cannot be empty");
    }

    if text.chars().count() > 300 {
        return redirect_with_message("/dashboard", "error", "Broadcast exceeds 300 characters");
    }

    if state.dry_run {
        return redirect_with_message(
            "/dashboard",
            "notice",
            "Dry-run mode: broadcast preview complete, nothing posted",
        );
    }

    let Some(client) = state.bsky_client.clone() else {
        return redirect_with_message(
            "/dashboard",
            "error",
            "Broadcast unavailable: Bluesky credentials are missing",
        );
    };

    match client.create_post(text).await {
        Ok(record) => {
            if let Ok(repo) = open_repo(&state.db_path).await {
                let _ = repo
                    .create_conversation(CreateConversationParams {
                        root_uri: record.uri.clone(),
                        post_uri: record.uri.clone(),
                        parent_uri: None,
                        author_did: bot_did_or_fallback(&state.settings),
                        role: Role::Model,
                        content: text.to_string(),
                        cid: Some(record.cid.clone()),
                        created_at: Utc::now().to_rfc3339(),
                    })
                    .await;
            }

            redirect_with_message("/dashboard", "notice", "Broadcast posted successfully")
        }
        Err(e) => {
            tracing::error!(error = %e, "Manual broadcast failed");
            redirect_with_message("/dashboard", "error", "Failed to post broadcast")
        }
    }
}

#[derive(Debug, Deserialize)]
struct ThreadReplyForm {
    root_uri: String,
    text: String,
}

async fn reply_in_thread(State(state): State<AppState>, Form(form): Form<ThreadReplyForm>) -> impl IntoResponse {
    let text = form.text.trim();
    let target = format!("/chat?root={}", urlencoding::encode(&form.root_uri));

    if form.root_uri.trim().is_empty() {
        return redirect_with_message("/chat", "error", "Thread root is required");
    }

    if text.is_empty() {
        return redirect_with_message(&target, "error", "Reply text cannot be empty");
    }

    if text.chars().count() > 300 {
        return redirect_with_message(&target, "error", "Reply exceeds 300 characters");
    }

    if state.dry_run {
        return redirect_with_message(
            &target,
            "notice",
            "Dry-run mode: manual reply preview complete, nothing posted",
        );
    }

    let Some(client) = state.bsky_client.clone() else {
        return redirect_with_message(&target, "error", "Reply unavailable: Bluesky credentials are missing");
    };

    let repo = match open_repo(&state.db_path).await {
        Ok(repo) => repo,
        Err(e) => {
            tracing::error!(error = %e, "Failed to open repository for manual reply");
            return redirect_with_message(&target, "error", "Could not access thread data");
        }
    };

    let thread = match repo.get_thread_by_root(&form.root_uri).await {
        Ok(thread) => thread,
        Err(e) => {
            tracing::error!(error = %e, root = %form.root_uri, "Failed to load thread for manual reply");
            return redirect_with_message(&target, "error", "Could not load thread history");
        }
    };

    let Some(parent) = thread.last() else {
        return redirect_with_message(&target, "error", "Thread has no messages to reply to");
    };

    match client.reply_to(&parent.post_uri, text).await {
        Ok(record) => {
            let _ = repo
                .create_conversation(CreateConversationParams {
                    root_uri: form.root_uri.clone(),
                    post_uri: record.uri.clone(),
                    parent_uri: Some(parent.post_uri.clone()),
                    author_did: bot_did_or_fallback(&state.settings),
                    role: Role::Model,
                    content: text.to_string(),
                    cid: Some(record.cid.clone()),
                    created_at: Utc::now().to_rfc3339(),
                })
                .await;

            redirect_with_message(&target, "notice", "Manual reply posted")
        }
        Err(e) => {
            tracing::error!(error = %e, root = %form.root_uri, "Manual thread reply failed");
            redirect_with_message(&target, "error", "Failed to send manual reply")
        }
    }
}

#[derive(Debug, Deserialize)]
struct ClearThreadForm {
    root_uri: String,
}

async fn clear_thread_context(State(state): State<AppState>, Form(form): Form<ClearThreadForm>) -> impl IntoResponse {
    let target = "/chat";

    if form.root_uri.trim().is_empty() {
        return redirect_with_message(target, "error", "Thread root is required");
    }

    let repo = match open_repo(&state.db_path).await {
        Ok(repo) => repo,
        Err(e) => {
            tracing::error!(error = %e, "Failed to open repository for clear thread");
            return redirect_with_message(target, "error", "Could not access thread data");
        }
    };

    let _ = repo.delete_memories_by_root(&form.root_uri).await;

    match repo
        .conn()
        .execute(
            "DELETE FROM conversations WHERE root_uri = ?1",
            [form.root_uri.as_str()],
        )
        .await
    {
        Ok(deleted) => redirect_with_message(
            target,
            "notice",
            &format!("Cleared {} conversation rows for thread", deleted),
        ),
        Err(e) => {
            tracing::error!(error = %e, root = %form.root_uri, "Failed clearing thread context");
            redirect_with_message(target, "error", "Failed to clear thread context")
        }
    }
}

fn redirect_with_message(path: &str, key: &str, message: &str) -> Redirect {
    let connector = if path.contains('?') { '&' } else { '?' };
    let location = format!("{}{}{}={}", path, connector, key, urlencoding::encode(message));
    Redirect::to(&location)
}

fn chat_thread_href(root_uri: &str, search: Option<&str>) -> String {
    let mut href = format!("/chat?root={}", urlencoding::encode(root_uri));
    if let Some(query) = search.filter(|value| !value.trim().is_empty()) {
        href.push_str("&q=");
        href.push_str(&urlencoding::encode(query));
    }
    href
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
        uptime: format_uptime(state.runtime.started_at()),
        last_event_label: format_relative_event(last_event_us),
        last_event_absolute: format_absolute_event(last_event_us),
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
    let rows = repo.get_recent(limit, 0).await?;
    Ok(rows)
}

async fn load_identities(db_path: &Path, limit: usize) -> anyhow::Result<Vec<Identity>> {
    let repo = open_repo(db_path).await?;
    let mut identities = repo.list_all().await?;
    identities.sort_by(|a, b| b.last_updated.cmp(&a.last_updated));
    identities.truncate(limit);
    Ok(identities)
}

async fn load_thread_summaries(repo: &LibsqlRepository, search: Option<&str>) -> anyhow::Result<Vec<ThreadSummary>> {
    let mut summaries = Vec::new();
    let normalized_query = search
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_lowercase);

    let threads = repo.get_recent_threads(100).await?;

    for (root_uri, last_activity_us) in threads {
        let messages = repo.get_thread_by_root(&root_uri).await?;
        if messages.is_empty() {
            continue;
        }

        let preview_message = messages.last().expect("checked empty");
        let message_count = messages.len();

        let handle_did = messages
            .iter()
            .rev()
            .find(|message| message.role == Role::User)
            .map(|message| message.author_did.clone())
            .unwrap_or_else(|| preview_message.author_did.clone());

        let handle = match repo.get_by_did(&handle_did).await? {
            Some(identity) => format!("@{}", identity.handle),
            None => shorten(&handle_did, 28),
        };

        if let Some(query) = normalized_query.as_deref() {
            let message_match = messages
                .iter()
                .any(|message| message.content.to_lowercase().contains(query));
            let handle_match = handle.to_lowercase().contains(query);
            let root_match = root_uri.to_lowercase().contains(query);

            if !message_match && !handle_match && !root_match {
                continue;
            }
        }

        summaries.push(ThreadSummary {
            root_uri,
            handle,
            preview: shorten(&preview_message.content, 90),
            last_seen: format_relative_event(last_activity_us),
            message_count,
        });
    }

    Ok(summaries)
}

async fn load_thread_messages(repo: &LibsqlRepository, root_uri: &str) -> anyhow::Result<Vec<ThreadMessageView>> {
    let messages = repo.get_thread_by_root(root_uri).await?;
    let mut views = Vec::new();
    let mut handle_cache: HashMap<String, String> = HashMap::new();
    let mut last_user_timestamp: Option<DateTime<Utc>> = None;

    for message in messages {
        let timestamp = parse_rfc3339(&message.created_at);
        let timestamp_label = format_time(&message.created_at);

        let author = match message.role {
            Role::Model => "@thunderbot".to_string(),
            Role::User => {
                if let Some(cached) = handle_cache.get(&message.author_did) {
                    cached.clone()
                } else {
                    let handle = match repo.get_by_did(&message.author_did).await? {
                        Some(identity) => format!("@{}", identity.handle),
                        None => shorten(&message.author_did, 28),
                    };
                    handle_cache.insert(message.author_did.clone(), handle.clone());
                    handle
                }
            }
        };

        let latency = if message.role == Role::Model {
            match (last_user_timestamp, timestamp) {
                (Some(previous_user), Some(model_time)) => {
                    let delta = model_time - previous_user;
                    let millis = delta.num_milliseconds();
                    if millis >= 0 { Some(format!("thinking {}", format_latency(millis as u64))) } else { None }
                }
                _ => None,
            }
        } else {
            None
        };

        if message.role == Role::User {
            last_user_timestamp = timestamp;
        }

        views.push(ThreadMessageView {
            role: message.role,
            author,
            content: message.content,
            timestamp: timestamp_label,
            latency,
        });
    }

    Ok(views)
}

fn bot_did_or_fallback(settings: &Settings) -> String {
    if settings.bot.did.trim().is_empty() {
        "did:unknown".to_string()
    } else {
        settings.bot.did.clone()
    }
}

fn format_uptime(started_at: Instant) -> String {
    let total_secs = started_at.elapsed().as_secs();
    let days = total_secs / 86_400;
    let hours = (total_secs % 86_400) / 3_600;
    let minutes = (total_secs % 3_600) / 60;

    if days > 0 {
        format!("{}d {:02}h {:02}m", days, hours, minutes)
    } else {
        format!("{:02}h {:02}m", hours, minutes)
    }
}

fn parse_rfc3339(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|timestamp| timestamp.with_timezone(&Utc))
}

fn format_time(value: &str) -> String {
    parse_rfc3339(value)
        .map(|timestamp| timestamp.format("%H:%M:%S").to_string())
        .unwrap_or_else(|| value.to_string())
}

fn format_relative_event(time_us: i64) -> String {
    if time_us <= 0 {
        return "waiting".to_string();
    }

    let dt = datetime_from_micros(time_us);
    let Some(dt) = dt else {
        return "unknown".to_string();
    };

    let now = Utc::now();
    let delta = now - dt;

    if delta.num_seconds() < 5 {
        "just now".to_string()
    } else if delta.num_seconds() < 60 {
        format!("{}s ago", delta.num_seconds())
    } else if delta.num_minutes() < 60 {
        format!("{}m ago", delta.num_minutes())
    } else if delta.num_hours() < 24 {
        format!("{}h ago", delta.num_hours())
    } else {
        format!("{}d ago", delta.num_days())
    }
}

fn format_absolute_event(time_us: i64) -> String {
    datetime_from_micros(time_us)
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|| "No event timestamp available".to_string())
}

fn datetime_from_micros(time_us: i64) -> Option<DateTime<Utc>> {
    let seconds = time_us.div_euclid(1_000_000);
    let micros = time_us.rem_euclid(1_000_000) as u32;
    Utc.timestamp_opt(seconds, micros * 1_000).single()
}

fn format_compact(value: i64) -> String {
    if value >= 1_000_000 {
        format!("{:.1}M", value as f64 / 1_000_000.0)
    } else if value >= 1_000 {
        format!("{:.1}K", value as f64 / 1_000.0)
    } else {
        value.to_string()
    }
}

fn format_latency(ms: u64) -> String {
    if ms < 1_000 { format!("{}ms", ms) } else { format!("{:.1}s", ms as f64 / 1_000.0) }
}

fn shorten(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_string();
    }

    value.chars().take(max.saturating_sub(3)).chain("...".chars()).collect()
}
