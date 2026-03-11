use super::{AppState, ChatQuery, ConfigSnapshot, DashboardSnapshot, NavItem, ThreadMessageView, ThreadSummary};
use super::{bot_did_or_fallback, formatters, partials};
use maud::{DOCTYPE, Markup, html};
use tnbot_core::db::models::{Conversation, FailedEvent, Identity, Role};

pub(super) fn login_page(error: Option<&str>, notice: Option<&str>) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" data-theme="dark" {
            (partials::head("Thunderbot Login"))
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

                        (partials::notices(notice, error))

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
            (partials::head_with_scripts(&format!("{} - Thunderbot", page_title)))
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
                                @for item in NavItem::items() {
                                    li { (nav_link(item, active)) }
                                }
                            }
                        }

                        footer {
                            small {
                                "bot: " (state.settings.bot.name.as_str()) br;
                                span class="mono" { (formatters::shorten(&bot_did_or_fallback(&state.settings), 36)) }
                            }
                            form method="post" action="/logout" {
                                button type="submit" class="secondary" { "Sign Out" }
                            }
                        }
                    }

                    main class="app-main" { (content) }
                }
            }
        }
    }
}

pub(super) fn live_status_cards(snapshot: &DashboardSnapshot) -> Markup {
    let queue_tone = if snapshot.queue_depth > 50 { "warn" } else { "ok" };

    html! {
        section class="grid" {
            (partials::stat_card(
                "Last Jetstream Event",
                snapshot.last_event_label.as_str(),
                Some(snapshot.last_event_absolute.as_str()),
            ))

            (partials::stat_card_with_tone(
                "Processing Queue Depth",
                &snapshot.queue_depth.to_string(),
                Some(&format!("{} pending embedding jobs", snapshot.pending_embeddings)),
                queue_tone,
            ))

            (partials::stat_card(
                "Monthly Token Usage",
                &formatters::fcompact(snapshot.monthly_tokens),
                Some("Estimated from persisted model responses"),
            ))

            (partials::stat_card(
                "Pipeline",
                &format!("{} ok / {} fail", snapshot.processed_events, snapshot.failed_events),
                Some(&format!("last model latency: {} ms", snapshot.last_model_latency_ms)),
            ))
        }

        section class="grid" {
            (partials::stat_card("Conversations", &snapshot.conversation_count.to_string(), None))
            (partials::stat_card("Identities", &snapshot.identity_count.to_string(), None))
        }
    }
}

pub(super) fn dashboard_content(
    notice: Option<&str>, error: Option<&str>, snapshot: &DashboardSnapshot, dry_run: bool,
    conversations: &[Conversation], identities: &[Identity],
) -> Markup {
    let conversation_rows: Vec<Vec<(String, bool)>> = conversations
        .iter()
        .map(|row| {
            vec![
                (formatters::ftime(&row.created_at), true),
                (
                    match row.role {
                        Role::User => "user",
                        Role::Model => "model",
                    }
                    .to_string(),
                    false,
                ),
                (formatters::shorten(&row.author_did, 34), true),
                (formatters::shorten(&row.root_uri, 56), true),
                (formatters::shorten(&row.content, 86), false),
            ]
        })
        .collect();

    let identity_rows: Vec<Vec<(String, bool)>> = identities
        .iter()
        .map(|identity| {
            vec![
                (formatters::shorten(&identity.did, 44), true),
                (identity.handle.clone(), false),
                (identity.display_name.as_deref().unwrap_or("-").to_string(), false),
                (formatters::ftime(&identity.last_updated), true),
            ]
        })
        .collect();

    html! {
        (partials::page_header("Live Status Dashboard", "Jetstream pipeline telemetry, controls, and database state"))
        (partials::notices(notice, error))

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
            (partials::data_table(
                &["Time", "Role", "Author", "Thread Root", "Content"],
                &conversation_rows,
                "No conversation rows yet.",
            ))
        }

        article {
            header { "Identity Map" }
            (partials::data_table(
                &["DID", "Handle", "Display Name", "Last Updated"],
                &identity_rows,
                "No cached identities yet.",
            ))
        }
    }
}

pub(super) fn logs_content(
    snapshot: &DashboardSnapshot, search: Option<&str>, failed_events: &[FailedEvent],
) -> Markup {
    let event_rows: Vec<Vec<(String, bool)>> = failed_events
        .iter()
        .map(|event| {
            vec![
                (formatters::ftime(&event.last_tried), true),
                (formatters::shorten(&event.post_uri, 64), true),
                (event.attempts.to_string(), false),
                (formatters::shorten(&event.error, 120), false),
            ]
        })
        .collect();

    html! {
        (partials::page_header("Logs", "Operational failures and pipeline diagnostics"))

        section class="grid" {
            (partials::stat_card("Processed", &snapshot.processed_events.to_string(), None))
            (partials::stat_card("Failed", &snapshot.failed_events.to_string(), None))
            (partials::stat_card("Queue Depth", &snapshot.queue_depth.to_string(), None))
            (partials::stat_card("Last Model Latency", &format!("{}ms", snapshot.last_model_latency_ms), None))
        }

        article {
            (partials::search_bar("/logs", "q", search.unwrap_or_default(), "Filter by post URI, error text, or payload", "Filter"))
        }

        article {
            header { "Recent Failed Events" }
            (partials::data_table(
                &["Time", "Post URI", "Attempts", "Error"],
                &event_rows,
                "No failed events recorded.",
            ))
        }
    }
}

pub(super) fn config_content(config: &ConfigSnapshot) -> Markup {
    html! {
        (partials::page_header("Config", "Read-only runtime and provider configuration snapshot"))

        section class="grid" {
            article { (partials::config_card("Bot", vec![
                ("Name".to_string(), config.bot_name.clone(), false),
                ("DID".to_string(), config.bot_did.clone(), true),
                ("Dry Run".to_string(), if config.dry_run { "enabled" } else { "disabled" }.to_string(), false),
            ])) }

            article { (partials::config_card("Bluesky", vec![
                ("Handle".to_string(), config.bluesky_handle.clone(), false),
                ("PDS Host".to_string(), config.bluesky_pds_host.clone(), true),
                ("App Password".to_string(), config.bluesky_password_status.clone(), false),
            ])) }

            article { (partials::config_card("AI", vec![
                ("Model".to_string(), config.ai_model.clone(), false),
                ("Base URL".to_string(), config.ai_base_url.clone(), true),
                ("API Key".to_string(), config.ai_api_key_status.clone(), false),
            ])) }

            article { (partials::config_card("Storage + Logs", vec![
                ("Database Path".to_string(), config.db_path.clone(), true),
                ("Log Level".to_string(), config.logging_level.clone(), false),
                ("Log Format".to_string(), config.logging_format.clone(), false),
            ])) }

            article { (partials::config_card("Memory", vec![
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

            article { (partials::config_card("Web Control Deck", vec![
                ("Bind Address".to_string(), config.web_bind_addr.clone(), true),
                ("Username".to_string(), config.web_username.clone(), false),
                ("Password Mode".to_string(), config.web_password_mode.clone(), false),
            ])) }
        }
    }
}

pub(super) fn chat_error_content(error: &str) -> Markup {
    html! {
        (partials::page_header("Chat", "Conversation inspector"))
        (partials::notice(error, "error"))
    }
}

pub(super) fn chat_content(
    query: &ChatQuery, thread_summaries: &[ThreadSummary], selected_root: Option<&str>,
    thread_messages: &[ThreadMessageView],
) -> Markup {
    html! {
        (partials::page_header("Chat", "Conversation inspector and manual override reply controls"))

        (partials::notices(query.notice.as_deref(), query.error.as_deref()))

        article {
            (partials::search_bar("/chat", "q", query.q.as_deref().unwrap_or(""), "Search by handle, DID, or message content", "Search"))
        }

        section class="split" {
            aside {
                article {
                    @if thread_summaries.is_empty() {
                        p class="muted" { "No conversation threads found." }
                    } @else {
                        nav {
                            @for thread in thread_summaries {
                                (partials::thread_link(
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

fn nav_link(item: NavItem, current: NavItem) -> Markup {
    let active = item == current;

    let (href, label) = match item {
        NavItem::Dashboard => ("/dashboard", "Status"),
        NavItem::Logs => ("/logs", "Logs"),
        NavItem::Chat => ("/chat", "Chat"),
        NavItem::Config => ("/config", "Config"),
    };

    if active {
        html! { a href=(href) aria-current="page" { (label) } }
    } else {
        html! { a href=(href) { (label) } }
    }
}
