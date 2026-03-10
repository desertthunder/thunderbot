use serde::{Deserialize, Serialize};

/// Represents a conversation/post in the database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: i64,
    pub root_uri: String,
    pub post_uri: String,
    pub parent_uri: Option<String>,
    pub author_did: String,
    pub role: Role,
    pub content: String,
    pub cid: Option<String>,
    pub created_at: String,
}

/// Role of the message author
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Model,
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Role::User => write!(f, "user"),
            Role::Model => write!(f, "model"),
        }
    }
}

impl TryFrom<&str> for Role {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value.to_lowercase().as_str() {
            "user" => Ok(Role::User),
            "model" => Ok(Role::Model),
            _ => Err(format!("Invalid role: {}", value)),
        }
    }
}

/// Identity cache entry mapping DID to handle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    pub did: String,
    pub handle: String,
    pub display_name: Option<String>,
    pub last_updated: String,
}

/// Failed event entry for dead-letter queue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedEvent {
    pub id: i64,
    pub post_uri: String,
    pub event_json: String,
    pub error: String,
    pub attempts: i64,
    pub created_at: String,
    pub last_tried: String,
}

/// Cursor state for Jetstream reconnection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorState {
    pub id: i64,
    pub time_us: i64,
    pub updated: String,
}

/// Parameters for creating a new conversation entry
#[derive(Debug, Clone)]
pub struct CreateConversationParams {
    pub root_uri: String,
    pub post_uri: String,
    pub parent_uri: Option<String>,
    pub author_did: String,
    pub role: Role,
    pub content: String,
    pub cid: Option<String>,
    pub created_at: String,
}

/// Parameters for creating/updating an identity
#[derive(Debug, Clone)]
pub struct CreateIdentityParams {
    pub did: String,
    pub handle: String,
    pub display_name: Option<String>,
    pub last_updated: String,
}

/// Parameters for creating a failed event entry
#[derive(Debug, Clone)]
pub struct CreateFailedEventParams {
    pub post_uri: String,
    pub event_json: String,
    pub error: String,
    pub created_at: String,
    pub last_tried: String,
}

/// Parameters for updating cursor state
#[derive(Debug, Clone)]
pub struct UpdateCursorParams {
    pub time_us: i64,
    pub updated: String,
}

/// Represents a memory embedding in the database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: i64,
    pub conversation_id: i64,
    pub root_uri: String,
    pub content: String,
    /// F32_BLOB(768) stored as Vec<f32>
    pub embedding: Option<Vec<f32>>,
    pub author_did: String,
    pub metadata: Option<serde_json::Value>,
    pub created_at: String,
    pub expires_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// populated by search queries
    pub distance: Option<f64>,
}

/// Represents the status of an embedding job
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EmbeddingStatus {
    Pending,
    Complete,
    Failed,
}

impl std::fmt::Display for EmbeddingStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EmbeddingStatus::Pending => write!(f, "pending"),
            EmbeddingStatus::Complete => write!(f, "complete"),
            EmbeddingStatus::Failed => write!(f, "failed"),
        }
    }
}

impl TryFrom<&str> for EmbeddingStatus {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value.to_lowercase().as_str() {
            "pending" => Ok(EmbeddingStatus::Pending),
            "complete" => Ok(EmbeddingStatus::Complete),
            "failed" => Ok(EmbeddingStatus::Failed),
            _ => Err(format!("Invalid embedding status: {}", value)),
        }
    }
}

/// Represents an embedding job in the database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingJob {
    pub id: i64,
    pub conversation_id: i64,
    pub status: EmbeddingStatus,
    pub attempts: u32,
    pub error: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
}

/// Parameters for creating a new memory entry
#[derive(Debug, Clone)]
pub struct CreateMemoryParams {
    pub conversation_id: i64,
    pub root_uri: String,
    pub content: String,
    pub embedding: Vec<f32>,
    pub author_did: String,
    pub metadata: Option<serde_json::Value>,
    pub created_at: String,
    pub expires_at: Option<String>,
}

/// Parameters for creating a new embedding job
#[derive(Debug, Clone)]
pub struct CreateEmbeddingJobParams {
    pub conversation_id: i64,
    pub created_at: String,
}

/// Parameters for updating an embedding job status
#[derive(Debug, Clone)]
pub struct UpdateEmbeddingJobParams {
    pub id: i64,
    pub status: EmbeddingStatus,
    pub error: Option<String>,
    pub completed_at: Option<String>,
}

/// Parameters for semantic search
#[derive(Debug, Clone)]
pub struct SemanticSearchParams {
    pub query_embedding: Vec<f32>,
    pub top_k: usize,
    pub author_filter: Option<String>,
    pub time_after: Option<String>,
    pub min_score: Option<f64>,
}

/// Search source for hybrid search results
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchSource {
    Semantic,
    Keyword,
    Hybrid,
}

/// Result from a memory search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySearchResult {
    pub memory: Memory,
    /// 0.0 = identical, 1.0 = orthogonal
    pub score: f64,
    pub source: SearchSource,
}
