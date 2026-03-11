use chrono::{DateTime, TimeZone, Utc};
use std::time::Instant;

pub fn latency(ms: u64) -> String {
    if ms < 1_000 { format!("{}ms", ms) } else { format!("{:.1}s", ms as f64 / 1_000.0) }
}

pub fn shorten(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_string();
    }

    value.chars().take(max.saturating_sub(3)).chain("...".chars()).collect()
}

pub fn non_empty_or_missing(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() { "(not set)".to_string() } else { trimmed.to_string() }
}

pub fn secret_status(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        "missing".to_string()
    } else {
        format!("set ({} chars)", trimmed.chars().count())
    }
}

pub fn parse_rfc3339(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|timestamp| timestamp.with_timezone(&Utc))
}

pub fn ftime(value: &str) -> String {
    parse_rfc3339(value)
        .map(|timestamp| timestamp.format("%H:%M:%S").to_string())
        .unwrap_or_else(|| value.to_string())
}

pub fn rel_event(time_us: i64) -> String {
    if time_us <= 0 {
        return "waiting".to_string();
    }

    let dt = datetime_from_micros(time_us);
    let Some(dt) = dt else {
        return "unknown".to_string();
    };

    let now = Utc::now();
    let delta = now - dt;

    if delta.num_seconds() < 5 {
        "just now".to_string()
    } else if delta.num_seconds() < 60 {
        format!("{}s ago", delta.num_seconds())
    } else if delta.num_minutes() < 60 {
        format!("{}m ago", delta.num_minutes())
    } else if delta.num_hours() < 24 {
        format!("{}h ago", delta.num_hours())
    } else {
        format!("{}d ago", delta.num_days())
    }
}

pub fn abs_event(time_us: i64) -> String {
    datetime_from_micros(time_us)
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|| "No event timestamp available".to_string())
}

pub fn datetime_from_micros(time_us: i64) -> Option<DateTime<Utc>> {
    let seconds = time_us.div_euclid(1_000_000);
    let micros = time_us.rem_euclid(1_000_000) as u32;
    Utc.timestamp_opt(seconds, micros * 1_000).single()
}

pub fn fcompact(value: i64) -> String {
    if value >= 1_000_000 {
        format!("{:.1}M", value as f64 / 1_000_000.0)
    } else if value >= 1_000 {
        format!("{:.1}K", value as f64 / 1_000.0)
    } else {
        value.to_string()
    }
}

pub fn fuptime(started_at: Instant) -> String {
    let total_secs = started_at.elapsed().as_secs();
    let days = total_secs / 86_400;
    let hours = (total_secs % 86_400) / 3_600;
    let minutes = (total_secs % 3_600) / 60;

    if days > 0 {
        format!("{}d {:02}h {:02}m", days, hours, minutes)
    } else {
        format!("{:02}h {:02}m", hours, minutes)
    }
}
