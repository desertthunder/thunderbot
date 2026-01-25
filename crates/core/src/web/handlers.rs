use crate::bsky::BskyClient;
use crate::db::DatabaseRepository;
use crate::web::{ReplyContext, UserClient};
use crate::web::cookies::{SessionCookie, UserSession, clear_session_cookie, set_session_cookie};
use crate::web::templates::DashboardStats;
use anyhow::Result;
use axum::{
    Form,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use maud;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Clone)]
pub struct WebAppState {
    pub db: Arc<dyn DatabaseRepository>,
    pub bsky_client: Arc<BskyClient>,
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
            Ok(StatusCode::OK.into_response())
        }
        Err(e) => {
            tracing::error!("Failed to create post: {}", e);
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
    State(_state): State<WebAppState>, Form(form): Form<ClearThreadForm>,
) -> Result<Response, StatusCode> {
    tracing::info!("Clearing thread context: {}", form.root_uri);
    Ok(StatusCode::OK.into_response())
}

pub async fn get_status() -> impl IntoResponse {
    Html(
        maud::html! {
            small { "Last event: " (chrono::Utc::now().to_rfc3339()) }
        }
        .into_string(),
    )
}

pub async fn get_login() -> impl IntoResponse {
    Html(super::templates::login_page().into_string())
}

#[allow(clippy::cognitive_complexity)]
pub async fn post_login(State(state): State<WebAppState>, Form(form): Form<LoginForm>) -> Result<Response, StatusCode> {
    if let Err(e) = check_allowed_handle(&form.handle) {
        tracing::error!("Handle check failed: {}", e);
        return Err(StatusCode::FORBIDDEN);
    }

    let session = match state.bsky_client.create_session(&form.handle, &form.password).await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Authentication failed: {}", e);
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

    let mut response = Redirect::to("/chat").into_response();
    for (_name, value) in cookies {
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
    for (_name, value) in cookies {
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

#[allow(clippy::cognitive_complexity)]
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

        if let Some(parent_uri) = parent_uri {
            let parent_post = match state.bsky_client.get_post(&parent_uri).await {
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

            match user_client
                .create_reply(&reply_context)
                .await
            {
                Ok(result) => result,
                Err(e) => {
                    tracing::error!("Failed to create reply: {}", e);
                    return Err(StatusCode::INTERNAL_SERVER_ERROR);
                }
            }
        } else {
            match user_client.create_post(&form.text, &bot_did, &bot_handle).await {
                Ok(result) => result,
                Err(e) => {
                    tracing::error!("Failed to create post: {}", e);
                    return Err(StatusCode::INTERNAL_SERVER_ERROR);
                }
            }
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
