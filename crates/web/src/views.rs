use maud::{DOCTYPE, Markup, html};
use tnbot_core::db::models::{Conversation, Identity, Role};

use super::{
    AppState, ChatQuery, DashboardSnapshot, NavItem, ThreadMessageView, ThreadSummary, bot_did_or_fallback,
    format_compact, format_time, shorten,
};

pub(super) fn login_page(error: Option<&str>, notice: Option<&str>) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" data-theme="dark" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Thunderbot Login" }
                link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/@picocss/pico@2/css/pico.min.css";
                link rel="stylesheet" href="/assets/app.css";
            }
            body class="login-screen" {
                main class="login-card" {
                    div class="login-brand" {
                        span class="brand-bolt" { "T" }
                        span { "THUNDERBOT CONTROL DECK" }
                    }
                    p class="login-subtitle" {
                        "Sign in to access dashboard, chat inspector, and manual controls."
                    }

                    @if let Some(notice_message) = notice {
                        div class="alert success" { (notice_message) }
                    }
                    @if let Some(error_message) = error {
                        div class="alert error" { (error_message) }
                    }

                    form method="post" action="/login" class="form-row" {
                        label class="field-label" for="username" { "Username" }
                        input id="username" name="username" autocomplete="username" required;

                        label class="field-label" for="password" { "Password" }
                        input id="password" name="password" type="password" autocomplete="current-password" required;

                        button type="submit" { "Sign In" }
                    }
                }
            }
        }
    }
}

pub(super) fn shell(
    state: &AppState, active: NavItem, page_title: &str, paused: bool, uptime: &str, content: Markup,
) -> Markup {
    let status_class = if paused { "status-pill status-paused" } else { "status-pill status-live" };
    let status_text = if paused { "PAUSED" } else { "LIVE" };

    html! {
        (DOCTYPE)
        html lang="en" data-theme="dark" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (page_title) " - Thunderbot" }
                link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/@picocss/pico@2/css/pico.min.css";
                link rel="stylesheet" href="/assets/app.css";
                script src="https://unpkg.com/htmx.org@1.9.12" defer {}
                script src="https://cdn.jsdelivr.net/npm/alpinejs@3.x.x/dist/cdn.min.js" defer {}
            }
            body {
                div class="shell" {
                    header class="topbar" {
                        div class="topbar-left" {
                            span class="brand-bolt" { "T" }
                            span class="topbar-title" { "THUNDERBOT" }
                            span class=(status_class) { (status_text) }
                        }
                        div class="topbar-meta" {
                            span { "uptime " strong { (uptime) } }
                            span class="mono" { (chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")) }
                        }
                    }

                    nav class="sidebar" {
                        section {
                            p class="nav-label" { "Monitor" }
                            a href="/dashboard" class={(if active == NavItem::Dashboard { "nav-item active" } else { "nav-item" })} {
                                "Live Status Dashboard"
                            }
                            a href="/chat" class={(if active == NavItem::Chat { "nav-item active" } else { "nav-item" })} {
                                "Chat Inspector"
                            }
                        }
                        div class="sidebar-foot" {
                            div { "bot: " (state.settings.bot.name.as_str()) }
                            div class="mono" { (shorten(&bot_did_or_fallback(&state.settings), 36)) }
                            form method="post" action="/logout" {
                                button type="submit" class="secondary" { "Sign Out" }
                            }
                        }
                    }

                    main class="main" {
                        (content)
                    }
                }
            }
        }
    }
}

pub(super) fn live_status_cards(snapshot: &DashboardSnapshot) -> Markup {
    let queue_class = if snapshot.queue_depth > 50 { "health-card warning card" } else { "health-card card" };

    html! {
        div class="card-grid grid-4" {
            section class="health-card card" {
                div class="card-label" { "Last Jetstream Event" }
                div class="card-value" { (snapshot.last_event_label.as_str()) }
                div class="card-detail mono" { (snapshot.last_event_absolute.as_str()) }
            }

            section class=(queue_class) {
                div class="card-label" { "Processing Queue Depth" }
                div class="card-value" { (snapshot.queue_depth) }
                div class="card-detail" {
                    (format!("{} pending embedding jobs", snapshot.pending_embeddings))
                }
            }

            section class="health-card card" {
                div class="card-label" { "Monthly Token Usage (Estimate)" }
                div class="card-value" { (format_compact(snapshot.monthly_tokens)) }
                div class="card-detail" {
                    "Derived from persisted model responses"
                }
            }

            section class="health-card card" {
                div class="card-label" { "Pipeline" }
                div class="card-value" {
                    (format!("{} ok / {} fail", snapshot.processed_events, snapshot.failed_events))
                }
                div class="card-detail" {
                    (format!("last model latency: {} ms", snapshot.last_model_latency_ms))
                }
            }
        }

        div class="card" {
            div class="card-grid grid-2" {
                div {
                    div class="card-label" { "Conversations" }
                    div class="card-value" { (snapshot.conversation_count) }
                }
                div {
                    div class="card-label" { "Identities" }
                    div class="card-value" { (snapshot.identity_count) }
                }
            }
        }
    }
}

pub(super) fn dashboard_content(
    notice: Option<&str>, error: Option<&str>, snapshot: &DashboardSnapshot, dry_run: bool,
    conversations: &[Conversation], identities: &[Identity],
) -> Markup {
    html! {
        div class="page-header" {
            h1 { "Live Status Dashboard" }
            div class="subtitle" {
                "Jetstream pipeline telemetry, controls, and database state"
            }
        }

        @if let Some(notice_message) = notice {
            div class="alert success" { (notice_message) }
        }
        @if let Some(error_message) = error {
            div class="alert error" { (error_message) }
        }

        div id="live-status" hx-get="/dashboard/live" hx-trigger="every 5s" hx-swap="innerHTML" {
            (live_status_cards(snapshot))
        }

        div class="card-grid grid-2" {
            section class="card" {
                div class="card-label" { "Admin Controls" }
                form method="post" action="/admin/pause" class="form-row" {
                    input type="hidden" name="paused" value={(if snapshot.paused {"false"} else {"true"})};
                    button type="submit" {
                        (if snapshot.paused { "Resume Bot" } else { "Pause Bot" })
                    }
                }
                p class="footer-note" {
                    "Pause immediately acknowledges mention events without generating replies."
                }
            }

            section class="card" {
                div class="card-label" { "Manual Broadcast" }
                form method="post" action="/admin/broadcast" class="form-row" {
                    textarea name="text" rows="4" maxlength="300" placeholder="Post as the bot account..." required {};
                    button type="submit" { "Send Broadcast" }
                }
                p class="footer-note" {
                    @if dry_run {
                        "Dry-run mode is active; broadcasts are preview-only."
                    } @else {
                        "Posts are published through the configured Bluesky credentials."
                    }
                }
            }
        }

        section class="card" {
            div class="card-label" { "Recent Conversation Rows" }
            div class="table-wrap" {
                table class="data-table" {
                    thead {
                        tr {
                            th { "Time" }
                            th { "Role" }
                            th { "Author" }
                            th { "Thread Root" }
                            th { "Content" }
                        }
                    }
                    tbody {
                        @if conversations.is_empty() {
                            tr {
                                td colspan="5" class="dim" { "No conversation rows yet." }
                            }
                        } @else {
                            @for row in conversations {
                                tr {
                                    td class="mono" { (format_time(&row.created_at)) }
                                    td { (match row.role { Role::User => "user", Role::Model => "model" }) }
                                    td class="mono" { (shorten(&row.author_did, 34)) }
                                    td class="mono" { (shorten(&row.root_uri, 56)) }
                                    td { (shorten(&row.content, 86)) }
                                }
                            }
                        }
                    }
                }
            }
        }

        section class="card" {
            div class="card-label" { "Identity Map" }
            div class="table-wrap" {
                table class="data-table" {
                    thead {
                        tr {
                            th { "DID" }
                            th { "Handle" }
                            th { "Display Name" }
                            th { "Last Updated" }
                        }
                    }
                    tbody {
                        @if identities.is_empty() {
                            tr {
                                td colspan="4" class="dim" { "No cached identities yet." }
                            }
                        } @else {
                            @for identity in identities {
                                tr {
                                    td class="mono" { (shorten(&identity.did, 44)) }
                                    td { (identity.handle.as_str()) }
                                    td { (identity.display_name.as_deref().unwrap_or("-")) }
                                    td class="mono" { (format_time(&identity.last_updated)) }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

pub(super) fn chat_error_content(error: &str) -> Markup {
    html! {
        div class="page-header" {
            h1 { "Chat" }
            div class="subtitle" { "Conversation inspector" }
        }
        div class="alert error" {
            "Failed to load conversation data: " (error)
        }
    }
}

pub(super) fn chat_content(
    query: &ChatQuery, thread_summaries: &[ThreadSummary], selected_root: Option<&str>,
    thread_messages: &[ThreadMessageView],
) -> Markup {
    html! {
        div class="page-header" {
            h1 { "Chat" }
            div class="subtitle" {
                "Conversation inspector and manual override reply controls"
            }
        }

        @if let Some(notice_message) = query.notice.as_deref() {
            div class="alert success" { (notice_message) }
        }
        @if let Some(error_message) = query.error.as_deref() {
            div class="alert error" { (error_message) }
        }

        form method="get" action="/chat" class="card" {
            div class="form-row inline" {
                input type="search" name="q" value=(query.q.as_deref().unwrap_or("")) placeholder="Search by handle, DID, or message content";
                button type="submit" { "Search" }
            }
        }

        div class="split-view" {
            aside class="thread-list" {
                @if thread_summaries.is_empty() {
                    div class="empty-state" { "No conversation threads found." }
                } @else {
                    @for thread in thread_summaries {
                        a
                            class={(if selected_root == Some(thread.root_uri.as_str()) {
                                "thread-item active"
                            } else {
                                "thread-item"
                            })}
                            href={(super::chat_thread_href(&thread.root_uri, query.q.as_deref()))}
                        {
                            div class="thread-head" {
                                span class="thread-handle" { (thread.handle.as_str()) }
                                span class="dim mono" { (thread.last_seen.as_str()) }
                            }
                            div class="thread-preview" { (thread.preview.as_str()) }
                            div class="thread-count" { (format!("{} messages", thread.message_count)) }
                        }
                    }
                }
            }

            section class="thread-detail" {
                @if let Some(root_uri) = selected_root {
                    div class="thread-meta" {
                        div class="dim" { "Thread Root" }
                        div class="mono" { (root_uri) }
                    }

                    div class="chat-panel" {
                        @if thread_messages.is_empty() {
                            div class="empty-state" { "No messages in this thread." }
                        } @else {
                            @for message in thread_messages {
                                div class=(if message.role == Role::User { "chat-msg user" } else { "chat-msg model" }) {
                                    div class="msg-header" {
                                        span { (message.author.as_str()) }
                                        span class="dim" { (message.timestamp.as_str()) }
                                        @if let Some(latency) = message.latency.as_deref() {
                                            span class="badge" { (latency) }
                                        }
                                    }
                                    div { (message.content.as_str()) }
                                }
                            }
                        }
                    }

                    div class="card" {
                        div class="card-label" { "Manual Override Reply" }
                        form method="post" action="/admin/reply" class="form-row" {
                            input type="hidden" name="root_uri" value=(root_uri);
                            textarea name="text" rows="3" maxlength="300" placeholder="Reply in this thread as the bot" required {};
                            button type="submit" { "Send Reply" }
                        }

                        form method="post" action="/admin/clear-thread" class="form-row" onsubmit="return confirm('Clear all context for this thread?')" {
                            input type="hidden" name="root_uri" value=(root_uri);
                            button type="submit" class="secondary" { "Clear Thread Context" }
                        }
                    }
                } @else {
                    div class="empty-state" {
                        "Select a thread to inspect messages and post manual replies."
                    }
                }
            }
        }
    }
}
