//! Core types for operational controls.
//!
//! This module defines data structures for response preview, quiet hours,
//! reply limits, blocklist management, dead letter queue, rate limit tracking,
//! and session metadata.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A response queued for approval before posting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseQueueItem {
    /// Unique identifier
    pub id: String,
    /// URI of the thread being replied to
    pub thread_uri: String,
    /// URI of the parent post being replied to
    pub parent_uri: String,
    /// CID of the parent post
    pub parent_cid: String,
    /// URI of the thread root
    pub root_uri: String,
    /// CID of the thread root
    pub root_cid: String,
    /// Generated response content
    pub content: String,
    /// Current approval status
    pub status: ResponseStatus,
    /// When the response was queued
    pub created_at: DateTime<Utc>,
    /// Optional expiration time for auto-approval
    pub expires_at: Option<DateTime<Utc>>,
}

/// Approval status for queued responses.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ResponseStatus {
    /// Awaiting manual approval
    Pending,
    /// Approved and posted (or will be posted)
    Approved,
    /// Edited content pending re-approval
    Edited,
    /// Discarded, will not be posted
    Discarded,
}

/// A time window when the bot should not automatically post.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuietHoursWindow {
    /// Unique identifier
    pub id: String,
    /// Day of week (0=Sunday, 1=Monday, ..., 6=Saturday)
    pub day_of_week: u8,
    /// Start time in HH:MM format (24-hour)
    pub start_time: String,
    /// End time in HH:MM format (24-hour)
    pub end_time: String,
    /// IANA timezone identifier (e.g., "America/New_York")
    pub timezone: String,
    /// Whether this window is currently active
    pub enabled: bool,
}

/// Configuration for reply limits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplyLimitsConfig {
    /// Unique identifier
    pub id: String,
    /// Maximum replies allowed in a single thread
    pub max_replies_per_thread: u32,
    /// Cooldown between replies in seconds
    pub cooldown_seconds: u64,
    /// Maximum replies to same author per hour
    pub max_replies_per_author_hour: u32,
    /// When this config was last updated
    pub updated_at: DateTime<Utc>,
}

/// An entry in the blocklist.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlocklistEntry {
    /// DID of the blocked entity
    pub did: String,
    /// When the block was added
    pub blocked_at: DateTime<Utc>,
    /// DID of user who added the block
    pub blocked_by: String,
    /// Optional reason for the block
    pub reason: Option<String>,
    /// Optional expiration time (NULL = permanent)
    pub expires_at: Option<DateTime<Utc>>,
    /// Type of block (author or domain)
    pub block_type: BlockType,
}

/// Type of blocklist entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BlockType {
    /// Block a specific author (DID)
    Author,
    /// Block all posts from a domain
    Domain,
}

/// An event that failed processing and was sent to DLQ.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadLetterItem {
    /// Unique identifier
    pub id: String,
    /// JSON-serialized event data
    pub event_json: String,
    /// Error message from processing failure
    pub error_message: String,
    /// Number of retry attempts
    pub retry_count: u32,
    /// Last time a retry was attempted
    pub last_retry_at: Option<DateTime<Utc>>,
    /// When the event was sent to DLQ
    pub created_at: DateTime<Utc>,
}

/// A snapshot of rate limit state at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitSnapshot {
    /// Unique identifier
    pub id: String,
    /// API endpoint (e.g., "com.atproto.repo.createRecord")
    pub endpoint: String,
    /// Remaining requests in current window
    pub limit_remaining: i64,
    /// When the rate limit window resets
    pub limit_reset: DateTime<Utc>,
    /// When this snapshot was recorded
    pub recorded_at: DateTime<Utc>,
}

/// Metadata about the current Bluesky session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    /// DID of the authenticated user
    pub did: String,
    /// When the access JWT expires
    pub access_jwt_expires_at: DateTime<Utc>,
    /// When the refresh JWT expires
    pub refresh_jwt_expires_at: DateTime<Utc>,
    /// Last time the session was refreshed
    pub last_refresh_at: Option<DateTime<Utc>>,
    /// Force refresh before this time (for proactive refresh)
    pub force_refresh_before: Option<DateTime<Utc>>,
}
