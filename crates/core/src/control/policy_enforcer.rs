//! Policy enforcement for quiet hours and reply limits.

use crate::db::DatabaseRepository;
use anyhow::{Context, Result};
use chrono::{Datelike, Utc};
use chrono_tz::Tz;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Enforces operational policies on bot behavior.
pub struct PolicyEnforcer {
    db: Arc<dyn DatabaseRepository>,
    limits: Arc<RwLock<crate::control::ReplyLimitsConfig>>,
}

impl PolicyEnforcer {
    /// Create a new policy enforcer.
    pub fn new(db: Arc<dyn DatabaseRepository>) -> Self {
        let limits = std::thread::spawn({
            let db_clone = db.clone();
            move || {
                tokio::runtime::Runtime::new()
                    .unwrap()
                    .block_on(async { db_clone.get_reply_limits_config().await })
                    .unwrap_or_else(|_| crate::control::ReplyLimitsConfig {
                        id: Uuid::new_v4().to_string(),
                        max_replies_per_thread: 10,
                        cooldown_seconds: 60,
                        max_replies_per_author_hour: 5,
                        updated_at: Utc::now(),
                    })
            }
        })
        .join()
        .unwrap();

        Self { db, limits: Arc::new(RwLock::new(limits)) }
    }

    /// Check if posting is allowed right now (quiet hours check).
    pub async fn can_post_now(&self) -> Result<bool> {
        let windows = self.db.get_quiet_hours().await?;
        let now = Utc::now();

        for window in windows {
            if !window.enabled {
                continue;
            }

            let tz: Tz = window
                .timezone
                .parse()
                .with_context(|| format!("Invalid timezone: {}", window.timezone))?;
            let local_time = now.with_timezone(&tz);

            let day_match = local_time.weekday() as u32 == window.day_of_week as u32;

            if day_match {
                let current_time = local_time.format("%H:%M").to_string();
                if current_time >= window.start_time && current_time <= window.end_time {
                    tracing::debug!("Quiet hours active: {} {}", window.timezone, current_time);
                    return Ok(false);
                }
            }
        }

        Ok(true)
    }

    /// Check if replying to a thread is allowed (reply limits check).
    pub async fn can_reply_to_thread(&self, thread_uri: &str, author_did: &str) -> Result<bool> {
        let limits = self.limits.read().await;

        let thread_replies = self.db.count_replies_in_thread(thread_uri).await?;
        if thread_replies >= limits.max_replies_per_thread as i64 {
            tracing::info!(
                "Reply limit reached for thread: {} (limit: {})",
                thread_uri,
                limits.max_replies_per_thread
            );
            return Ok(false);
        }

        let author_replies = self.db.count_replies_by_author_last_hour(author_did).await?;
        if author_replies >= limits.max_replies_per_author_hour as i64 {
            tracing::info!(
                "Hourly reply limit reached for author: {} (limit: {})",
                author_did,
                limits.max_replies_per_author_hour
            );
            return Ok(false);
        }

        if let Some(last_reply) = self.db.get_last_reply_time(author_did).await? {
            let elapsed = Utc::now() - last_reply;
            if elapsed.num_seconds() < limits.cooldown_seconds as i64 {
                tracing::debug!(
                    "Cooldown active for {}: {}s elapsed, {}s required",
                    author_did,
                    elapsed.num_seconds(),
                    limits.cooldown_seconds
                );
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Update reply limits configuration.
    pub async fn update_reply_limits(&self, config: crate::control::ReplyLimitsConfig) -> Result<()> {
        self.db.update_reply_limits_config(config.clone()).await?;
        let mut limits = self.limits.write().await;
        *limits = config;
        Ok(())
    }

    /// Get current reply limits configuration.
    pub async fn get_reply_limits(&self) -> crate::control::ReplyLimitsConfig {
        self.limits.read().await.clone()
    }
}
