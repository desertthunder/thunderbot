//! Handlers for operational controls endpoints.
use super::handlers::WebAppState;
use crate::control::{BlockType, QuietHoursWindow, ReplyLimitsConfig, ResponseStatus};

use axum::{
    extract::{Form, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
};
use chrono::Utc;
use maud;
use serde::Deserialize;
use uuid::Uuid;

pub async fn get_rate_limits(State(state): State<WebAppState>) -> impl IntoResponse {
    let remaining = state.bsky_client.rate_tracker.remaining();
    let limit = state.bsky_client.rate_tracker.limit();
    let usage = state.bsky_client.rate_tracker.usage_percentage();
    let reset_at = state.bsky_client.rate_tracker.reset_at().await;

    Html(
        maud::html! {
            h2 { "Rate Limits" }

            .stats-grid {
                .stat-card {
                    h3 { "Remaining Requests" }
                                    .stat-value { (remaining) }
                }
                .stat-card {
                    h3 { "Rate Limit" }
                                    .stat-value { (limit) }
                }
                @if let Some(pct) = usage {
                    .stat-card {
                        h3 { "Usage" }
                                            .stat-value { (format!("{:.1}%", pct)) }
                        @if pct >= 80.0 {
                            span.status-badge.paused { "Warning" }
                        }
                    }
                }
                .stat-card {
                    h3 { "Reset Time" }
                                    p {
                        @if let Some(reset) = reset_at {
                            (reset.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                        } else {
                            "Unknown"
                        }
                    }
                }
            }
        }
        .into_string(),
    )
}

pub async fn get_rate_limit_history_data(State(state): State<WebAppState>) -> impl IntoResponse {
    match state.db.get_rate_limit_history(24).await {
        Ok(history) => {
            let data: Vec<String> = history
                .iter()
                .map(|s| {
                    format!(
                        r#"{{"time":"{}","remaining":{}}}"#,
                        s.recorded_at.format("%H:%M"),
                        s.limit_remaining
                    )
                })
                .collect();

            (format!("[{}]", data.join(", "))).into_response()
        }
        Err(e) => {
            tracing::error!("Failed to get rate limit history: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn get_event_queue_status(State(state): State<WebAppState>) -> impl IntoResponse {
    let jetstream_state = state.jetstream_state.read().await;
    let queue_depth = jetstream_state.queue_depth;
    let events_per_second = jetstream_state.events_per_second;
    let is_paused = jetstream_state.is_paused;
    let is_backlogged = jetstream_state.is_backlogged();
    drop(jetstream_state);

    Html(
        maud::html! {
            h2 { "Event Queue Status" }

            .stats-grid {
                .stat-card {
                    h3 { "Queue Depth" }
                                    .stat-value { (queue_depth) }
                                    p { "Events awaiting processing" }
                }
                .stat-card {
                    h3 { "Throughput" }
                                    .stat-value { (format!("{:.1}", events_per_second)) }
                                    p { "Events/second" }
                }
                .stat-card {
                    h3 { "Status" }
                    @if is_paused {
                        .status-badge.paused { "Paused" }
                    } else {
                        .status-badge.active { "Running" }
                    }
                }
                @if is_backlogged {
                    .stat-card {
                        h3 { "Alert" }
                                            .status-badge.paused { "Backlogged" }
                                            p { "Queue exceeds threshold" }
                    }
                }
            }

            .actions {
                @if is_paused {
                    form method="post" action="/api/resume-events" {
                        button type="submit" { "Resume Event Processing" }
                    }
                } else {
                    form method="post" action="/api/pause-events" {
                        button type="submit" { "Pause Event Processing" }
                    }
                }
            }
        }
        .into_string(),
    )
}

pub async fn post_pause_events(State(state): State<WebAppState>) -> impl IntoResponse {
    let mut jetstream_state = state.jetstream_state.write().await;
    jetstream_state.set_paused(true);
    drop(jetstream_state);

    tracing::info!("Event processing paused via dashboard");

    Redirect::to("/controls/event-queue").into_response()
}

pub async fn post_resume_events(State(state): State<WebAppState>) -> impl IntoResponse {
    let mut jetstream_state = state.jetstream_state.write().await;
    jetstream_state.set_paused(false);
    drop(jetstream_state);

    tracing::info!("Event processing resumed via dashboard");

    Redirect::to("/controls/event-queue").into_response()
}

pub async fn get_session_info(State(state): State<WebAppState>) -> impl IntoResponse {
    let session_info = state.session_manager.get_session_info().await;

    Html(
        maud::html! {
            h2 { "Session Management" }

            .stats-grid {
                .stat-card {
                    h3 { "DID" }
                                    p { (session_info.did) }
                }
                .stat-card {
                    h3 { "Handle" }
                                    p { (session_info.handle) }
                }
                .stat-card {
                    h3 { "Expires In" }
                    @if let Some(secs) = session_info.expires_in {
                        .stat-value { (format!("{}s", secs)) }
                        @if secs < 900 {
                            span.status-badge.paused { "Expiring Soon" }
                        }
                    } else {
                        p { "Unknown" }
                    }
                }
                .stat-card {
                    h3 { "Last Refresh" }
                    @if let Some(refreshed) = session_info.last_refresh {
                        p { (refreshed.format("%Y-%m-%d %H:%M:%S UTC").to_string()) }
                    } else {
                        p { "Never" }
                    }
                }
            }

            .actions {
                form method="post" action="/api/session/refresh" {
                    button type="submit" { "Force Session Refresh" }
                }
            }
        }
        .into_string(),
    )
}

pub async fn post_refresh_session(State(state): State<WebAppState>) -> Result<Response, StatusCode> {
    match state.session_manager.force_refresh().await {
        Ok(_) => {
            tracing::info!("Session refreshed via dashboard");
            Ok(Redirect::to("/controls/session").into_response())
        }
        Err(e) => {
            tracing::error!("Failed to refresh session: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(Deserialize)]
pub struct ApproveResponseForm {
    id: String,
}

#[derive(Deserialize)]
pub struct EditResponseForm {
    id: String,
    content: String,
}

#[derive(Deserialize)]
pub struct DiscardResponseForm {
    id: String,
}

pub async fn get_pending_responses(State(state): State<WebAppState>) -> impl IntoResponse {
    let responses = match state.db.get_pending_responses().await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("Failed to get pending responses: {}", e);
            Vec::new()
        }
    };

    let preview_enabled = state.agent.is_preview_mode().await;

    Html(maud::html! {
        h2 { "Response Preview Queue" }

        .controls-info {
            p {
                "Preview mode allows you to review and approve responses before they are posted. "
                "During quiet hours, preview mode is automatically enabled."
            }
            @if preview_enabled {
                .status-badge.paused { "Preview Mode Enabled" }
            } else {
                .status-badge.active { "Preview Mode Disabled" }
            }
        }

        @if responses.is_empty() {
            .info {
                p { "No pending responses." }
            }
        } else {
            table {
                thead {
                    tr {
                        th { "Thread" }
                        th { "Response" }
                        th { "Created" }
                        th { "Actions" }
                    }
                }
                tbody {
                    @for response in &responses {
                        tr {
                            td {
                                a href={"https://bsky.app/thread/" (response.thread_uri.replace("at://", "").replace('/', "/")) } {
                                    (response.thread_uri.chars().take(40).collect::<String>())
                                    "..."
                                }
                            }
                            td {
                                (response.content.chars().take(100).collect::<String>())
                                @if response.content.len() > 100 {
                                    "..."
                                }
                            }
                            td {
                                (response.created_at.format("%Y-%m-%d %H:%M").to_string())
                            }
                            td {
                                form method="post" action="/api/preview/approve" style="display: inline;" {
                                    input type="hidden" name="id" value=(response.id);
                                    button type="submit" { "Approve" }
                                }
                                form method="post" action="/api/preview/discard" style="display: inline;" {
                                    input type="hidden" name="id" value=(response.id);
                                    button type="submit" { "Discard" }
                                }
                            }
                        }
                    }
                }
            }
        }

        .actions {
            form method="post" action="/api/preview/toggle" {
                @if preview_enabled {
                    button type="submit" { "Disable Preview Mode" }
                } else {
                    button type="submit" { "Enable Preview Mode" }
                }
            }
        }
    }
    .into_string())
}

pub async fn post_approve_response(
    State(state): State<WebAppState>, Form(form): Form<ApproveResponseForm>,
) -> Result<Response, StatusCode> {
    let item = match state.db.get_response_item(&form.id).await {
        Ok(i) => i,
        Err(e) => {
            tracing::error!("Failed to get response item: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    if let Err(e) = state
        .bsky_client
        .reply_to_post(
            &item.content,
            &item.parent_uri,
            &item.parent_cid,
            &item.root_uri,
            &item.root_cid,
        )
        .await
    {
        tracing::error!("Failed to post approved response: {}", e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    if let Err(e) = state
        .db
        .update_response_status(&form.id, ResponseStatus::Approved)
        .await
    {
        tracing::error!("Failed to update response status: {}", e);
    }

    tracing::info!("Approved and posted response: {}", form.id);

    Ok(Redirect::to("/controls/preview").into_response())
}

pub async fn post_edit_response(
    State(state): State<WebAppState>, Form(form): Form<EditResponseForm>,
) -> Result<Response, StatusCode> {
    if let Err(e) = state.db.update_response_content(&form.id, &form.content).await {
        tracing::error!("Failed to update response content: {}", e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    tracing::info!("Edited response: {}", form.id);

    Ok(Redirect::to("/controls/preview").into_response())
}

pub async fn post_discard_response(
    State(state): State<WebAppState>, Form(form): Form<DiscardResponseForm>,
) -> Result<Response, StatusCode> {
    if let Err(e) = state
        .db
        .update_response_status(&form.id, ResponseStatus::Discarded)
        .await
    {
        tracing::error!("Failed to discard response: {}", e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    tracing::info!("Discarded response: {}", form.id);

    Ok(Redirect::to("/controls/preview").into_response())
}

pub async fn post_toggle_preview_mode(State(state): State<WebAppState>) -> impl IntoResponse {
    let current_mode = state.agent.is_preview_mode().await;
    state.agent.set_preview_mode(!current_mode).await;

    tracing::info!("Preview mode toggled to: {}", !current_mode);

    Redirect::to("/controls/preview").into_response()
}

#[derive(Deserialize)]
pub struct QuietHoursForm {
    id: Option<String>,
    day_of_week: u8,
    start_time: String,
    end_time: String,
    timezone: String,
    enabled: Option<String>,
}

#[derive(Deserialize)]
pub struct DeleteForm {
    id: String,
}

pub async fn get_quiet_hours(State(state): State<WebAppState>) -> impl IntoResponse {
    let windows = match state.db.get_quiet_hours().await {
        Ok(w) => w,
        Err(e) => {
            tracing::error!("Failed to get quiet hours: {}", e);
            Vec::new()
        }
    };

    let day_names = [
        "Sunday",
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
    ];

    Html(maud::html! {
        h2 { "Quiet Hours Configuration" }

        p { "Configure time windows when the bot should not automatically post. During quiet hours, responses are queued for manual approval." }

        .quiet-hours-list {
            @if windows.is_empty() {
                .info {
                    p { "No quiet hours configured." }
                }
            } else {
                @for window in &windows {
                    .quiet-hours-window {
                        h4 {
                            (day_names[window.day_of_week as usize])
                            " - "
                            @if window.enabled {
                                span.status-badge.paused { "Enabled" }
                            } else {
                                span.status-badge.active { "Disabled" }
                            }
                        }
                        p {
                            (window.start_time) " - " (window.end_time)
                            " (" (window.timezone) ")"
                        }
                        form method="post" action="/api/quiet-hours/delete" {
                            input type="hidden" name="id" value=(window.id);
                            button type="submit" { "Delete" }
                        }
                    }
                }
            }
        }

        .add-quiet-hours {
            h3 { "Add Quiet Hours" }
            form method="post" action="/api/quiet-hours/save" {
                label { "Day of Week:" }
                                        select name="day_of_week" {
                                            @for (i, name) in day_names.iter().enumerate() {
                                                option value={(i)} { (name) }
                                            }
                                        }

                label { "Start Time (HH:MM):" }
                                        input type="text" name="start_time" placeholder="22:00" required;

                label { "End Time (HH:MM):" }
                                        input type="text" name="end_time" placeholder="08:00" required;

                label { "Timezone:" }
                                        input type="text" name="timezone" placeholder="America/New_York" value="UTC" required;

                label {
                                            input type="checkbox" name="enabled" value="true";
                                            " Enabled"
                                        }

                button type="submit" { "Add Quiet Hours" }
            }
        }
    }
    .into_string())
}

pub async fn post_save_quiet_hours(
    State(state): State<WebAppState>, Form(form): Form<QuietHoursForm>,
) -> Result<Response, StatusCode> {
    let window = QuietHoursWindow {
        id: form.id.unwrap_or_else(|| Uuid::new_v4().to_string()),
        day_of_week: form.day_of_week,
        start_time: form.start_time,
        end_time: form.end_time,
        timezone: form.timezone,
        enabled: form.enabled.is_some(),
    };

    if let Err(e) = state.db.save_quiet_hours(window).await {
        tracing::error!("Failed to save quiet hours: {}", e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    tracing::info!("Saved quiet hours configuration");

    Ok(Redirect::to("/controls/quiet-hours").into_response())
}

pub async fn post_delete_quiet_hours(
    State(state): State<WebAppState>, Form(form): Form<DeleteForm>,
) -> Result<Response, StatusCode> {
    if let Err(e) = state.db.delete_quiet_hours(&form.id).await {
        tracing::error!("Failed to delete quiet hours: {}", e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    tracing::info!("Deleted quiet hours: {}", form.id);

    Ok(Redirect::to("/controls/quiet-hours").into_response())
}

#[derive(Deserialize)]
pub struct ReplyLimitsForm {
    max_replies_per_thread: u32,
    cooldown_seconds: u64,
    max_replies_per_author_hour: u32,
}

pub async fn get_reply_limits(State(state): State<WebAppState>) -> impl IntoResponse {
    let config = state.policy_enforcer.get_reply_limits().await;

    Html(maud::html! {
        h2 { "Reply Limits Configuration" }

        .limits-config {
            form method="post" action="/api/reply-limits/update" {
                label { "Max Replies Per Thread:" }
                                        input type="number" name="max_replies_per_thread" value=(config.max_replies_per_thread) min="1" required;
                                        small { "Prevent runaway conversations" }

                label { "Cooldown Between Replies (seconds):" }
                                        input type="number" name="cooldown_seconds" value=(config.cooldown_seconds) min="0" required;
                                        small { "Wait time before replying again to same thread" }

                label { "Max Replies Per Author Per Hour:" }
                                        input type="number" name="max_replies_per_author_hour" value=(config.max_replies_per_author_hour) min="1" required;
                                        small { "Rate limit per user" }

                button type="submit" { "Update Limits" }
            }
        }

        .limits-info {
            h3 { "Current Settings" }
            ul {
                li { "Max replies per thread: " (config.max_replies_per_thread) }
                li { "Cooldown: " (config.cooldown_seconds) " seconds" }
                li { "Max per author per hour: " (config.max_replies_per_author_hour) }
                li { "Last updated: " (config.updated_at.format("%Y-%m-%d %H:%M:%S UTC").to_string()) }
            }
        }
    }
    .into_string())
}

pub async fn post_update_reply_limits(
    State(state): State<WebAppState>, Form(form): Form<ReplyLimitsForm>,
) -> Result<Response, StatusCode> {
    let current = state.policy_enforcer.get_reply_limits().await;

    let new_config = ReplyLimitsConfig {
        id: current.id,
        max_replies_per_thread: form.max_replies_per_thread,
        cooldown_seconds: form.cooldown_seconds,
        max_replies_per_author_hour: form.max_replies_per_author_hour,
        updated_at: Utc::now(),
    };

    if let Err(e) = state.policy_enforcer.update_reply_limits(new_config.clone()).await {
        tracing::error!("Failed to update reply limits: {}", e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    tracing::info!("Updated reply limits configuration");

    Ok(Redirect::to("/controls/reply-limits").into_response())
}

#[derive(Deserialize)]
pub struct BlockAuthorForm {
    did: String,
    reason: Option<String>,
    block_type: Option<String>,
}

#[derive(Deserialize)]
pub struct UnblockForm {
    did: String,
}

pub async fn get_blocklist(State(state): State<WebAppState>) -> impl IntoResponse {
    let blocklist = match state.db.get_blocklist().await {
        Ok(b) => b,
        Err(e) => {
            tracing::error!("Failed to get blocklist: {}", e);
            Vec::new()
        }
    };

    Html(
        maud::html! {
            h2 { "Blocklist Management" }

            .blocklist-list {
                @if blocklist.is_empty() {
                    .info {
                        p { "No authors or domains are blocked." }
                    }
                } else {
                    table {
                        thead {
                            tr {
                                th { "DID/Domain" }
                                th { "Type" }
                                th { "Blocked At" }
                                th { "Reason" }
                                th { "Expires" }
                                th { "Actions" }
                            }
                        }
                        tbody {
                            @for entry in &blocklist {
                                tr {
                                    td { (entry.did) }
                                    td {
                                        @if matches!(entry.block_type, BlockType::Author) {
                                            "Author"
                                        } else {
                                            "Domain"
                                        }
                                    }
                                    td { (entry.blocked_at.format("%Y-%m-%d").to_string()) }
                                    td {
                                        @if let Some(ref reason) = entry.reason {
                                            (reason)
                                        } else {
                                            "N/A"
                                        }
                                    }
                                    td {
                                        @if let Some(expires) = entry.expires_at {
                                            (expires.format("%Y-%m-%d").to_string())
                                        } else {
                                            "Never"
                                        }
                                    }
                                    td {
                                        form method="post" action="/api/blocklist/remove" {
                                            input type="hidden" name="did" value=(entry.did);
                                            button type="submit" { "Unblock" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            .add-block {
                h3 { "Add to Blocklist" }
                form method="post" action="/api/blocklist/add" {
                    label { "DID or Domain:" }
                                            input type="text" name="did" placeholder="did:plc:..." required;

                    label { "Type:" }
                                            select name="block_type" {
                                                option value="Author" { "Author (DID)" }
                                                option value="Domain" { "Domain" }
                                            }

                    label { "Reason (optional):" }
                                            input type="text" name="reason" placeholder="Spam, harassment, etc.";

                    button type="submit" { "Block" }
                }
            }

            .blocklist-actions {
                a href="/api/blocklist/export" { "Export Blocklist" }
            }
        }
        .into_string(),
    )
}

pub async fn post_block_author(
    State(state): State<WebAppState>, Form(form): Form<BlockAuthorForm>,
) -> Result<Response, StatusCode> {
    let blocked_by = match state.bsky_client.get_session().await {
        Some(s) => s.did,
        None => return Err(StatusCode::UNAUTHORIZED),
    };

    let block_type = match form.block_type.as_deref() {
        Some("Domain") => BlockType::Domain,
        _ => BlockType::Author,
    };

    let did = form.did.clone();
    let entry = crate::control::BlocklistEntry {
        did,
        blocked_at: Utc::now(),
        blocked_by,
        reason: form.reason,
        expires_at: None,
        block_type,
    };

    if let Err(e) = state.db.add_to_blocklist(entry).await {
        tracing::error!("Failed to add to blocklist: {}", e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    tracing::info!("Added to blocklist: {}", form.did);

    Ok(Redirect::to("/controls/blocklist").into_response())
}

pub async fn post_unblock_author(
    State(state): State<WebAppState>, Form(form): Form<UnblockForm>,
) -> Result<Response, StatusCode> {
    if let Err(e) = state.db.remove_from_blocklist(&form.did).await {
        tracing::error!("Failed to remove from blocklist: {}", e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    tracing::info!("Removed from blocklist: {}", form.did);

    Ok(Redirect::to("/controls/blocklist").into_response())
}

pub async fn get_export_blocklist(State(state): State<WebAppState>) -> Response {
    match state.db.get_blocklist().await {
        Ok(blocklist) => {
            let json = match serde_json::to_string_pretty(&blocklist) {
                Ok(j) => j,
                Err(e) => {
                    tracing::error!("Failed to serialize blocklist: {}", e);
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                }
            };

            use axum::response::AppendHeaders;
            (
                AppendHeaders([
                    ("Content-Type", "application/json"),
                    ("Content-Disposition", "attachment; filename=\"blocklist.json\""),
                ]),
                json,
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("Failed to export blocklist: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn post_import_blocklist(
    State(state): State<WebAppState>, mut multipart: axum::extract::Multipart,
) -> Result<Response, StatusCode> {
    let mut entries = Vec::new();

    while let Ok(Some(field)) = multipart.next_field().await {
        if let Some(name) = field.name()
            && name == "file"
        {
            let data = field.bytes().await.map_err(|_| StatusCode::BAD_REQUEST)?;
            entries = serde_json::from_slice::<Vec<crate::control::BlocklistEntry>>(&data)
                .map_err(|_| StatusCode::BAD_REQUEST)?;
            break;
        }
    }

    if entries.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let count = entries.len();
    for entry in entries {
        if let Err(e) = state.db.add_to_blocklist(entry).await {
            tracing::error!("Failed to import blocklist entry: {}", e);
        }
    }

    tracing::info!("Imported {} blocklist entries", count);

    Ok(Redirect::to("/controls/blocklist").into_response())
}

#[derive(Deserialize)]
pub struct StatusUpdateForm {
    message: String,
}

#[derive(Deserialize)]
pub struct BioUpdateForm {
    status: String,
}

pub async fn get_status_broadcast(State(_state): State<WebAppState>) -> impl IntoResponse {
    Html(maud::html! {
        h2 { "Status Broadcasting" }

        .broadcast-section {
            h3 { "Post Status Update" }
            p { "Post a status message to your Bluesky profile." }
            form method="post" action="/api/status/post" {
                label { "Message:" }
                                        textarea name="message" rows="3" placeholder="Enter status message..." required;

                button type="submit" { "Post Status" }
            }
        }

        .broadcast-section {
            h3 { "Update Bio" }
            p { "Update your Bluesky profile bio with a status indicator." }
            form method="post" action="/api/status/bio" {
                label { "Status:" }
                                        input type="text" name="status" placeholder="Online, Maintenance, Away..." required;

                button type="submit" { "Update Bio" }
            }
            small { "Note: This will prepend a status indicator to your existing bio." }
        }

        .broadcast-section {
            h3 { "Post Maintenance Announcement" }
            p { "Announce scheduled maintenance to your followers." }
            form method="post" action="/api/status/maintenance" {
                label { "Duration (minutes):" }
                                        input type="number" name="duration" value="30" min="1" required;

                button type="submit" { "Post Announcement" }
            }
        }
    }
    .into_string())
}

pub async fn post_status_update(
    State(state): State<WebAppState>, Form(form): Form<StatusUpdateForm>,
) -> Result<Response, StatusCode> {
    if let Err(e) = state.broadcaster.post_status_update(&form.message).await {
        tracing::error!("Failed to post status update: {}", e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    tracing::info!("Posted status update: {}", form.message);

    Ok(Redirect::to("/controls/status-broadcast").into_response())
}

pub async fn post_update_bio(
    State(state): State<WebAppState>, Form(form): Form<BioUpdateForm>,
) -> Result<Response, StatusCode> {
    if let Err(e) = state.broadcaster.update_bio(&form.status).await {
        tracing::error!("Failed to update bio: {}", e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    tracing::info!("Updated bio with status: {}", form.status);

    Ok(Redirect::to("/controls/status-broadcast").into_response())
}

#[derive(Deserialize)]
pub struct MaintenanceForm {
    duration: u64,
}

pub async fn post_maintenance_announcement(
    State(state): State<WebAppState>, Form(form): Form<MaintenanceForm>,
) -> Result<Response, StatusCode> {
    if let Err(e) = state.broadcaster.post_maintenance_announcement(form.duration).await {
        tracing::error!("Failed to post maintenance announcement: {}", e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    tracing::info!("Posted maintenance announcement: {} minutes", form.duration);

    Ok(Redirect::to("/controls/status-broadcast").into_response())
}

#[derive(Deserialize)]
pub struct RetryDlqForm {
    id: String,
}

pub async fn get_dead_letter_queue(State(state): State<WebAppState>) -> impl IntoResponse {
    let dlq_items = match state.db.get_dlq_items(50).await {
        Ok(items) => items,
        Err(e) => {
            tracing::error!("Failed to get DLQ items: {}", e);
            Vec::new()
        }
    };

    Html(
        maud::html! {
            h2 { "Dead Letter Queue" }

            p { "Events that failed processing are stored here for inspection and retry." }

            .dlq-list {
                @if dlq_items.is_empty() {
                    .info {
                        p { "No failed events in the queue." }
                    }
                } else {
                    table {
                        thead {
                            tr {
                                th { "Event ID" }
                                th { "Error" }
                                th { "Retries" }
                                th { "Created" }
                                th { "Actions" }
                            }
                        }
                        tbody {
                            @for item in &dlq_items {
                                tr {
                                    td { (item.id.chars().take(8).collect::<String>()) }
                                    td { (item.error_message.chars().take(50).collect::<String>()) }
                                    td { (item.retry_count) }
                                    td { (item.created_at.format("%Y-%m-%d %H:%M").to_string()) }
                                    td {
                                        form method="post" action="/api/dlq/retry" {
                                            input type="hidden" name="id" value=(item.id);
                                            button type="submit" { "Retry" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            .dlq-actions {
                form method="post" action="/api/dlq/bulk-retry" {
                    button type="submit" { "Bulk Retry All" }
                }
                form method="post" action="/api/dlq/purge" {
                    button type="submit" { "Purge Old Items" }
                }
            }
        }
        .into_string(),
    )
}

pub async fn post_retry_dlq_item(
    State(state): State<WebAppState>, Form(form): Form<RetryDlqForm>,
) -> Result<Response, StatusCode> {
    let Some(ref sender) = state.event_sender else {
        tracing::warn!("Cannot retry DLQ item: event sender not set");
        return Ok(Redirect::to("/controls/dlq").into_response());
    };

    let item = match state.db.get_dlq_item(&form.id).await {
        Ok(i) => i,
        Err(e) => {
            tracing::error!("Failed to get DLQ item: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    match serde_json::from_str::<crate::jetstream::event::JetstreamEvent>(&item.event_json) {
        Ok(event) => {
            if sender.send(event).await.is_err() {
                tracing::error!("Failed to send event for retry");
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }

            if let Err(e) = state.db.remove_from_dlq(&form.id).await {
                tracing::error!("Failed to remove from DLQ: {}", e);
            }

            tracing::info!("Retried DLQ item: {}", form.id);
            Ok(Redirect::to("/controls/dlq").into_response())
        }
        Err(e) => {
            tracing::error!("Failed to parse event JSON: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn post_bulk_retry_dlq(State(state): State<WebAppState>) -> Result<Response, StatusCode> {
    let Some(ref sender) = state.event_sender else {
        tracing::warn!("Cannot bulk retry DLQ: event sender not set");
        return Ok(Redirect::to("/controls/dlq").into_response());
    };

    let items = match state.db.get_dlq_items(100).await {
        Ok(i) => i,
        Err(e) => {
            tracing::error!("Failed to get DLQ items: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let mut retried = 0;
    for item in items {
        if serde_json::from_str::<crate::jetstream::event::JetstreamEvent>(&item.event_json).is_ok()
            && sender
                .send(serde_json::from_str::<crate::jetstream::event::JetstreamEvent>(&item.event_json).unwrap())
                .await
                .is_ok()
        {
            let _ = state.db.remove_from_dlq(&item.id).await;
            retried += 1;
        }
    }

    tracing::info!("Bulk retried {} DLQ items", retried);

    Ok(Redirect::to("/controls/dlq").into_response())
}

pub async fn post_purge_dlq(State(state): State<WebAppState>) -> Result<Response, StatusCode> {
    match state.db.purge_old_dlq_items(10).await {
        Ok(count) => {
            tracing::info!("Purged {} old DLQ items", count);
            Ok(Redirect::to("/controls/dlq").into_response())
        }
        Err(e) => {
            tracing::error!("Failed to purge DLQ: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_controls_landing() -> impl IntoResponse {
    Html(
        maud::html! {
            h2 { "Operational Controls" }
            p { "Monitor and control bot behavior, rate limits, and system health." }

           .stats-grid {
                a.stat-card href="/controls/rate-limits" {
                    h3 { "Rate Limits" }
                    p { "View Bluesky API rate limit usage and history" }
                }
                a.stat-card href="/controls/event-queue" {
                    h3 { "Event Queue" }
                    p { "Monitor event processing status and queue depth" }
                }
                a.stat-card href="/controls/session" {
                    h3 { "Session Management" }
                    p { "View and refresh Bluesky session tokens" }
                }
                a.stat-card href="/controls/preview" {
                    h3 { "Response Preview" }
                    p { "Review and approve pending responses" }
                }
                a.stat-card href="/controls/quiet-hours" {
                    h3 { "Quiet Hours" }
                    p { "Configure time windows for manual approval" }
                }
                a.stat-card href="/controls/reply-limits" {
                    h3 { "Reply Limits" }
                    p { "Set limits on thread replies and cooldowns" }
                }
                a.stat-card href="/controls/blocklist" {
                    h3 { "Blocklist" }
                    p { "Block authors or domains from triggering replies" }
                }
                a.stat-card href="/controls/status-broadcast" {
                    h3 { "Status Broadcast" }
                    p { "Post status updates and maintenance announcements" }
                }
                a.stat-card href="/controls/dlq" {
                    h3 { "Dead Letter Queue" }
                    p { "View and retry failed events" }
                }
            }
        }
        .into_string(),
    )
}
