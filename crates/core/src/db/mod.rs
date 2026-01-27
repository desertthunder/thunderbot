pub mod identity;
pub mod libsql;
pub mod thread;
pub mod traits;
pub mod types;

pub use identity::{IdentityResolver, IdentityResolverConfig};
pub use libsql::LibsqlRepository;
pub use traits::DatabaseRepository;
pub use thread::{ThreadContext, ThreadContextBuilder};
pub use types::{
    ActivityLogRow, ConversationRow, DatabaseStats, FilterPresetRow, IdentityRow, MutedAuthorRow,
    SessionRow,
};

pub type Db = std::sync::Arc<dyn DatabaseRepository>;
