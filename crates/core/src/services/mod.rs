//! Services module for high-level business logic
//!
//! This module provides:
//! - Thread context reconstruction
//! - Identity resolution with caching
//! - Other domain-specific services

pub mod identity;
pub mod thread;

pub use identity::{IDENTITY_TTL_HOURS, IdentityResolver, SharedIdentityResolver, create_shared_resolver};
pub use thread::{ConversationRole, ThreadContext, ThreadReconstructor};
pub use thread::{extract_created_at, extract_parent_uri, extract_root_uri, extract_text};
