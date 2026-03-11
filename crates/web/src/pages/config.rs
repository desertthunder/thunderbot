use crate::{AppState, NavItem, dashboard_snapshot_or_default, formatters, partials, views};
use axum::Router;
use axum::extract::State;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use maud::{Markup, html};
use tnbot_core::UnauthorizedPolicy;

#[derive(Debug)]
struct ConfigSnapshot {
    bot_name: String,
    bot_did: String,
    bluesky_handle: String,
    bluesky_pds_host: String,
    bluesky_password_status: String,
    ai_base_url: String,
    ai_model: String,
    ai_api_key_status: String,
    db_path: String,
    logging_level: String,
    logging_format: String,
    memory_enabled: bool,
    memory_ttl_days: u32,
    memory_consolidation_ttl_days: u32,
    memory_dedup_threshold: f64,
    access_allowed_dids: String,
    access_allowed_handles: String,
    access_unauthorized_policy: String,
    web_bind_addr: String,
    web_username: String,
    web_password_mode: String,
    dry_run: bool,
}

#[derive(Debug)]
struct ConfigTemplateContext<'a> {
    config: &'a ConfigSnapshot,
}

pub(crate) fn protected_routes() -> Router<AppState> {
    Router::new().route("/config", get(config_page))
}

async fn config_page(State(state): State<AppState>) -> Response {
    let snapshot = dashboard_snapshot_or_default(&state).await;
    let config = build_config_snapshot(&state);
    let context = ConfigTemplateContext { config: &config };
    let body = config_view(&context);

    Html(
        views::shell(
            &state,
            NavItem::Config,
            "Config",
            snapshot.paused,
            &snapshot.uptime,
            body,
        )
        .into_string(),
    )
    .into_response()
}

fn build_config_snapshot(state: &AppState) -> ConfigSnapshot {
    let settings = &state.settings;
    ConfigSnapshot {
        bot_name: settings.bot.name.clone(),
        bot_did: formatters::non_empty_or_missing(&settings.bot.did),
        bluesky_handle: formatters::non_empty_or_missing(&settings.bluesky.handle),
        bluesky_pds_host: settings.bluesky.pds_host.clone(),
        bluesky_password_status: formatters::secret_status(&settings.bluesky.app_password),
        ai_base_url: settings.ai.base_url.clone(),
        ai_model: settings.ai.model.clone(),
        ai_api_key_status: formatters::secret_status(&settings.ai.api_key),
        db_path: settings.database.path.display().to_string(),
        logging_level: settings.logging.level.clone(),
        logging_format: format!("{:?}", settings.logging.format).to_lowercase(),
        memory_enabled: settings.memory.enabled,
        memory_ttl_days: settings.memory.ttl_days,
        memory_consolidation_ttl_days: settings.memory.consolidation_ttl_days,
        memory_dedup_threshold: settings.memory.dedup_threshold,
        access_allowed_dids: if settings.access.allowed_dids.is_empty() {
            "all authors (no DID whitelist)".to_string()
        } else {
            settings.access.allowed_dids.join(", ")
        },
        access_allowed_handles: if settings.access.allowed_handles.is_empty() {
            "none".to_string()
        } else {
            settings.access.allowed_handles.join(", ")
        },
        access_unauthorized_policy: match settings.access.unauthorized_policy {
            UnauthorizedPolicy::StoreNoReply => "store_no_reply".to_string(),
        },
        web_bind_addr: state.web.bind_addr.to_string(),
        web_username: state.web.username.clone(),
        web_password_mode: if state.web.generated_password {
            "ephemeral (generated at startup)".to_string()
        } else {
            "configured via TNBOT_WEB__PASSWORD".to_string()
        },
        dry_run: state.dry_run,
    }
}

fn config_view(context: &ConfigTemplateContext<'_>) -> Markup {
    let config = context.config;
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

            article { (partials::config_card("Access Policy", vec![
                ("Allowed DIDs".to_string(), config.access_allowed_dids.clone(), true),
                ("Allowed Handles".to_string(), config.access_allowed_handles.clone(), false),
                ("Unauthorized Behavior".to_string(), config.access_unauthorized_policy.clone(), false),
            ])) }

            article { (partials::config_card("Web Operator Access", vec![
                ("Bind Address".to_string(), config.web_bind_addr.clone(), true),
                ("Username".to_string(), config.web_username.clone(), false),
                ("Password Mode".to_string(), config.web_password_mode.clone(), false),
            ])) }
        }
    }
}
