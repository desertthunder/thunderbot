//! Thread and conversation repository operations.

use crate::db::types::{ConversationRow, DatabaseStats};
use anyhow::Result;
use async_trait::async_trait;

/// Repository for thread and conversation operations.
#[async_trait]
pub trait ThreadRepository: Send + Sync {
    /// Save a conversation message to the database.
    async fn save_conversation(&self, row: ConversationRow) -> Result<()>;

    /// Get all messages in a thread, ordered by creation time.
    async fn get_thread_history(&self, thread_root_uri: &str) -> Result<Vec<ConversationRow>>;

    /// Get all thread URIs, most recently active first.
    async fn get_all_threads(&self, limit: usize) -> Result<Vec<String>>;

    /// Get all threads for a specific author.
    async fn get_user_threads(&self, author_did: &str, limit: usize) -> Result<Vec<String>>;

    /// Get database statistics (conversation/thread counts).
    async fn get_stats(&self) -> Result<DatabaseStats>;

    /// Delete conversations by thread URIs. Returns count of deleted threads.
    async fn delete_conversations_by_uris(&self, thread_uris: &[String]) -> Result<usize>;

    /// Delete conversations older than specified days. Returns count deleted.
    async fn delete_old_conversations(&self, days: i64) -> Result<usize>;

    /// Get muted authors list.
    async fn get_muted_authors(&self) -> Result<Vec<crate::db::types::MutedAuthorRow>>;

    /// Add an author to the muted list.
    async fn mute_author(&self, did: &str, muted_by: &str) -> Result<()>;

    /// Remove an author from the muted list.
    async fn unmute_author(&self, did: &str) -> Result<()>;
}
