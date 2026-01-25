use base64::prelude::*;
use maud::{Markup, PreEscaped, html};

#[allow(clippy::needless_pass_by_value)]
pub fn base_layout(title: &str, content: Markup) -> Markup {
    html! {
        (PreEscaped(r#"<!DOCTYPE html>"#))
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (title) }
                link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/@picocss/pico@2/css/pico.jade.min.css";
                script src="https://cdn.jsdelivr.net/npm/htmx.org@2.0.8/dist/htmx.min.js" { };
                style {
                    (PreEscaped(r#"
                        .dashboard-grid {
                            display: grid;
                            grid-template-columns: 250px 1fr;
                            min-height: 100vh;
                        }
                        .sidebar {
                            padding: 1rem;
                            background: #f5f5f5;
                            border-right: 1px solid #ddd;
                        }
                        .sidebar nav ul {
                            list-style: none;
                            padding: 0;
                        }
                        .sidebar nav li {
                            margin-bottom: 0.5rem;
                        }
                        .sidebar nav a {
                            display: block;
                            padding: 0.5rem 1rem;
                            color: #333;
                            text-decoration: none;
                            border-radius: 0.25rem;
                        }
                        .sidebar nav a:hover {
                            background: #e0e0e0;
                        }
                        .sidebar nav a.active {
                            background: #007bff;
                            color: white;
                        }
                        .content {
                            padding: 1.5rem;
                        }
                        .stats-grid {
                            display: grid;
                            grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
                            gap: 1rem;
                            margin-bottom: 2rem;
                        }
                        .stat-card {
                            padding: 1.5rem;
                            border: 1px solid #ddd;
                            border-radius: 0.25rem;
                            background: white;
                        }
                        .stat-value {
                            font-size: 2rem;
                            font-weight: bold;
                            color: #007bff;
                        }
                        .chat-container {
                            max-width: 800px;
                            margin: 0 auto;
                        }
                        .chat-bubble {
                            padding: 1rem;
                            border-radius: 0.5rem;
                            margin-bottom: 1rem;
                        }
                        .chat-bubble.user {
                            background: #e3f2fd;
                            margin-left: 2rem;
                        }
                        .chat-bubble.model {
                            background: #f3e5f5;
                            margin-right: 2rem;
                        }
                        .chat-bubble .author {
                            font-size: 0.75rem;
                            color: #666;
                            margin-bottom: 0.25rem;
                        }
                        .chat-bubble .content {
                            margin-bottom: 0.25rem;
                        }
                        .chat-bubble .timestamp {
                            font-size: 0.7rem;
                            color: #999;
                        }
                        .thread-item {
                            padding: 0.75rem;
                            border: 1px solid #ddd;
                            border-radius: 0.25rem;
                            margin-bottom: 0.5rem;
                            cursor: pointer;
                        }
                        .thread-item:hover {
                            background: #f5f5f5;
                        }
                        .status-badge {
                            display: inline-block;
                            padding: 0.25rem 0.5rem;
                            border-radius: 0.25rem;
                            font-size: 0.75rem;
                            font-weight: bold;
                        }
                        .status-badge.active {
                            background: #4caf50;
                            color: white;
                        }
                        .status-badge.paused {
                            background: #ff9800;
                            color: white;
                        }
                    "#))
                }
            }
            body {
                div.dashboard-grid {
                    aside.sidebar {
                        h3 { "ThunderBot" }
                        nav {
                            ul {
                                li { a href="/dashboard" { "Dashboard" } }
                                li { a href="/threads" { "Threads" } }
                                li { a href="/identities" { "Identities" } }
                                li { a href="/admin" { "Admin" } }
                            }
                        }
                        hr;
                        small {
                            "Authenticated"
                        }
                    }
                    main.content {
                        (content)
                    }
                }
            }
        }
    }
}

pub fn landing_page() -> Markup {
    base_layout(
        "ThunderBot - Control Deck",
        html! {
            div.container {
                h1 { "ThunderBot Control Deck" }
                p { "Monitor and control your stateful AI agent for Bluesky" }
                div style="margin: 2rem 0;" {
                    a href="/dashboard" role="button" class="contrast" {
                        "Enter Dashboard"
                    }
                }
                details {
                    summary { "Authentication Required" }
                    p {
                        "The dashboard requires authentication via a Bearer token. "
                        "Set the DASHBOARD_TOKEN environment variable to control access."
                    }
                }
            }
        },
    )
}

pub fn dashboard_page(stats: &crate::web::handlers::DashboardStats) -> Markup {
    base_layout(
        "Dashboard - ThunderBot",
        html! {
            h2 { "Dashboard" }
            div.stats-grid {
                div.stat-card {
                    h3 { "Conversations" }
                    div.stat-value { (stats.conversation_count) }
                    small { "Total messages" }
                }
                div.stat-card {
                    h3 { "Threads" }
                    div.stat-value { (stats.thread_count) }
                    small { "Active conversations" }
                }
                div.stat-card {
                    h3 { "Identities" }
                    div.stat-value { (stats.identity_count) }
                    small { "Cached DIDs" }
                }
            div.stat-card {
                h3 { "Status" }
                span class="status-badge active" { "Running" }
                small hx-get="/api/status" hx-trigger="every 5s" hx-swap="innerHTML" { "Last event: Loading..." }
            }
            }
            h3 { "Recent Activity" }
            p { "View recent threads and manage the bot from the sidebar." }
            div {
                a href="/threads" role="button" class="contrast" { "View Threads" }
            }
        },
    )
}

pub fn threads_list(threads: &[String]) -> Markup {
    base_layout(
        "Threads - ThunderBot",
        html! {
            h2 { "Threads" }
            div {
            @if threads.is_empty() {
                p { "No threads found yet." }
            } @else {
                @for thread in threads {
                    @let thread_id = thread_uri_to_id(thread);
                    @let hx_get = format!("/thread/{}", thread_id);
                    div.thread-item hx-get=(hx_get) hx-target="#thread-detail" {
                        small { "Thread URI:" }
                        div { (truncate_uri(thread)) }
                    }
                }
            }
            }
            div id="thread-detail" style="margin-top: 2rem;" {
                p { "Select a thread above to view details" }
            }
        },
    )
}

pub fn thread_detail(messages: &[crate::db::ConversationRow]) -> Markup {
    html! {
        h3 { "Thread Detail" }
        div.chat-container {
            @for msg in messages {
                @let class_str = format!("chat-bubble {}", msg.role);
                div class=(class_str) {
                    div.author { (format_author(&msg.author_did, &msg.role)) }
                    div.content { (msg.content) }
                    div.timestamp { (msg.created_at.format("%Y-%m-%d %H:%M")) }
                }
            }
        }
    }
}

pub fn identities_list(identities: &[crate::db::IdentityRow]) -> Markup {
    base_layout(
        "Identities - ThunderBot",
        html! {
            h2 { "Cached Identities" }
            table {
                thead {
                    tr {
                        th { "DID" }
                        th { "Handle" }
                        th { "Last Updated" }
                    }
                }
                tbody {
                    @if identities.is_empty() {
                        tr {
                            td colspan="3" { "No identities cached yet." }
                        }
                    } @else {
                        @for identity in identities {
                            tr {
                                td { (identity.did) }
                                td { (identity.handle) }
                                td { (identity.last_updated.format("%Y-%m-%d %H:%M")) }
                            }
                        }
                    }
                }
            }
        },
    )
}

pub fn admin_page() -> Markup {
    base_layout(
        "Admin - ThunderBot",
        html! {
            h2 { "Admin Controls" }
            article {
                header {
                    strong { "Manual Post" }
                }
                form action="/api/post" method="post" {
                    label for="post-text" { "Post Content" }
                    textarea id="post-text" name="text" rows="4" placeholder="What's on your mind?" required;
                    input type="submit" value="Post";
                }
            }
            article {
                header {
                    strong { "Bot Control" }
                }
                div {
                    button hx-post="/api/pause" hx-target="#bot-status" { "Pause Bot" }
                    " "
                    button hx-post="/api/resume" hx-target="#bot-status" { "Resume Bot" }
                }
                div id="bot-status" style="margin-top: 1rem;" {
                    span class="status-badge active" { "Bot Active" }
                }
            }
            article {
                header {
                    strong { "Clear Context" }
                }
                form action="/api/clear-thread" method="post" {
                    label for="thread-uri" { "Thread Root URI" }
                    input type="text" id="thread-uri" name="root_uri" placeholder="at://did:plc:.../app.bsky.feed.post/..." required;
                    input type="submit" value="Clear Thread Context";
                }
            }
        },
    )
}

fn format_author(did: &str, role: &str) -> String {
    if role == "model" {
        "ThunderBot".to_string()
    } else {
        let did_short = did.split(':').next_back().unwrap_or(did);
        format!("User ({})", did_short)
    }
}

fn truncate_uri(uri: &str) -> String {
    if uri.len() > 60 { format!("{}...", &uri[..60]) } else { uri.to_string() }
}

fn thread_uri_to_id(uri: &str) -> String {
    BASE64_STANDARD.encode(uri.as_bytes())
}
