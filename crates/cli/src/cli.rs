use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "thunderbot")]
#[command(about = "Stateful AI Agent for Bluesky", long_about = None)]
pub struct Cli {
    /// Path to configuration file
    #[arg(short, long, default_value = "config.toml")]
    pub config: String,

    /// Output format
    #[arg(long, default_value = "text")]
    pub format: OutputFormat,

    /// Verbosity level
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputFormat {
    Text,
    Json,
}

impl clap::ValueEnum for OutputFormat {
    fn value_variants<'a>() -> &'a [Self] {
        &[Self::Text, Self::Json]
    }

    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        match self {
            Self::Text => Some(clap::builder::PossibleValue::new("text")),
            Self::Json => Some(clap::builder::PossibleValue::new("json")),
        }
    }
}

#[derive(Subcommand)]
pub enum Commands {
    /// Jetstream operations
    Jetstream {
        #[command(subcommand)]
        command: JetstreamCommands,
    },
    /// Bluesky operations
    Bsky {
        #[command(subcommand)]
        command: BskyCommands,
    },
    /// Database operations
    Db {
        #[command(subcommand)]
        command: DbCommands,
    },
    /// AI operations
    Ai {
        #[command(subcommand)]
        command: AiCommands,
    },
    /// Vector memory operations
    Vector {
        #[command(subcommand)]
        command: VectorCommands,
    },
    /// Start the main daemon
    Serve,
    /// Show service status
    Status,
    /// Configuration commands
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },
}

#[derive(Subcommand)]
pub enum JetstreamCommands {
    /// Listen to Jetstream events
    Listen {
        /// Filter to mentions of this DID
        #[arg(long)]
        filter_did: Option<String>,
        /// Stop after this many seconds
        #[arg(long)]
        duration: Option<u64>,
    },
    /// Replay events from cursor
    Replay {
        /// Unix microseconds cursor
        #[arg(long)]
        cursor: i64,
    },
}

#[derive(Subcommand)]
pub enum BskyCommands {
    /// Login and cache session
    Login,
    /// Show current session info
    Whoami,
    /// Create a new post
    Post {
        /// Post text
        text: String,
    },
    /// Reply to a post
    Reply {
        /// URI of post to reply to
        uri: String,
        /// Reply text
        text: String,
    },
    /// Resolve handle to DID
    Resolve {
        /// Handle to resolve
        handle: String,
    },
    /// Get a post by URI
    GetPost {
        /// Post URI
        uri: String,
    },
}

#[derive(Subcommand)]
pub enum DbCommands {
    /// Run pending database migrations
    Migrate,
    /// Show database statistics
    Stats,
    /// List recent threads
    Threads {
        /// Number of threads to show
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },
    /// Show thread history
    Thread {
        /// Root URI of thread
        root_uri: String,
    },
    /// List cached identities
    Identities,
}

#[derive(Subcommand)]
pub enum AiCommands {
    /// Send a one-shot prompt
    Prompt {
        /// Prompt text
        text: String,
    },
    /// Interactive chat session
    Chat,
    /// Build context for a thread
    Context {
        /// Root URI of thread
        root_uri: String,
    },
    /// Simulate response without posting
    Simulate {
        /// Root URI of thread
        root_uri: String,
    },
}

#[derive(Subcommand)]
pub enum ConfigCommands {
    /// Show current configuration
    Show,
    /// Validate configuration file
    Validate,
}

#[derive(Subcommand)]
pub enum VectorCommands {
    /// Show vector store statistics
    Stats,
    /// Perform similarity search
    Search {
        /// Search query text
        query: String,
        /// Number of results to return
        #[arg(short, long, default_value = "5")]
        top_k: usize,
        /// Filter by author DID
        #[arg(long)]
        author_did: Option<String>,
        /// Filter by role (user/model)
        #[arg(long)]
        role: Option<String>,
    },
    /// Generate and display embedding for text
    Embed {
        /// Text to embed
        text: String,
    },
    /// Backfill embeddings for existing conversations
    Backfill {
        /// Root URI of conversation to backfill
        root_uri: String,
    },
}
