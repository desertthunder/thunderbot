pub mod activity;
pub mod blocklist;
pub mod control;
pub mod dead_letter;
pub mod filter;
pub mod identity;
pub mod quiet_hours;
pub mod rate_limit;
pub mod reply_limits;
pub mod response_queue;
pub mod search;
pub mod session;
pub mod session_metadata;
pub mod thread;

pub use activity::ActivityRepository;
pub use blocklist::BlocklistRepository;
pub use control::ControlRepository;
pub use dead_letter::DeadLetterRepository;
pub use filter::FilterRepository;
pub use identity::IdentityRepository;
pub use quiet_hours::QuietHoursRepository;
pub use rate_limit::RateLimitRepository;
pub use reply_limits::ReplyLimitsRepository;
pub use response_queue::ResponseQueueRepository;
pub use search::SearchRepository;
pub use session::SessionRepository;
pub use session_metadata::SessionMetadataRepository;
pub use thread::ThreadRepository;

use anyhow::Result;
use async_trait::async_trait;

/// Combined database repository that aggregates all domain-specific traits.
///
/// This trait requires all specialized repository traits, providing a single interface
/// for complete database access while maintaining clear separation of concerns in implementation.
#[async_trait]
pub trait DatabaseRepository:
    Send
    + Sync
    + ThreadRepository
    + IdentityRepository
    + SessionRepository
    + ControlRepository
    + SearchRepository
    + FilterRepository
    + ActivityRepository
    + ResponseQueueRepository
    + QuietHoursRepository
    + ReplyLimitsRepository
    + BlocklistRepository
    + DeadLetterRepository
    + RateLimitRepository
    + SessionMetadataRepository
{
    /// Run all database migrations to bring schema up to date.
    async fn run_migration(&self) -> Result<()>;
}

pub use crate::control::{
    BlockType, BlocklistEntry, DeadLetterItem, QuietHoursWindow, RateLimitSnapshot, ReplyLimitsConfig,
    ResponseQueueItem, ResponseStatus, SessionMetadata,
};
