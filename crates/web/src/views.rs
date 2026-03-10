use maud::{DOCTYPE, Markup, html};
use tnbot_core::db::models::{Conversation, FailedEvent, Identity, Role};

use super::{
    AppState, ChatQuery, ConfigSnapshot, DashboardSnapshot, NavItem, ThreadMessageView, ThreadSummary,
    bot_did_or_fallback, format_compact, format_time, shorten,
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
            body class="auth" {
                main class="container" {
                    article {
                        header {
                            p class="brand" {
                                span class="brand-mark" { "T" }
                                strong { "Thunderbot Control Deck" }
                            }
                            p class="muted" {
                                "Sign in to access dashboard, chat inspector, and manual controls."
                            }
                        }

                        @if let Some(notice_message) = notice {
                            article class="notice" data-tone="success" { (notice_message) }
                        }
                        @if let Some(error_message) = error {
                            article class="notice" data-tone="error" { (error_message) }
                        }

                        form method="post" action="/login" {
                            label for="username" {
                                "Username"
                                input id="username" name="username" autocomplete="username" required;
                            }
                            label for="password" {
                                "Password"
                                input id="password" name="password" type="password" autocomplete="current-password" required;
                            }
                            button type="submit" { "Sign In" }
                        }
                    }
                }
            }
        }
    }
}

pub(super) fn shell(
    state: &AppState, active: NavItem, page_title: &str, paused: bool, uptime: &str, content: Markup,
) -> Markup {
    let status = if paused { "paused" } else { "live" };
    let status_text = if paused { "Paused" } else { "Live" };

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
                div class="app-shell" {
                    header class="app-header" {
                        p class="brand" {
                            span class="brand-mark" { "T" }
                            strong { "Thunderbot" }
                            span class="status" data-status=(status) { (status_text) }
                        }
                        p {
                            "uptime "
                            strong { (uptime) }
                            " | "
                            span class="mono" { (chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")) }
                        }
                    }

                    aside class="app-nav" {
                        nav {
                            ul {
                                li { (nav_link("/dashboard", "Status", active == NavItem::Dashboard)) }
                                li { (nav_link("/logs", "Logs", active == NavItem::Logs)) }
                                li { (nav_link("/chat", "Chat", active == NavItem::Chat)) }
                                li { (nav_link("/config", "Config", active == NavItem::Config)) }
                            }
                        }

                        footer {
                            small {
                                "bot: " (state.settings.bot.name.as_str()) br;
                                span class="mono" { (shorten(&bot_did_or_fallback(&state.settings), 36)) }
                            }
                            form method="post" action="/logout" {
                                button type="submit" class="secondary" { "Sign Out" }
                            }
                        }
                    }

                    main class="app-main" {
                        (content)
                    }
                }
            }
        }
    }
}

pub(super) fn live_status_cards(snapshot: &DashboardSnapshot) -> Markup {
    let queue_tone = if snapshot.queue_depth > 50 { "warn" } else { "ok" };

    html! {
        section class="grid" {
            article {
                header { "Last Jetstream Event" }
                h3 { (snapshot.last_event_label.as_str()) }
                small class="mono muted" { (snapshot.last_event_absolute.as_str()) }
            }

            article data-tone=(queue_tone) {
                header { "Processing Queue Depth" }
                h3 { (snapshot.queue_depth) }
                small class="muted" { (format!("{} pending embedding jobs", snapshot.pending_embeddings)) }
            }

            article {
                header { "Monthly Token Usage" }
                h3 { (format_compact(snapshot.monthly_tokens)) }
                small class="muted" { "Estimated from persisted model responses" }
            }

            article {
                header { "Pipeline" }
                h3 { (format!("{} ok / {} fail", snapshot.processed_events, snapshot.failed_events)) }
                small class="muted" { (format!("last model latency: {} ms", snapshot.last_model_latency_ms)) }
            }
        }

        section class="grid" {
            article {
                header { "Conversations" }
                h3 { (snapshot.conversation_count) }
            }
            article {
                header { "Identities" }
                h3 { (snapshot.identity_count) }
            }
        }
    }
}

pub(super) fn dashboard_content(
    notice: Option<&str>, error: Option<&str>, snapshot: &DashboardSnapshot, dry_run: bool,
    conversations: &[Conversation], identities: &[Identity],
) -> Markup {
    html! {
        header {
            h1 { "Live Status Dashboard" }
            p class="muted" { "Jetstream pipeline telemetry, controls, and database state" }
        }

        @if let Some(notice_message) = notice {
            article class="notice" data-tone="success" { (notice_message) }
        }
        @if let Some(error_message) = error {
            article class="notice" data-tone="error" { (error_message) }
        }

        section id="live-status" hx-get="/dashboard/live" hx-trigger="every 5s" hx-swap="innerHTML" {
            (live_status_cards(snapshot))
        }

        section class="grid" {
            article {
                header { "Admin Controls" }
                form method="post" action="/admin/pause" {
                    input type="hidden" name="paused" value={(if snapshot.paused {"false"} else {"true"})};
                    button type="submit" { (if snapshot.paused { "Resume Bot" } else { "Pause Bot" }) }
                }
                small class="muted" { "Pause immediately acknowledges mention events without generating replies." }
            }

            article {
                header { "Manual Broadcast" }
                form method="post" action="/admin/broadcast" {
                    textarea name="text" rows="4" maxlength="300" placeholder="Post as the bot account..." required {};
                    button type="submit" { "Send Broadcast" }
                }
                small class="muted" {
                    @if dry_run {
                        "Dry-run mode is active; broadcasts are preview-only."
                    } @else {
                        "Posts are published through the configured Bluesky credentials."
                    }
                }
            }
        }

        article {
            header { "Recent Conversation Rows" }
            div class="table-wrap" {
                table {
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
                            tr { td colspan="5" class="muted" { "No conversation rows yet." } }
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

        article {
            header { "Identity Map" }
            div class="table-wrap" {
                table {
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
                            tr { td colspan="4" class="muted" { "No cached identities yet." } }
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

pub(super) fn logs_content(
    snapshot: &DashboardSnapshot, search: Option<&str>, failed_events: &[FailedEvent],
) -> Markup {
    html! {
        header {
            h1 { "Logs" }
            p class="muted" { "Operational failures and pipeline diagnostics" }
        }

        section class="grid" {
            article { header { "Processed" } h3 { (snapshot.processed_events) } }
            article { header { "Failed" } h3 { (snapshot.failed_events) } }
            article { header { "Queue Depth" } h3 { (snapshot.queue_depth) } }
            article { header { "Last Model Latency" } h3 { (format!("{}ms", snapshot.last_model_latency_ms)) } }
        }

        article {
            form method="get" action="/logs" {
                div class="grid" {
                    input type="search" name="q" value=(search.unwrap_or_default()) placeholder="Filter by post URI, error text, or payload";
                    button type="submit" { "Filter" }
                }
            }
        }

        article {
            header { "Recent Failed Events" }
            div class="table-wrap" {
                table {
                    thead {
                        tr {
                            th { "Time" }
                            th { "Post URI" }
                            th { "Attempts" }
                            th { "Error" }
                        }
                    }
                    tbody {
                        @if failed_events.is_empty() {
                            tr { td colspan="4" class="muted" { "No failed events recorded." } }
                        } @else {
                            @for event in failed_events {
                                tr {
                                    td class="mono" { (format_time(&event.last_tried)) }
                                    td class="mono" { (shorten(&event.post_uri, 64)) }
                                    td { (event.attempts) }
                                    td { (shorten(&event.error, 120)) }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

pub(super) fn config_content(config: &ConfigSnapshot) -> Markup {
    html! {
        header {
            h1 { "Config" }
            p class="muted" { "Read-only runtime and provider configuration snapshot" }
        }

        section class="grid" {
            article { (config_card("Bot", vec![
                ("Name".to_string(), config.bot_name.clone(), false),
                ("DID".to_string(), config.bot_did.clone(), true),
                ("Dry Run".to_string(), if config.dry_run { "enabled" } else { "disabled" }.to_string(), false),
            ])) }

            article { (config_card("Bluesky", vec![
                ("Handle".to_string(), config.bluesky_handle.clone(), false),
                ("PDS Host".to_string(), config.bluesky_pds_host.clone(), true),
                ("App Password".to_string(), config.bluesky_password_status.clone(), false),
            ])) }

            article { (config_card("AI", vec![
                ("Model".to_string(), config.ai_model.clone(), false),
                ("Base URL".to_string(), config.ai_base_url.clone(), true),
                ("API Key".to_string(), config.ai_api_key_status.clone(), false),
            ])) }

            article { (config_card("Storage + Logs", vec![
                ("Database Path".to_string(), config.db_path.clone(), true),
                ("Log Level".to_string(), config.logging_level.clone(), false),
                ("Log Format".to_string(), config.logging_format.clone(), false),
            ])) }

            article { (config_card("Memory", vec![
                ("Enabled".to_string(), if config.memory_enabled { "yes" } else { "no" }.to_string(), false),
                ("TTL".to_string(), format!("{} days", config.memory_ttl_days), false),
                (
                    "Summary TTL".to_string(),
                    format!("{} days", config.memory_consolidation_ttl_days),
                    false,
                ),
                (
                    "Dedup Threshold".to_string(),
                    format!("{:.3}", config.memory_dedup_threshold),
                    false,
                ),
            ])) }

            article { (config_card("Web Control Deck", vec![
                ("Bind Address".to_string(), config.web_bind_addr.clone(), true),
                ("Username".to_string(), config.web_username.clone(), false),
                ("Password Mode".to_string(), config.web_password_mode.clone(), false),
            ])) }
        }
    }
}

pub(super) fn chat_error_content(error: &str) -> Markup {
    html! {
        header {
            h1 { "Chat" }
            p class="muted" { "Conversation inspector" }
        }
        article class="notice" data-tone="error" {
            "Failed to load conversation data: " (error)
        }
    }
}

pub(super) fn chat_content(
    query: &ChatQuery, thread_summaries: &[ThreadSummary], selected_root: Option<&str>,
    thread_messages: &[ThreadMessageView],
) -> Markup {
    html! {
        header {
            h1 { "Chat" }
            p class="muted" { "Conversation inspector and manual override reply controls" }
        }

        @if let Some(notice_message) = query.notice.as_deref() {
            article class="notice" data-tone="success" { (notice_message) }
        }
        @if let Some(error_message) = query.error.as_deref() {
            article class="notice" data-tone="error" { (error_message) }
        }

        article {
            form method="get" action="/chat" {
                div class="grid" {
                    input type="search" name="q" value=(query.q.as_deref().unwrap_or("")) placeholder="Search by handle, DID, or message content";
                    button type="submit" { "Search" }
                }
            }
        }

        section class="split" {
            aside {
                article {
                    @if thread_summaries.is_empty() {
                        p class="muted" { "No conversation threads found." }
                    } @else {
                        nav {
                            @for thread in thread_summaries {
                                (thread_link(
                                    &super::chat_thread_href(&thread.root_uri, query.q.as_deref()),
                                    selected_root == Some(thread.root_uri.as_str()),
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
                @if let Some(root_uri) = selected_root {
                    article {
                        header {
                            small class="muted" { "Thread Root" }
                            p class="mono" { (root_uri) }
                        }

                        @if thread_messages.is_empty() {
                            p class="muted" { "No messages in this thread." }
                        } @else {
                            section class="grid" {
                                @for message in thread_messages {
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

                        form method="post" action="/admin/clear-thread" onsubmit="return confirm('Clear all context for this thread?')" {
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

fn nav_link(href: &str, label: &str, active: bool) -> Markup {
    if active {
        html! { a href=(href) aria-current="page" { (label) } }
    } else {
        html! { a href=(href) { (label) } }
    }
}

fn thread_link(href: &str, active: bool, handle: &str, last_seen: &str, preview: &str, message_count: usize) -> Markup {
    if active {
        html! {
            a href=(href) class="thread-link" aria-current="page" {
                strong { (handle) }
                small class="mono muted" { (last_seen) }
                p { (preview) }
                small class="muted" { (format!("{} messages", message_count)) }
            }
        }
    } else {
        html! {
            a href=(href) class="thread-link" {
                strong { (handle) }
                small class="mono muted" { (last_seen) }
                p { (preview) }
                small class="muted" { (format!("{} messages", message_count)) }
            }
        }
    }
}

fn config_card(title: &str, rows: Vec<(String, String, bool)>) -> Markup {
    html! {
        header { (title) }
        dl class="kv" {
            @for (label, value, is_mono) in rows {
                dt class="muted" { (label.as_str()) }
                @if is_mono {
                    dd class="mono" { (value.as_str()) }
                } @else {
                    dd { (value.as_str()) }
                }
            }
        }
    }
}
