//! Search and export repository operations.

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use crate::db::types::ConversationRow;

/// Repository for search and export operations.
#[async_trait]
pub trait SearchRepository: Send + Sync {
    /// Full-text search conversations with optional filters.
    async fn search_conversations(
        &self,
        query: &str,
        author_filter: Option<&str>,
        role_filter: Option<&str>,
        date_from: Option<DateTime<Utc>>,
        date_to: Option<DateTime<Utc>>,
        limit: usize,
    ) -> Result<Vec<ConversationRow>>;

    /// Export all conversations.
    async fn export_all_conversations(&self) -> Result<Vec<ConversationRow>>;

    /// Export all messages in a single thread.
    async fn export_thread(&self, thread_root_uri: &str) -> Result<Vec<ConversationRow>>;

    /// Get threads with minimum message count.
    async fn get_conversations_with_length_filter(
        &self,
        min_messages: usize,
        limit: usize,
    ) -> Result<Vec<String>>;

    /// Get recently active threads within specified hours.
    async fn get_recent_threads(&self, hours: i64, limit: usize) -> Result<Vec<String>>;
}
