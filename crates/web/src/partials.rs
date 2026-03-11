use maud::{Markup, html};

/// Shared `<head>` block: charset, viewport, Pico CSS, app stylesheet, and optional extras.
pub(crate) fn head(title: &str) -> Markup {
    html! {
        head {
            meta charset="utf-8";
            meta name="viewport" content="width=device-width, initial-scale=1";
            title { (title) }
            link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/@picocss/pico@2/css/pico.min.css";
            link rel="stylesheet" href="/assets/app.css";
        }
    }
}

/// Shared `<head>` block with HTMX and Alpine.js scripts included.
pub(crate) fn head_with_scripts(title: &str) -> Markup {
    html! {
        head {
            meta charset="utf-8";
            meta name="viewport" content="width=device-width, initial-scale=1";
            title { (title) }
            link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/@picocss/pico@2/css/pico.min.css";
            link rel="stylesheet" href="/assets/app.css";
            script src="https://unpkg.com/htmx.org@1.9.12" defer {}
            script src="https://cdn.jsdelivr.net/npm/alpinejs@3.x.x/dist/cdn.min.js" defer {}
        }
    }
}

/// Page header using Pico's `<hgroup>` for automatic muted subtitle styling.
pub(crate) fn page_header(title: &str, subtitle: &str) -> Markup {
    html! {
        hgroup {
            h2 { (title) }
            p { (subtitle) }
        }
    }
}

/// Feedback notice banner with success/error tone.
pub(crate) fn notice(message: &str, tone: &str) -> Markup {
    html! {
        article class="notice" data-tone=(tone) { (message) }
    }
}

/// Renders optional notice and error messages (common pattern across pages).
pub(crate) fn notices(notice_msg: Option<&str>, error_msg: Option<&str>) -> Markup {
    html! {
        @if let Some(msg) = notice_msg {
            (notice(msg, "success"))
        }
        @if let Some(msg) = error_msg {
            (notice(msg, "error"))
        }
    }
}

/// Search/filter form using Pico's `role="search"` group styling.
pub(crate) fn search_bar(action: &str, name: &str, value: &str, placeholder: &str, button_label: &str) -> Markup {
    html! {
        form method="get" action=(action) role="search" {
            input type="search" name=(name) value=(value) placeholder=(placeholder);
            button type="submit" { (button_label) }
        }
    }
}

/// A single stat card for dashboards: label header, large value, optional detail line.
pub(crate) fn stat_card(label: &str, value: &str, detail: Option<&str>) -> Markup {
    html! {
        article {
            header { (label) }
            h3 { (value) }
            @if let Some(detail_text) = detail {
                small class="muted" { (detail_text) }
            }
        }
    }
}

/// A stat card with an extra data-tone attribute.
pub(crate) fn stat_card_with_tone(label: &str, value: &str, detail: Option<&str>, tone: &str) -> Markup {
    html! {
        article data-tone=(tone) {
            header { (label) }
            h3 { (value) }
            @if let Some(detail_text) = detail {
                small class="muted" { (detail_text) }
            }
        }
    }
}

/// Generic data table with overflow wrapper.
pub(crate) fn data_table(headers: &[&str], rows: &[Vec<(String, bool)>], empty_msg: &str) -> Markup {
    html! {
        div class="overflow-auto" {
            table {
                thead {
                    tr {
                        @for header in headers {
                            th { (*header) }
                        }
                    }
                }
                tbody {
                    @if rows.is_empty() {
                        tr {
                            td colspan=(headers.len().to_string()) class="muted" { (empty_msg) }
                        }
                    } @else {
                        @for row in rows {
                            tr {
                                @for (cell, is_mono) in row {
                                    @if *is_mono {
                                        td class="mono" { (cell.as_str()) }
                                    } @else {
                                        td { (cell.as_str()) }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Config key-value card used on the config page.
pub(crate) fn config_card(title: &str, rows: Vec<(String, String, bool)>) -> Markup {
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

/// A thread link entry in the chat sidebar.
pub(crate) fn thread_link(
    href: &str, active: bool, handle: &str, last_seen: &str, preview: &str, message_count: usize,
) -> Markup {
    let current = if active { Some("page") } else { None };
    html! {
        a href=(href) class="thread-link" aria-current=[current] {
            strong { (handle) }
            small class="mono muted" { (last_seen) }
            p { (preview) }
            small class="muted" { (format!("{} messages", message_count)) }
        }
    }
}
