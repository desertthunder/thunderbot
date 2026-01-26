pub mod agent;
pub mod bsky;
pub mod db;
pub mod gemini;
pub mod health;
pub mod jetstream;
pub mod metrics;
pub mod processor;
pub mod vector;
pub mod web;

pub use agent::Agent;
pub use bsky::{BskyClient, Session};
pub use db::{
    ConversationRow, DatabaseRepository, DatabaseStats, Db, IdentityResolver, IdentityResolverConfig, IdentityRow,
    LibsqlRepository, SessionRow, ThreadContext, ThreadContextBuilder,
};
pub use gemini::{GeminiClient, PromptBuilder};
pub use health::{ComponentHealth, HealthRegistry, HealthReport, HealthStatus, JetstreamState};
pub use jetstream::{JetstreamClient, listen, replay};
pub use metrics::Metrics;
pub use processor::EventProcessor;
pub use vector::{
    EmbeddingProvider, EmbeddingRequest, EmbeddingResponse, GeminiEmbeddingProvider, Memory, MemoryConfig,
    MemoryMetadata, MemoryWithScore, SearchFilter, SemanticRetriever, SqliteVecStore, VectorStats, VectorStore,
};
pub use web::Server;
