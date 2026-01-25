use crate::bsky::BskyClient;
use crate::db::DatabaseRepository;
use anyhow::Result;
use axum::{
    Form,
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use base64::prelude::*;
use maud;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Clone)]
pub struct WebAppState {
    pub db: Arc<dyn DatabaseRepository>,
    pub bsky_client: Arc<BskyClient>,
}

#[derive(Clone)]
pub struct DashboardStats {
    pub conversation_count: i64,
    pub thread_count: i64,
    pub identity_count: i64,
}

#[derive(Deserialize)]
pub struct PostForm {
    text: String,
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
    let thread_uri = match BASE64_STANDARD.decode(&thread_id) {
        Ok(bytes) => String::from_utf8(bytes).unwrap_or_default(),
        Err(_) => return Err(StatusCode::BAD_REQUEST),
    };

    match state.db.get_thread_history(&thread_uri).await {
        Ok(messages) => Ok(Html(super::templates::thread_detail(&messages).into_string()).into_response()),
        Err(e) => {
            tracing::error!("Failed to get thread detail: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_identities(State(state): State<WebAppState>) -> Result<Response, StatusCode> {
    match state.db.get_all_identities().await {
        Ok(identities) => Ok(Html(super::templates::identities_list(&identities).into_string()).into_response()),
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
