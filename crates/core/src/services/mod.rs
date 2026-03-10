//! Services module for high-level business logic
//!
//! This module provides:
//! - Thread context reconstruction
//! - Identity resolution with caching
//! - Action pipeline for processing mentions
//! - Other domain-specific services

pub mod action;
pub mod identity;
pub mod thread;

pub use action::{ActionPipeline, ActionResult};
pub use identity::{IDENTITY_TTL_HOURS, IdentityResolver, SharedIdentityResolver, create_shared_resolver};
pub use thread::{ConversationRole, ThreadContext, ThreadReconstructor};
pub use thread::{
    extract_created_at, extract_parent_cid, extract_parent_uri, extract_root_cid, extract_root_uri, extract_text,
};
