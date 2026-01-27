use crate::bsky::BskyClient;
use crate::control::{PolicyEnforcer, SessionManager, StatusBroadcaster};
use crate::db::types::FilterPresetRow;
use crate::db::{ActivityLogRow, DatabaseRepository};
use crate::health::{
    ComponentHealth, DatabaseHealthCheck, HealthCheck, HealthRegistry, HealthStatus, JetstreamHealthCheck,
    JetstreamState,
};
use crate::web::cookies::{SessionCookie, UserSession, clear_session_cookie, set_session_cookie};
use crate::web::templates::DashboardStats;
use crate::web::{ReplyContext, UserClient};

use anyhow::Result;
use axum::{
    Form,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chrono::{DateTime, Utc};
use maud;
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

/// Application state shared across all web handlers.
#[derive(Clone)]
pub struct WebAppState {
    /// Database repository
    pub db: Arc<dyn DatabaseRepository>,
    /// Bluesky API client
    pub bsky_client: Arc<BskyClient>,
    /// Health check registry
    pub health: Arc<HealthRegistry>,
    /// Jetstream connection state
    pub jetstream_state: Arc<tokio::sync::RwLock<JetstreamState>>,
    /// Session manager for proactive token refresh
    pub session_manager: Arc<SessionManager>,
    /// Policy enforcer for quiet hours and reply limits
    pub policy_enforcer: Arc<PolicyEnforcer>,
    /// Status broadcaster for profile updates and announcements
    pub broadcaster: Arc<StatusBroadcaster>,
    /// Event sender for DLQ retry (None if jetstream not running)
    pub event_sender: Option<tokio::sync::mpsc::Sender<crate::jetstream::event::JetstreamEvent>>,
    /// Agent reference for operational control (preview mode, etc.)
    pub agent: Arc<crate::Agent>,
}

#[derive(Deserialize)]
pub struct PostForm {
    text: String,
}

#[derive(Deserialize)]
pub struct LoginForm {
    handle: String,
    password: String,
}

#[derive(Deserialize)]
pub struct ChatMessageForm {
    text: String,
    #[serde(default)]
    thread_uri: Option<String>,
}

#[derive(Deserialize)]
pub struct SearchForm {
    query: String,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    date_from: Option<String>,
    #[serde(default)]
    date_to: Option<String>,
}

#[derive(Deserialize)]
pub struct BulkDeleteForm {
    thread_uris: Vec<String>,
}

#[derive(Deserialize)]
pub struct CleanupForm {
    days: i64,
}

#[derive(Deserialize)]
pub struct MuteAuthorForm {
    did: String,
}

#[derive(Deserialize)]
pub struct FilterPresetForm {
    name: String,
    filters_json: String,
}

fn get_session_from_cookies(headers: &HeaderMap) -> Result<Option<UserSession>> {
    let cookie_header = headers.get("cookie").and_then(|h| h.to_str().ok());

    if let Some(cookies_str) = cookie_header {
        for cookie in cookies_str.split(';') {
            let cookie = cookie.trim();
            if let Some(session_part) = cookie.strip_prefix("thunderbot_session=")
                && !session_part.is_empty()
            {
                let cookie_mgr = SessionCookie::new()?;
                let session = cookie_mgr.decrypt_session(session_part)?;
                return Ok(Some(session));
            }
        }
    }

    Ok(None)
}

fn check_allowed_handle(handle: &str) -> Result<()> {
    let allowed = std::env::var("ALLOWED_HANDLES")
        .unwrap_or_else(|_| String::new())
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .collect::<Vec<_>>();

    if allowed.is_empty() {
        anyhow::bail!("ALLOWED_HANDLES environment variable is not set");
    }

    if !allowed.iter().any(|a| *a == handle.to_lowercase()) {
        anyhow::bail!("Handle '{}' is not in ALLOWED_HANDLES list", handle);
    }

    Ok(())
}

#[derive(Deserialize)]
pub struct ClearThreadForm {
    root_uri: String,
}

pub async fn get_landing() -> impl IntoResponse {
    Html(super::templates::landing_page().into_string())
}

pub async fn get_dashboard(State(state): State<WebAppState>) -> Result<Response, StatusCode> {
    match state.db.get_stats().await {
        Ok(stats) => {
            let dashboard_stats = DashboardStats {
                conversation_count: stats.conversation_count,
                thread_count: stats.thread_count,
                identity_count: stats.identity_count,
            };
            Ok(Html(super::templates::dashboard_page(&dashboard_stats).into_string()).into_response())
        }
        Err(e) => {
            tracing::error!("Failed to get stats: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_threads(State(state): State<WebAppState>) -> Result<Response, StatusCode> {
    match state.db.get_all_threads(50).await {
        Ok(threads) => Ok(Html(super::templates::threads_list(&threads).into_string()).into_response()),
        Err(e) => {
            tracing::error!("Failed to get threads: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_thread_detail(
    State(state): State<WebAppState>, Path(thread_id): Path<String>,
) -> Result<Response, StatusCode> {
    let thread_uri = match STANDARD.decode(&thread_id) {
        Ok(bytes) => String::from_utf8(bytes).unwrap_or_default(),
        Err(_) => return Err(StatusCode::BAD_REQUEST),
    };

    match state.db.get_thread_history(&thread_uri).await {
        Ok(rows) => {
            let messages: Vec<_> = rows
                .iter()
                .map(|row| super::templates::ConversationMessage {
                    author_did: row.author_did.clone(),
                    role: row.role.clone(),
                    content: row.content.clone(),
                    created_at: row.created_at,
                })
                .collect();
            Ok(Html(super::templates::thread_detail(&messages, &thread_uri).into_string()).into_response())
        }
        Err(e) => {
            tracing::error!("Failed to get thread detail: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_identities(State(state): State<WebAppState>) -> Result<Response, StatusCode> {
    match state.db.get_all_identities().await {
        Ok(rows) => {
            let identities: Vec<_> = rows
                .iter()
                .map(|row| super::templates::IdentityInfo {
                    did: row.did.clone(),
                    handle: row.handle.clone(),
                    last_updated: row.last_updated,
                })
                .collect();
            Ok(Html(super::templates::identities_list(&identities).into_string()).into_response())
        }
        Err(e) => {
            tracing::error!("Failed to get identities: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_admin() -> impl IntoResponse {
    Html(super::templates::admin_page().into_string())
}

pub async fn post_post(State(state): State<WebAppState>, Form(form): Form<PostForm>) -> Result<Response, StatusCode> {
    match state.bsky_client.create_post(&form.text).await {
        Ok(result) => {
            tracing::info!("Post created via dashboard: {}", result.uri);

            let activity = ActivityLogRow {
                id: Uuid::new_v4().to_string(),
                action_type: "post".to_string(),
                description: format!("Manual post: {}", &form.text.chars().take(50).collect::<String>()),
                thread_uri: Some(result.uri.clone()),
                metadata_json: Some(
                    serde_json::json!({"post_uri": result.uri, "content_length": form.text.len()}).to_string(),
                ),
                created_at: Utc::now(),
            };
            let _ = state.db.log_activity(activity).await;

            Ok(StatusCode::OK.into_response())
        }
        Err(e) => {
            tracing::error!("Failed to create post: {}", e);

            let activity = ActivityLogRow {
                id: Uuid::new_v4().to_string(),
                action_type: "error".to_string(),
                description: format!("Failed to create post: {}", e),
                thread_uri: None,
                metadata_json: Some(serde_json::json!({"error": e.to_string()}).to_string()),
                created_at: Utc::now(),
            };
            let _ = state.db.log_activity(activity).await;

            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn post_pause() -> impl IntoResponse {
    tracing::info!("Bot paused via dashboard");
    Html(
        maud::html! {
            span class="status-badge paused" { "Bot Paused" }
        }
        .into_string(),
    )
}

pub async fn post_resume() -> impl IntoResponse {
    tracing::info!("Bot resumed via dashboard");
    Html(
        maud::html! {
            span class="status-badge active" { "Bot Active" }
        }
        .into_string(),
    )
}

pub async fn post_clear_thread(
    State(_): State<WebAppState>, Form(form): Form<ClearThreadForm>,
) -> Result<Response, StatusCode> {
    tracing::info!("Clearing thread context: {}", form.root_uri);
    Ok(StatusCode::OK.into_response())
}

pub async fn get_status(State(state): State<WebAppState>) -> impl IntoResponse {
    let version = env!("CARGO_PKG_VERSION");
    let report = state.health.generate_report(version.to_string()).await;

    Html(
        maud::html! {
            div.health-grid {
                @for (component, health) in report.checks.iter() {
                    @let status_class = match health.status {
                        HealthStatus::Pass => "healthy",
                        HealthStatus::Warn => "degraded",
                        HealthStatus::Fail => "unhealthy",
                    };

                    @let status_emoji = match health.status {
                        HealthStatus::Pass => "✓",
                        HealthStatus::Warn => "⚠",
                        HealthStatus::Fail => "✗",
                    };

                    div.health-card.(status_class) {
                        div.health-card-header {
                            strong { (component) }
                            span.health-status { (status_emoji) " " (format!("{:?}", health.status).to_lowercase()) }
                        }
                        @if let Some(output) = &health.output {
                            div.health-card-body {
                                small { (output) }
                            }
                        }
                        @if let Some(error) = &health.error {
                            div.health-card-body {
                                small class="error" { (error) }
                            }
                        }
                        @if health.observed_value > 0 {
                            div.health-card-footer {
                                small { "Latency: " (health.observed_value) "ms" }
                            }
                        }
                    }
                }
            }
        }
        .into_string(),
    )
}

pub async fn get_login() -> impl IntoResponse {
    Html(super::templates::login_page().into_string())
}

pub async fn post_login(State(state): State<WebAppState>, Form(form): Form<LoginForm>) -> Result<Response, StatusCode> {
    if let Err(e) = check_allowed_handle(&form.handle) {
        tracing::error!("Handle check failed: {}", e);
        return Err(StatusCode::FORBIDDEN);
    }

    let session = match state.bsky_client.create_session(&form.handle, &form.password).await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Authentication failed: {}", e);

            let activity = ActivityLogRow {
                id: Uuid::new_v4().to_string(),
                action_type: "auth_failure".to_string(),
                description: format!("Failed login attempt for {}", form.handle),
                thread_uri: None,
                metadata_json: Some(serde_json::json!({"handle": form.handle, "error": e.to_string()}).to_string()),
                created_at: Utc::now(),
            };
            let _ = state.db.log_activity(activity).await;

            return Err(StatusCode::UNAUTHORIZED);
        }
    };

    let user_session = UserSession {
        did: session.did.clone(),
        handle: session.handle.clone(),
        access_jwt: session.access_jwt.clone(),
        refresh_jwt: session.refresh_jwt,
        exp: chrono::Utc::now().timestamp() + 7200,
    };

    let mut cookies = Vec::new();
    if let Err(e) = set_session_cookie(&mut cookies, &user_session) {
        tracing::error!("Failed to set session cookie: {}", e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    let activity = ActivityLogRow {
        id: Uuid::new_v4().to_string(),
        action_type: "login".to_string(),
        description: format!("User logged in: {}", session.handle),
        thread_uri: None,
        metadata_json: Some(serde_json::json!({"did": session.did, "handle": session.handle}).to_string()),
        created_at: Utc::now(),
    };
    let _ = state.db.log_activity(activity).await;

    let mut response = Redirect::to("/chat").into_response();
    for (_, value) in cookies {
        if let Err(e) = value.parse::<axum::http::HeaderValue>() {
            tracing::error!("Invalid cookie header: {}", e);
            continue;
        }
        response.headers_mut().insert(
            axum::http::header::SET_COOKIE,
            value.parse::<axum::http::HeaderValue>().unwrap(),
        );
    }

    Ok(response)
}

pub async fn post_logout() -> impl IntoResponse {
    let mut cookies = Vec::new();
    clear_session_cookie(&mut cookies);

    let mut response = Redirect::to("/").into_response();
    for (_, value) in cookies {
        if let Err(e) = value.parse::<axum::http::HeaderValue>() {
            tracing::error!("Invalid cookie header: {}", e);
            continue;
        }
        response
            .headers_mut()
            .insert(axum::http::header::SET_COOKIE, value.parse().unwrap());
    }

    response
}

pub async fn get_config() -> impl IntoResponse {
    Html(super::templates::config_page().into_string())
}

pub async fn get_chat(State(state): State<WebAppState>, headers: HeaderMap) -> Result<Response, StatusCode> {
    let session = get_session_from_cookies(&headers).unwrap_or(None);

    if let Some(session) = session {
        match state.db.get_user_threads(&session.did, 50).await {
            Ok(threads) => {
                Ok(Html(super::templates::chat_page(&session.handle, &threads).into_string()).into_response())
            }
            Err(e) => {
                tracing::error!("Failed to get user threads: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    } else {
        Ok(Redirect::to("/login").into_response())
    }
}

pub async fn post_chat_send(
    State(state): State<WebAppState>, headers: HeaderMap, Form(form): Form<ChatMessageForm>,
) -> Result<Response, StatusCode> {
    let session = get_session_from_cookies(&headers)
        .unwrap_or(None)
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if let Err(e) = check_allowed_handle(&session.handle) {
        tracing::error!("Handle check failed: {}", e);
        return Err(StatusCode::FORBIDDEN);
    }

    let pds_host = std::env::var("PDS_HOST").unwrap_or_else(|_| "https://bsky.social".to_string());
    let mut user_client = UserClient::new(pds_host, session.clone());

    let bot_handle = std::env::var("BSKY_HANDLE").unwrap_or_else(|_| "thunderbot.bsky.social".to_string());
    let bot_did = match state.bsky_client.resolve_handle(&bot_handle).await {
        Ok(did) => did,
        Err(e) => {
            tracing::error!("Failed to resolve bot handle: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let post_result = if let Some(thread_uri) = form.thread_uri.as_ref().filter(|uri| !uri.trim().is_empty()) {
        let rows = match state.db.get_thread_history(thread_uri).await {
            Ok(rows) => rows,
            Err(e) => {
                tracing::error!("Failed to load thread history: {}", e);
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        };

        let parent_uri = rows
            .iter()
            .rev()
            .find(|row| !row.post_uri.is_empty())
            .map(|row| row.post_uri.clone());

        match parent_uri {
            Some(p_uri) => {
                let parent_post = match state.bsky_client.get_post(&p_uri).await {
                    Ok(post) => post,
                    Err(e) => {
                        tracing::error!("Failed to load parent post: {}", e);
                        return Err(StatusCode::INTERNAL_SERVER_ERROR);
                    }
                };

                let root_post = match state.bsky_client.get_post(thread_uri).await {
                    Ok(post) => post,
                    Err(e) => {
                        tracing::error!("Failed to load root post: {}", e);
                        return Err(StatusCode::INTERNAL_SERVER_ERROR);
                    }
                };

                let reply_context = ReplyContext {
                    text: form.text.clone(),
                    parent_uri: parent_post.uri.clone(),
                    parent_cid: parent_post.cid.clone(),
                    root_uri: root_post.uri.clone(),
                    root_cid: root_post.cid.clone(),
                    bot_did,
                    bot_handle,
                };

                match user_client.create_reply(&reply_context).await {
                    Ok(result) => result,
                    Err(e) => {
                        tracing::error!("Failed to create reply: {}", e);
                        return Err(StatusCode::INTERNAL_SERVER_ERROR);
                    }
                }
            }
            None => match user_client.create_post(&form.text, &bot_did, &bot_handle).await {
                Ok(result) => result,
                Err(e) => {
                    tracing::error!("Failed to create post: {}", e);
                    return Err(StatusCode::INTERNAL_SERVER_ERROR);
                }
            },
        }
    } else {
        match user_client.create_post(&form.text, &bot_did, &bot_handle).await {
            Ok(result) => result,
            Err(e) => {
                tracing::error!("Failed to create post: {}", e);
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
    };

    tracing::info!("Posted as {}: {}", session.handle, post_result.uri);

    Ok(Redirect::to("/chat").into_response())
}

pub async fn get_health(State(state): State<WebAppState>) -> Response {
    let version = env!("CARGO_PKG_VERSION").to_string();
    let mut report = state.health.generate_report(version).await;

    match DatabaseHealthCheck::new(state.db.clone()).check().await {
        Ok(health) => report = report.with_check("database", health),
        Err(e) => report = report.with_check("database", ComponentHealth::unhealthy("database", e.to_string())),
    }

    match JetstreamHealthCheck::new(state.jetstream_state.clone()).check().await {
        Ok(health) => report = report.with_check("jetstream", health),
        Err(e) => report = report.with_check("jetstream", ComponentHealth::unhealthy("jetstream", e.to_string())),
    }

    let status_code = report.http_status();
    let body = serde_json::to_vec(&report).unwrap();

    axum::response::Response::builder()
        .status(StatusCode::from_u16(status_code).unwrap_or(StatusCode::SERVICE_UNAVAILABLE))
        .header("Content-Type", "application/health+json")
        .header("Cache-Control", "max-age=5")
        .body(axum::body::Body::from(body))
        .unwrap_or_else(|_| {
            axum::response::Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(axum::body::Body::from("Internal error"))
                .unwrap()
        })
}

pub async fn get_metrics() -> Response {
    let version = env!("CARGO_PKG_VERSION");
    let metrics = crate::metrics::Metrics::new();
    let prometheus_output = metrics.render_prometheus(version).await;
    let body = prometheus_output.into_bytes();

    axum::response::Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/plain; version=0.0.4; charset=utf-8")
        .body(axum::body::Body::from(body))
        .unwrap_or_else(|_| {
            axum::response::Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(axum::body::Body::from("Internal error"))
                .unwrap()
        })
}

pub async fn get_search() -> impl IntoResponse {
    Html(super::templates::search_page().into_string())
}

pub async fn post_search(
    State(state): State<WebAppState>, Form(form): Form<SearchForm>,
) -> Result<Response, StatusCode> {
    let author_filter = form.author.as_deref();
    let role_filter = form.role.as_deref();

    let date_from = if let Some(ref df) = form.date_from {
        Some(
            DateTime::parse_from_rfc3339(df)
                .map_err(|_| StatusCode::BAD_REQUEST)?
                .with_timezone(&chrono::Utc),
        )
    } else {
        None
    };

    let date_to = if let Some(ref dt) = form.date_to {
        Some(
            DateTime::parse_from_rfc3339(dt)
                .map_err(|_| StatusCode::BAD_REQUEST)?
                .with_timezone(&chrono::Utc),
        )
    } else {
        None
    };

    match state
        .db
        .search_conversations(&form.query, author_filter, role_filter, date_from, date_to, 50)
        .await
    {
        Ok(results) => Ok(Html(super::templates::search_results(&results, &form.query).into_string()).into_response()),
        Err(e) => {
            tracing::error!("Search failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_export_json(State(state): State<WebAppState>) -> Response {
    match state.db.export_all_conversations().await {
        Ok(conversations) => match serde_json::to_vec(&conversations) {
            Ok(json) => axum::response::Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/json")
                .header("Content-Disposition", "attachment; filename=\"conversations.json\"")
                .body(axum::body::Body::from(json))
                .unwrap_or_else(|_| {
                    axum::response::Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(axum::body::Body::from("Internal error"))
                        .unwrap()
                }),
            Err(e) => {
                tracing::error!("Failed to serialize JSON: {}", e);
                axum::response::Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(axum::body::Body::from("Failed to serialize"))
                    .unwrap()
            }
        },
        Err(e) => {
            tracing::error!("Export failed: {}", e);
            axum::response::Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(axum::body::Body::from("Export failed"))
                .unwrap()
        }
    }
}

pub async fn get_export_csv(State(state): State<WebAppState>) -> Response {
    match state.db.export_all_conversations().await {
        Ok(conversations) => {
            let mut buffer = Vec::new();
            {
                let mut writer = csv::Writer::from_writer(&mut buffer);
                for conv in &conversations {
                    if let Err(e) = writer.write_record([
                        &conv.id,
                        &conv.thread_root_uri,
                        &conv.post_uri,
                        conv.parent_uri.as_deref().unwrap_or(""),
                        &conv.author_did,
                        &conv.role,
                        &conv.content,
                        &conv.created_at.to_rfc3339(),
                    ]) {
                        tracing::error!("Failed to write CSV row: {}", e);
                        return axum::response::Response::builder()
                            .status(StatusCode::INTERNAL_SERVER_ERROR)
                            .body(axum::body::Body::from("CSV write failed"))
                            .unwrap();
                    }
                }
                if let Err(e) = writer.flush() {
                    tracing::error!("Failed to flush CSV: {}", e);
                    return axum::response::Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(axum::body::Body::from("CSV flush failed"))
                        .unwrap();
                }
            }

            axum::response::Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "text/csv")
                .header("Content-Disposition", "attachment; filename=\"conversations.csv\"")
                .body(axum::body::Body::from(buffer))
                .unwrap_or_else(|_| {
                    axum::response::Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(axum::body::Body::from("Internal error"))
                        .unwrap()
                })
        }
        Err(e) => {
            tracing::error!("Export failed: {}", e);
            axum::response::Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(axum::body::Body::from("Export failed"))
                .unwrap()
        }
    }
}

pub async fn post_bulk_delete(
    State(state): State<WebAppState>, Form(form): Form<BulkDeleteForm>,
) -> Result<Response, StatusCode> {
    match state.db.delete_conversations_by_uris(&form.thread_uris).await {
        Ok(count) => {
            tracing::info!("Deleted {} threads", count);
            let activity = ActivityLogRow {
                id: Uuid::new_v4().to_string(),
                action_type: "bulk_delete".to_string(),
                description: format!("Deleted {} threads", count),
                thread_uri: None,
                metadata_json: Some(serde_json::json!({"thread_count": count, "uris": form.thread_uris}).to_string()),
                created_at: Utc::now(),
            };

            let _ = state.db.log_activity(activity).await;
            Ok(Redirect::to("/threads").into_response())
        }
        Err(e) => {
            tracing::error!("Bulk delete failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn post_cleanup_old(
    State(state): State<WebAppState>, Form(form): Form<CleanupForm>,
) -> Result<Response, StatusCode> {
    match state.db.delete_old_conversations(form.days).await {
        Ok(count) => {
            tracing::info!("Cleaned up {} conversations older than {} days", count, form.days);
            let activity = ActivityLogRow {
                id: Uuid::new_v4().to_string(),
                action_type: "cleanup".to_string(),
                description: format!("Cleaned {} conversations older than {} days", count, form.days),
                thread_uri: None,
                metadata_json: Some(serde_json::json!({"conversation_count": count, "days": form.days}).to_string()),
                created_at: Utc::now(),
            };

            let _ = state.db.log_activity(activity).await;
            Ok(Redirect::to("/threads").into_response())
        }
        Err(e) => {
            tracing::error!("Cleanup failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn post_mute_author(
    State(state): State<WebAppState>, headers: HeaderMap, Form(form): Form<MuteAuthorForm>,
) -> Result<Response, StatusCode> {
    let session = get_session_from_cookies(&headers)
        .unwrap_or(None)
        .ok_or(StatusCode::UNAUTHORIZED)?;

    match state.db.mute_author(&form.did, &session.did).await {
        Ok(_) => {
            tracing::info!("Muted author: {}", form.did);
            Ok(StatusCode::OK.into_response())
        }
        Err(e) => {
            tracing::error!("Failed to mute author: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn post_unmute_author(
    State(state): State<WebAppState>, Form(form): Form<MuteAuthorForm>,
) -> Result<Response, StatusCode> {
    match state.db.unmute_author(&form.did).await {
        Ok(_) => {
            tracing::info!("Unmuted author: {}", form.did);
            Ok(StatusCode::OK.into_response())
        }
        Err(e) => {
            tracing::error!("Failed to unmute author: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn post_save_preset(
    State(state): State<WebAppState>, headers: HeaderMap, Form(form): Form<FilterPresetForm>,
) -> Result<Response, StatusCode> {
    let session = get_session_from_cookies(&headers)
        .unwrap_or(None)
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let preset = FilterPresetRow {
        id: uuid::Uuid::new_v4().to_string(),
        name: form.name.clone(),
        filters_json: form.filters_json.clone(),
        created_at: chrono::Utc::now(),
        created_by: session.did.clone(),
    };

    match state.db.save_filter_preset(preset).await {
        Ok(_) => {
            tracing::info!("Saved filter preset: {}", form.name);
            Ok(StatusCode::OK.into_response())
        }
        Err(e) => {
            tracing::error!("Failed to save preset: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_filtered_threads(
    State(state): State<WebAppState>, Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Response, StatusCode> {
    let min_length = params.get("min_length").and_then(|s| s.parse::<usize>().ok());
    let recent_hours = params.get("recent_hours").and_then(|s| s.parse::<i64>().ok());

    let threads = if let Some(min) = min_length {
        state.db.get_conversations_with_length_filter(min, 50).await
    } else if let Some(hours) = recent_hours {
        state.db.get_recent_threads(hours, 50).await
    } else {
        state.db.get_all_threads(50).await
    };

    match threads {
        Ok(threads) => Ok(Html(super::templates::threads_list(&threads).into_string()).into_response()),
        Err(e) => {
            tracing::error!("Failed to get filtered threads: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_activity_timeline(
    State(state): State<WebAppState>, Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Response, StatusCode> {
    let action_type = params.get("action_type").map(|s| s.as_str());
    let limit = params.get("limit").and_then(|s| s.parse::<usize>().ok()).unwrap_or(50);

    match state.db.get_activity_log(action_type, limit).await {
        Ok(activities) => Ok(Html(super::templates::activity_timeline_page(&activities).into_string()).into_response()),
        Err(e) => {
            tracing::error!("Failed to get activity log: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
