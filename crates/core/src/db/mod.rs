//! Database module for state persistence
//!
//! This module provides:
//! - Database connection management via [`DatabaseManager`]
//! - Schema migrations via embedded SQL files
//! - Repository pattern for database operations
//! - Data models for conversations, identities, and cursor state

pub mod connection;
pub mod migrations;
pub mod models;
pub mod repository;

pub use connection::{DatabaseManager, DatabaseStats};
pub use migrations::{check_migrations, run_migrations};
pub use models::{Conversation, CursorState, FailedEvent, Identity, Role};
pub use models::{CreateConversationParams, CreateFailedEventParams, CreateIdentityParams, UpdateCursorParams};
pub use repository::{
    ConversationRepository, CursorRepository, FailedEventRepository, IdentityRepository, LibsqlRepository,
};
