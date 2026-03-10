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
