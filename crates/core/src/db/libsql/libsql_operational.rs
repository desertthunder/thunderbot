//! Operational controls repository implementations for libSQL.
//!
//! This module contains implementations for:
//! - Quiet hours (quiet_hours)
//! - Reply limits (reply_limits_config)
//! - Blocklist (blocklist)
//! - Dead letter queue (dead_letter_queue)
//! - Rate limit tracking (rate_limit_history)
//! - Session metadata (session_metadata)

use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Utc};
use chrono_tz::Tz;
use libsql::params;
use uuid::Uuid;

use crate::control::{
    BlockType, BlocklistEntry, DeadLetterItem, QuietHoursWindow, RateLimitSnapshot, ReplyLimitsConfig, SessionMetadata,
};
use crate::db::libsql::LibsqlRepository;
use crate::db::traits::{
    BlocklistRepository, DeadLetterRepository, QuietHoursRepository, RateLimitRepository, ReplyLimitsRepository,
    SessionMetadataRepository,
};

#[async_trait::async_trait]
impl QuietHoursRepository for LibsqlRepository {
    async fn get_quiet_hours(&self) -> Result<Vec<QuietHoursWindow>> {
        let sql = r#"
            SELECT id, day_of_week, start_time, end_time, timezone, enabled
            FROM quiet_hours
            ORDER BY day_of_week, start_time
        "#;

        let conn = self.db.connect()?;
        let mut rows = conn.query(sql, ()).await?;
        let mut results = Vec::new();

        while let Some(row) = rows.next().await? {
            results.push(QuietHoursWindow {
                id: row.get(0)?,
                day_of_week: row.get::<i32>(1)? as u8,
                start_time: row.get(2)?,
                end_time: row.get(3)?,
                timezone: row.get(4)?,
                enabled: row.get::<i32>(5)? == 1,
            });
        }

        Ok(results)
    }

    async fn save_quiet_hours(&self, window: QuietHoursWindow) -> Result<()> {
        let sql = r#"
            INSERT OR REPLACE INTO quiet_hours (id, day_of_week, start_time, end_time, timezone, enabled)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#;

        self.execute(
            sql,
            params![
                window.id.as_str(),
                window.day_of_week,
                window.start_time.as_str(),
                window.end_time.as_str(),
                window.timezone.as_str(),
                if window.enabled { 1i32 } else { 0 },
            ],
        )
        .await
    }

    async fn delete_quiet_hours(&self, id: &str) -> Result<()> {
        let sql = "DELETE FROM quiet_hours WHERE id = ?1";
        self.execute(sql, [id]).await
    }

    async fn is_quiet_hours_active(&self) -> Result<bool> {
        let windows = self.get_quiet_hours().await?;
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
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }
}

#[async_trait::async_trait]
impl ReplyLimitsRepository for LibsqlRepository {
    async fn get_reply_limits_config(&self) -> Result<ReplyLimitsConfig> {
        let sql = r#"
            SELECT id, max_replies_per_thread, cooldown_seconds, max_replies_per_author_hour, updated_at
            FROM reply_limits_config
            LIMIT 1
        "#;

        let conn = self.db.connect()?;
        let mut rows = conn.query(sql, ()).await?;

        match rows.next().await? {
            Some(row) => Ok(ReplyLimitsConfig {
                id: row.get(0)?,
                max_replies_per_thread: row.get(1)?,
                cooldown_seconds: row.get(2)?,
                max_replies_per_author_hour: row.get(3)?,
                updated_at: DateTime::parse_from_rfc3339(&row.get::<String>(4)?)
                    .unwrap()
                    .with_timezone(&Utc),
            }),
            None => Ok(ReplyLimitsConfig {
                id: Uuid::new_v4().to_string(),
                max_replies_per_thread: 10,
                cooldown_seconds: 60,
                max_replies_per_author_hour: 5,
                updated_at: Utc::now(),
            }),
        }
    }

    async fn update_reply_limits_config(&self, config: ReplyLimitsConfig) -> Result<()> {
        let sql = r#"
            INSERT OR REPLACE INTO reply_limits_config
                (id, max_replies_per_thread, cooldown_seconds, max_replies_per_author_hour, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
        "#;

        self.execute(
            sql,
            params![
                config.id.as_str(),
                config.max_replies_per_thread,
                config.cooldown_seconds,
                config.max_replies_per_author_hour,
                config.updated_at.to_rfc3339().as_str(),
            ],
        )
        .await
    }

    async fn count_replies_in_thread(&self, thread_uri: &str) -> Result<i64> {
        let sql = r#"
            SELECT COUNT(*)
            FROM conversations
            WHERE thread_root_uri = ?1 AND role = 'assistant'
        "#;

        let conn = self.db.connect()?;
        let mut rows = conn.query(sql, [thread_uri]).await?;
        let row = rows.next().await?.ok_or_else(|| anyhow::anyhow!("Count failed"))?;

        Ok(row.get(0)?)
    }

    async fn count_replies_by_author_last_hour(&self, author_did: &str) -> Result<i64> {
        let cutoff = Utc::now() - chrono::Duration::hours(1);
        let sql = format!(
            r#"
            SELECT COUNT(*)
            FROM conversations
            WHERE author_did = '{}' AND role = 'assistant' AND created_at >= '{}'
        "#,
            author_did.replace('\'', "''"),
            cutoff.to_rfc3339()
        );

        let conn = self.db.connect()?;
        let mut rows = conn.query(&sql, ()).await?;
        let row = rows.next().await?.ok_or_else(|| anyhow::anyhow!("Count failed"))?;

        Ok(row.get(0)?)
    }

    async fn get_last_reply_time(&self, author_did: &str) -> Result<Option<DateTime<Utc>>> {
        let sql = r#"
            SELECT created_at
            FROM conversations
            WHERE author_did = ?1 AND role = 'assistant'
            ORDER BY created_at DESC
            LIMIT 1
        "#;

        let conn = self.db.connect()?;
        let mut rows = conn.query(sql, [author_did]).await?;

        match rows.next().await? {
            Some(row) => Ok(Some(
                DateTime::parse_from_rfc3339(&row.get::<String>(0)?)
                    .unwrap()
                    .with_timezone(&Utc),
            )),
            None => Ok(None),
        }
    }
}

#[async_trait::async_trait]
impl BlocklistRepository for LibsqlRepository {
    async fn get_blocklist(&self) -> Result<Vec<BlocklistEntry>> {
        let sql = r#"
            SELECT did, blocked_at, blocked_by, reason, expires_at, block_type
            FROM blocklist
            ORDER BY blocked_at DESC
        "#;

        let conn = self.db.connect()?;
        let mut rows = conn.query(sql, ()).await?;
        let mut results = Vec::new();

        while let Some(row) = rows.next().await? {
            let block_type_str: String = row.get(5)?;
            let block_type = match block_type_str.as_str() {
                "Author" => BlockType::Author,
                "Domain" => BlockType::Domain,
                _ => anyhow::bail!("Unknown block type: {}", block_type_str),
            };

            results.push(BlocklistEntry {
                did: row.get(0)?,
                blocked_at: DateTime::parse_from_rfc3339(&row.get::<String>(1)?)
                    .unwrap()
                    .with_timezone(&Utc),
                blocked_by: row.get(2)?,
                reason: row.get(3)?,
                expires_at: row
                    .get::<Option<String>>(4)?
                    .map(|s| DateTime::parse_from_rfc3339(&s).unwrap().with_timezone(&Utc)),
                block_type,
            });
        }

        Ok(results)
    }

    async fn add_to_blocklist(&self, entry: BlocklistEntry) -> Result<()> {
        let sql = r#"
            INSERT OR REPLACE INTO blocklist (did, blocked_at, blocked_by, reason, expires_at, block_type)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#;

        self.execute(
            sql,
            params![
                entry.did.as_str(),
                entry.blocked_at.to_rfc3339().as_str(),
                entry.blocked_by.as_str(),
                entry.reason.as_deref(),
                entry.expires_at.map(|d| d.to_rfc3339()),
                format!("{:?}", entry.block_type).as_str(),
            ],
        )
        .await
    }

    async fn remove_from_blocklist(&self, did: &str) -> Result<()> {
        let sql = "DELETE FROM blocklist WHERE did = ?1";
        self.execute(sql, [did]).await
    }

    async fn is_blocked(&self, did: &str) -> Result<bool> {
        let sql = r#"
            SELECT did, expires_at
            FROM blocklist
            WHERE did = ?1
        "#;

        let conn = self.db.connect()?;
        let mut rows = conn.query(sql, [did]).await?;

        if let Some(row) = rows.next().await? {
            if let Some(expiration_str) = row.get::<Option<String>>(1)? {
                let expiration = DateTime::parse_from_rfc3339(&expiration_str)?.with_timezone(&Utc);
                if expiration < Utc::now() {
                    return Ok(false);
                }
            }
            return Ok(true);
        }

        Ok(false)
    }
}

#[async_trait::async_trait]
impl DeadLetterRepository for LibsqlRepository {
    async fn add_to_dlq(&self, item: DeadLetterItem) -> Result<()> {
        let sql = r#"
            INSERT INTO dead_letter_queue (id, event_json, error_message, retry_count, last_retry_at, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#;

        self.execute(
            sql,
            params![
                item.id.as_str(),
                item.event_json.as_str(),
                item.error_message.as_str(),
                item.retry_count,
                item.last_retry_at.map(|d| d.to_rfc3339()),
                item.created_at.to_rfc3339().as_str(),
            ],
        )
        .await
    }

    async fn get_dlq_items(&self, limit: usize) -> Result<Vec<DeadLetterItem>> {
        let sql = r#"
            SELECT id, event_json, error_message, retry_count, last_retry_at, created_at
            FROM dead_letter_queue
            ORDER BY created_at DESC
            LIMIT ?
        "#;

        let conn = self.db.connect()?;
        let mut rows = conn.query(sql, [limit as i64]).await?;
        let mut results = Vec::new();

        while let Some(row) = rows.next().await? {
            results.push(DeadLetterItem {
                id: row.get(0)?,
                event_json: row.get(1)?,
                error_message: row.get(2)?,
                retry_count: row.get(3)?,
                last_retry_at: row
                    .get::<Option<String>>(4)?
                    .map(|s| DateTime::parse_from_rfc3339(&s).unwrap().with_timezone(&Utc)),
                created_at: DateTime::parse_from_rfc3339(&row.get::<String>(5)?)
                    .unwrap()
                    .with_timezone(&Utc),
            });
        }

        Ok(results)
    }

    async fn get_dlq_item(&self, id: &str) -> Result<DeadLetterItem> {
        let sql = r#"
            SELECT id, event_json, error_message, retry_count, last_retry_at, created_at
            FROM dead_letter_queue
            WHERE id = ?1
        "#;

        let conn = self.db.connect()?;
        let mut rows = conn.query(sql, [id]).await?;
        let row = rows
            .next()
            .await?
            .ok_or_else(|| anyhow::anyhow!("DLQ item not found: {}", id))?;

        Ok(DeadLetterItem {
            id: row.get(0)?,
            event_json: row.get(1)?,
            error_message: row.get(2)?,
            retry_count: row.get(3)?,
            last_retry_at: row
                .get::<Option<String>>(4)?
                .map(|s| DateTime::parse_from_rfc3339(&s).unwrap().with_timezone(&Utc)),
            created_at: DateTime::parse_from_rfc3339(&row.get::<String>(5)?)
                .unwrap()
                .with_timezone(&Utc),
        })
    }

    async fn remove_from_dlq(&self, id: &str) -> Result<()> {
        let sql = "DELETE FROM dead_letter_queue WHERE id = ?1";
        self.execute(sql, [id]).await
    }

    async fn purge_dlq(&self) -> Result<()> {
        let sql = "DELETE FROM dead_letter_queue";
        self.execute(sql, ()).await
    }

    async fn purge_old_dlq_items(&self, days: i64) -> Result<u64> {
        let cutoff = Utc::now() - chrono::Duration::days(days);
        let sql = "DELETE FROM dead_letter_queue WHERE created_at < ?";

        let conn = self.db.connect()?;
        let rows_affected = conn.execute(sql, [cutoff.to_rfc3339().as_str()]).await?;

        Ok(rows_affected as u64)
    }
}

#[async_trait::async_trait]
impl RateLimitRepository for LibsqlRepository {
    async fn save_rate_limit_snapshot(&self, endpoint: String, remaining: i64, reset: DateTime<Utc>) -> Result<()> {
        let sql = r#"
            INSERT INTO rate_limit_history (id, endpoint, limit_remaining, limit_reset, recorded_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
        "#;

        self.execute(
            sql,
            params![
                Uuid::new_v4().to_string().as_str(),
                endpoint.as_str(),
                remaining,
                reset.to_rfc3339().as_str(),
                Utc::now().to_rfc3339().as_str(),
            ],
        )
        .await
    }

    async fn get_rate_limit_history(&self, hours: i64) -> Result<Vec<RateLimitSnapshot>> {
        let cutoff = Utc::now() - chrono::Duration::hours(hours);
        let sql = format!(
            r#"
            SELECT id, endpoint, limit_remaining, limit_reset, recorded_at
            FROM rate_limit_history
            WHERE recorded_at >= '{}'
            ORDER BY recorded_at DESC
        "#,
            cutoff.to_rfc3339()
        );

        let conn = self.db.connect()?;
        let mut rows = conn.query(&sql, ()).await?;
        let mut results = Vec::new();

        while let Some(row) = rows.next().await? {
            results.push(RateLimitSnapshot {
                id: row.get(0)?,
                endpoint: row.get(1)?,
                limit_remaining: row.get(2)?,
                limit_reset: DateTime::parse_from_rfc3339(&row.get::<String>(3)?)
                    .unwrap()
                    .with_timezone(&Utc),
                recorded_at: DateTime::parse_from_rfc3339(&row.get::<String>(4)?)
                    .unwrap()
                    .with_timezone(&Utc),
            });
        }

        Ok(results)
    }
}

#[async_trait::async_trait]
impl SessionMetadataRepository for LibsqlRepository {
    async fn save_session_metadata(&self, metadata: SessionMetadata) -> Result<()> {
        let sql = r#"
            INSERT OR REPLACE INTO session_metadata
                (did, access_jwt_expires_at, refresh_jwt_expires_at, last_refresh_at, force_refresh_before)
            VALUES (?1, ?2, ?3, ?4, ?5)
        "#;

        self.execute(
            sql,
            params![
                metadata.did.as_str(),
                metadata.access_jwt_expires_at.to_rfc3339().as_str(),
                metadata.refresh_jwt_expires_at.to_rfc3339().as_str(),
                metadata.last_refresh_at.map(|d| d.to_rfc3339()),
                metadata.force_refresh_before.map(|d| d.to_rfc3339()),
            ],
        )
        .await
    }

    async fn get_session_metadata(&self, did: &str) -> Result<Option<SessionMetadata>> {
        let sql = r#"
            SELECT did, access_jwt_expires_at, refresh_jwt_expires_at, last_refresh_at, force_refresh_before
            FROM session_metadata
            WHERE did = ?1
        "#;

        let conn = self.db.connect()?;
        let mut rows = conn.query(sql, [did]).await?;

        match rows.next().await? {
            Some(row) => Ok(Some(SessionMetadata {
                did: row.get(0)?,
                access_jwt_expires_at: DateTime::parse_from_rfc3339(&row.get::<String>(1)?)
                    .unwrap()
                    .with_timezone(&Utc),
                refresh_jwt_expires_at: DateTime::parse_from_rfc3339(&row.get::<String>(2)?)
                    .unwrap()
                    .with_timezone(&Utc),
                last_refresh_at: row
                    .get::<Option<String>>(3)?
                    .map(|s| DateTime::parse_from_rfc3339(&s).unwrap().with_timezone(&Utc)),
                force_refresh_before: row
                    .get::<Option<String>>(4)?
                    .map(|s| DateTime::parse_from_rfc3339(&s).unwrap().with_timezone(&Utc)),
            })),
            None => Ok(None),
        }
    }
}
