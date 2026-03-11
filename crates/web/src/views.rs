use super::{AppState, NavItem};
use super::{bot_did_or_fallback, formatters, partials};
use maud::{DOCTYPE, Markup, html};

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
                (partials::confirm_modal_host())
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
