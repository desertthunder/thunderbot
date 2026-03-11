use crate::{AppState, DashboardSnapshot, NavItem, load_dashboard_snapshot, partials, views};
use crate::{dashboard_snapshot_or_default, formatters, load_identities, load_recent_conversations};
use axum::Router;
use axum::extract::{Query, State};
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use maud::{Markup, html};
use serde::Deserialize;
use tnbot_core::db::models::{Conversation, Identity, Role};

#[derive(Debug, Deserialize, Default)]
struct DashboardQuery {
    notice: Option<String>,
    error: Option<String>,
}

#[derive(Debug)]
struct DashboardTemplateContext<'a> {
    notice: Option<&'a str>,
    error: Option<&'a str>,
    snapshot: &'a DashboardSnapshot,
    dry_run: bool,
    conversations: &'a [Conversation],
    identities: &'a [Identity],
}

pub(crate) fn protected_routes() -> Router<AppState> {
    Router::new()
        .route("/dashboard", get(dashboard_page))
        .route("/dashboard/live", get(dashboard_live))
}

async fn dashboard_page(State(state): State<AppState>, Query(query): Query<DashboardQuery>) -> impl IntoResponse {
    let snapshot = dashboard_snapshot_or_default(&state).await;
    let conversations = load_recent_conversations(&state.db_path, 25).await.unwrap_or_default();
    let identities = load_identities(&state.db_path, 25).await.unwrap_or_default();

    let context = DashboardTemplateContext {
        notice: query.notice.as_deref(),
        error: query.error.as_deref(),
        snapshot: &snapshot,
        dry_run: state.dry_run,
        conversations: &conversations,
        identities: &identities,
    };
    let body = dashboard_view(&context);

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
        Ok(snapshot) => Html(live_status_view(&snapshot).into_string()).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "Failed to refresh dashboard live panel");
            Html("<div class=\"alert error\">Live status unavailable</div>".to_string()).into_response()
        }
    }
}

fn live_status_view(snapshot: &DashboardSnapshot) -> Markup {
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

fn dashboard_view(context: &DashboardTemplateContext<'_>) -> Markup {
    let conversation_rows: Vec<Vec<(String, bool)>> = context
        .conversations
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

    let identity_rows: Vec<Vec<(String, bool)>> = context
        .identities
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
        (partials::notices(context.notice, context.error))

        section id="live-status" hx-get="/dashboard/live" hx-trigger="every 5s" hx-swap="innerHTML" {
            (live_status_view(context.snapshot))
        }

        section class="grid" {
            article {
                header { "Admin Controls" }
                form method="post" action="/admin/pause" {
                    input type="hidden" name="paused" value={(if context.snapshot.paused {"false"} else {"true"})};
                    button type="submit" { (if context.snapshot.paused { "Resume Bot" } else { "Pause Bot" }) }
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
                    @if context.dry_run {
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
