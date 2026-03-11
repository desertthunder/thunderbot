use crate::{AppState, bot_did_or_fallback, open_repo};
use axum::Router;
use axum::extract::{Form, State};
use axum::response::{IntoResponse, Redirect};
use axum::routing::post;
use chrono::Utc;
use serde::Deserialize;
use tnbot_core::db::models::{CreateConversationParams, Role};
use tnbot_core::db::repository::{ConversationRepository, MemoryRepository};

#[derive(Debug, Deserialize)]
struct PauseForm {
    paused: bool,
}

#[derive(Debug, Deserialize)]
struct BroadcastForm {
    text: String,
}

#[derive(Debug, Deserialize)]
struct ThreadReplyForm {
    root_uri: String,
    text: String,
}

#[derive(Debug, Deserialize)]
struct ClearThreadForm {
    root_uri: String,
}

pub(crate) fn protected_routes() -> Router<AppState> {
    Router::new()
        .route("/admin/pause", post(set_pause))
        .route("/admin/broadcast", post(broadcast_post))
        .route("/admin/reply", post(reply_in_thread))
        .route("/admin/clear-thread", post(clear_thread_context))
}

async fn set_pause(State(state): State<AppState>, Form(form): Form<PauseForm>) -> impl IntoResponse {
    state.runtime.set_paused(form.paused);

    if form.paused {
        redirect_with_message("/dashboard", "notice", "Bot paused")
    } else {
        redirect_with_message("/dashboard", "notice", "Bot resumed")
    }
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
