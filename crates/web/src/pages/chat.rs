use crate::{AppState, NavItem, formatters, open_repo, partials, views};
use axum::Router;
use axum::extract::{Query, State};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use chrono::{DateTime, Utc};
use maud::{Markup, html};
use serde::Deserialize;
use std::collections::HashMap;
use tnbot_core::db::models::Role;
use tnbot_core::db::repository::{ConversationRepository, IdentityRepository, LibsqlRepository};

#[derive(Debug, Deserialize, Default)]
struct ChatQuery {
    root: Option<String>,
    q: Option<String>,
    notice: Option<String>,
    error: Option<String>,
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

#[derive(Debug)]
struct ChatTemplateContext<'a> {
    query: &'a ChatQuery,
    thread_summaries: &'a [ThreadSummary],
    selected_root: Option<&'a str>,
    thread_messages: &'a [ThreadMessageView],
}

pub(crate) fn protected_routes() -> Router<AppState> {
    Router::new().route("/chat", get(chat_page))
}

async fn chat_page(State(state): State<AppState>, Query(query): Query<ChatQuery>) -> Response {
    let repo = match open_repo(&state.db_path).await {
        Ok(repo) => repo,
        Err(e) => {
            let content = chat_error_view(&e.to_string());

            return Html(
                views::shell(
                    &state,
                    NavItem::Chat,
                    "Chat",
                    state.runtime.is_paused(),
                    &formatters::fuptime(state.runtime.started_at()),
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

    let context = ChatTemplateContext {
        query: &query,
        thread_summaries: &thread_summaries,
        selected_root: selected_root.as_deref(),
        thread_messages: &thread_messages,
    };

    let content = chat_view(&context);

    Html(
        views::shell(
            &state,
            NavItem::Chat,
            "Chat",
            state.runtime.is_paused(),
            &formatters::fuptime(state.runtime.started_at()),
            content,
        )
        .into_string(),
    )
    .into_response()
}

fn chat_error_view(error: &str) -> Markup {
    html! {
        (partials::page_header("Chat", "Conversation inspector"))
        (partials::notice(error, "error"))
    }
}

fn chat_view(context: &ChatTemplateContext<'_>) -> Markup {
    html! {
        (partials::page_header("Chat", "Conversation inspector and manual override reply controls"))

        (partials::notices(context.query.notice.as_deref(), context.query.error.as_deref()))

        article {
            (partials::search_bar(
                "/chat",
                "q",
                context.query.q.as_deref().unwrap_or(""),
                "Search by handle, DID, or message content",
                "Search",
            ))
        }

        section class="split" {
            aside {
                article {
                    @if context.thread_summaries.is_empty() {
                        p class="muted" { "No conversation threads found." }
                    } @else {
                        nav {
                            @for thread in context.thread_summaries {
                                (partials::thread_link(
                                    &chat_thread_href(&thread.root_uri, context.query.q.as_deref()),
                                    context.selected_root == Some(thread.root_uri.as_str()),
                                    &thread.handle,
                                    &thread.last_seen,
                                    &thread.preview,
                                    thread.message_count,
                                ))
                            }
                        }
                    }
                }
            }

            section {
                @if let Some(root_uri) = context.selected_root {
                    article {
                        header {
                            small class="muted" { "Thread Root" }
                            p class="mono" { (root_uri) }
                        }

                        @if context.thread_messages.is_empty() {
                            p class="muted" { "No messages in this thread." }
                        } @else {
                            section class="grid" {
                                @for message in context.thread_messages {
                                    article class="chat-message" data-role={(if message.role == Role::User { "user" } else { "model" })} {
                                        header {
                                            strong { (message.author.as_str()) }
                                            small class="muted" { (message.timestamp.as_str()) }
                                            @if let Some(latency) = message.latency.as_deref() {
                                                small { (latency) }
                                            }
                                        }
                                        p { (message.content.as_str()) }
                                    }
                                }
                            }
                        }
                    }

                    article {
                        header { "Manual Override Reply" }
                        form method="post" action="/admin/reply" {
                            input type="hidden" name="root_uri" value=(root_uri);
                            textarea name="text" rows="3" maxlength="300" placeholder="Reply in this thread as the bot" required {};
                            button type="submit" { "Send Reply" }
                        }

                        form
                            method="post"
                            action="/admin/clear-thread"
                            hx-boost="true"
                            hx-confirm="Clear all context for this thread?"
                            data-confirm-modal
                            data-confirm-title="Clear thread context?"
                            data-confirm-label="Clear context"
                            data-confirm-cancel-label="Keep thread"
                            data-confirm-variant="danger" {
                            input type="hidden" name="root_uri" value=(root_uri);
                            button type="submit" class="secondary" { "Clear Thread Context" }
                        }
                    }
                } @else {
                    article {
                        p class="muted" { "Select a thread to inspect messages and post manual replies." }
                    }
                }
            }
        }
    }
}

fn chat_thread_href(root_uri: &str, search: Option<&str>) -> String {
    let mut href = format!("/chat?root={}", urlencoding::encode(root_uri));
    if let Some(query) = search.filter(|value| !value.trim().is_empty()) {
        href.push_str("&q=");
        href.push_str(&urlencoding::encode(query));
    }
    href
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
            None => formatters::shorten(&handle_did, 28),
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
            preview: formatters::shorten(&preview_message.content, 90),
            last_seen: formatters::rel_event(last_activity_us),
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
        let timestamp = formatters::parse_rfc3339(&message.created_at);
        let timestamp_label = formatters::ftime(&message.created_at);

        let author = match message.role {
            Role::Model => "@thunderbot".to_string(),
            Role::User => {
                if let Some(cached) = handle_cache.get(&message.author_did) {
                    cached.clone()
                } else {
                    let handle = match repo.get_by_did(&message.author_did).await? {
                        Some(identity) => format!("@{}", identity.handle),
                        None => formatters::shorten(&message.author_did, 28),
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
                    if millis >= 0 { Some(format!("thinking {}", formatters::latency(millis as u64))) } else { None }
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
