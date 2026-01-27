//! Shared types for database operations.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationRow {
    pub id: String,
    pub thread_root_uri: String,
    pub post_uri: String,
    pub parent_uri: Option<String>,
    pub author_did: String,
    pub role: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityRow {
    pub did: String,
    pub handle: String,
    pub last_updated: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRow {
    pub did: String,
    pub handle: String,
    pub access_jwt: String,
    pub refresh_jwt: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutedAuthorRow {
    pub did: String,
    pub muted_at: DateTime<Utc>,
    pub muted_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterPresetRow {
    pub id: String,
    pub name: String,
    pub filters_json: String,
    pub created_at: DateTime<Utc>,
    pub created_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityLogRow {
    pub id: String,
    pub action_type: String,
    pub description: String,
    pub thread_uri: Option<String>,
    pub metadata_json: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseStats {
    pub conversation_count: i64,
    pub thread_count: i64,
    pub identity_count: i64,
}
