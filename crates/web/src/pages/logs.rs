use crate::formatters;
use crate::load_recent_failed_events;
use crate::{AppState, DashboardSnapshot, NavItem, dashboard_snapshot_or_default, partials, views};
use axum::Router;
use axum::extract::{Query, State};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use maud::{Markup, html};
use serde::Deserialize;
use tnbot_core::db::models::FailedEvent;

#[derive(Debug, Deserialize, Default)]
struct LogsQuery {
    q: Option<String>,
}

#[derive(Debug)]
struct LogsTemplateContext<'a> {
    snapshot: &'a DashboardSnapshot,
    search: Option<&'a str>,
    failed_events: &'a [FailedEvent],
}

pub(crate) fn protected_routes() -> Router<AppState> {
    Router::new().route("/logs", get(logs_page))
}

async fn logs_page(State(state): State<AppState>, Query(query): Query<LogsQuery>) -> Response {
    let snapshot = dashboard_snapshot_or_default(&state).await;
    let mut failed_events = load_recent_failed_events(&state.db_path, 100).await.unwrap_or_default();

    if let Some(filter) = query
        .q
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_lowercase)
    {
        failed_events.retain(|event| {
            event.post_uri.to_lowercase().contains(&filter)
                || event.error.to_lowercase().contains(&filter)
                || event.event_json.to_lowercase().contains(&filter)
        });
    }

    let context =
        LogsTemplateContext { snapshot: &snapshot, search: query.q.as_deref(), failed_events: &failed_events };
    let body = logs_view(&context);

    Html(views::shell(&state, NavItem::Logs, "Logs", snapshot.paused, &snapshot.uptime, body).into_string())
        .into_response()
}

fn logs_view(context: &LogsTemplateContext<'_>) -> Markup {
    let event_rows: Vec<Vec<(String, bool)>> = context
        .failed_events
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
            (partials::stat_card("Processed", &context.snapshot.processed_events.to_string(), None))
            (partials::stat_card("Failed", &context.snapshot.failed_events.to_string(), None))
            (partials::stat_card("Queue Depth", &context.snapshot.queue_depth.to_string(), None))
            (partials::stat_card("Last Model Latency", &format!("{}ms", context.snapshot.last_model_latency_ms), None))
        }

        article {
            (partials::search_bar(
                "/logs",
                "q",
                context.search.unwrap_or_default(),
                "Filter by post URI, error text, or payload",
                "Filter",
            ))
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
