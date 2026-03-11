use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "tnbot",
    about = "ThunderBot - A Stateful AI Agent for Bluesky",
    version = "0.1.0"
)]
pub struct Cli {
    #[arg(short, long, help = "Path to configuration file")]
    pub config: Option<PathBuf>,

    #[arg(short, long, action = clap::ArgAction::Count, help = "Increase verbosity (use multiple times)")]
    pub verbose: u8,

    #[arg(long, help = "Output results as JSON")]
    pub json: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    #[command(about = "Start the main daemon")]
    Serve {
        #[arg(long, help = "Run without posting replies (dry run mode)")]
        dry_run: bool,
    },

    #[command(about = "Configuration commands")]
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    #[command(about = "Jetstream firehose commands", visible_alias = "js")]
    Jetstream {
        #[command(subcommand)]
        action: JetstreamAction,
    },

    #[command(about = "Bluesky XRPC commands")]
    Bsky {
        #[command(subcommand)]
        action: BskyAction,
    },

    #[command(about = "Database commands")]
    Db {
        #[command(subcommand)]
        action: DbAction,
    },

    #[command(about = "AI/GLM-5 commands")]
    Ai {
        #[command(subcommand)]
        action: AiAction,
    },

    #[command(about = "Vector memory commands")]
    Vector {
        #[command(subcommand)]
        action: VectorAction,
    },

    #[command(about = "Show service status")]
    Status,
}

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    #[command(about = "Display current configuration")]
    Show,

    #[command(about = "Validate configuration file")]
    Validate,
}

#[derive(Subcommand, Debug)]
pub enum JetstreamAction {
    #[command(about = "Listen to Jetstream firehose")]
    Listen {
        #[arg(long, help = "Filter to mentions of a specific DID")]
        filter_did: Option<String>,

        #[arg(long, help = "Listen for a fixed duration then exit")]
        duration: Option<u64>,
    },

    #[command(about = "Replay events from a specific cursor")]
    Replay {
        #[arg(long, help = "Cursor timestamp (time_us) to start from")]
        cursor: u64,
        #[arg(long, help = "Filter to mentions of a specific DID")]
        filter_did: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum BskyAction {
    #[command(about = "Bind/authenticate the bot account with Bluesky")]
    Login,

    #[command(about = "Display current session info")]
    Whoami,

    #[command(about = "Create a new post")]
    Post { text: String },

    #[command(about = "Reply to an existing post")]
    Reply { uri: String, text: String },

    #[command(about = "Resolve a handle to DID")]
    Resolve { handle: String },

    #[command(about = "Fetch and display a post record")]
    GetPost { uri: String },
}

#[derive(Subcommand, Debug)]
pub enum DbAction {
    #[command(about = "Run pending database migrations")]
    Migrate,

    #[command(about = "Show database statistics")]
    Stats,

    #[command(about = "List recent conversation threads")]
    Threads,

    #[command(about = "Display full thread history")]
    Thread { root_uri: String },

    #[command(about = "List cached identity mappings")]
    Identities,
}

#[derive(Subcommand, Debug)]
pub enum AiAction {
    #[command(about = "Send a one-shot prompt to GLM-5")]
    Prompt { text: String },

    #[command(about = "Interactive chat session with GLM-5")]
    Chat,

    #[command(about = "Build and display the prompt context for a thread")]
    Context { root_uri: String },

    #[command(about = "Simulate a response without posting")]
    Simulate { root_uri: String },
}

#[derive(Subcommand, Debug, Clone)]
pub enum VectorAction {
    #[command(about = "Show vector store statistics")]
    Stats,

    #[command(about = "Semantic search across all memories")]
    Search {
        query: String,
        #[arg(long, help = "Number of results to return")]
        top_k: Option<usize>,
        #[arg(long, help = "Filter by author DID")]
        author: Option<String>,
    },

    #[command(about = "Generate and display raw embedding vector")]
    Embed { text: String },

    #[command(about = "Backfill embeddings for all conversations")]
    Backfill {
        #[arg(long, help = "Batch size for processing")]
        batch_size: Option<usize>,
    },

    #[command(about = "Consolidate stale thread memories")]
    Consolidate,

    #[command(about = "Remove expired memories")]
    Expire,
}

pub fn parse_log_level(verbose: u8) -> String {
    match verbose {
        0 => "info",
        1 => "debug",
        _ => "trace",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parse_serve() {
        let args = vec!["tnbot", "serve", "--dry-run"];
        let cli = Cli::parse_from(args);
        assert!(!cli.json);
        assert!(cli.verbose == 0);
        assert!(matches!(cli.command, Commands::Serve { dry_run: true }));
    }

    #[test]
    fn test_cli_parse_config_show() {
        let args = vec!["tnbot", "config", "show"];
        let cli = Cli::parse_from(args);
        assert!(matches!(cli.command, Commands::Config { action: ConfigAction::Show }));
    }

    #[test]
    fn test_cli_parse_config_validate() {
        let args = vec!["tnbot", "config", "validate"];
        let cli = Cli::parse_from(args);
        assert!(matches!(
            cli.command,
            Commands::Config { action: ConfigAction::Validate }
        ));
    }

    #[test]
    fn test_cli_parse_jetstream_listen() {
        let args = vec!["tnbot", "js", "listen", "--filter-did", "did:plc:test"];
        let cli = Cli::parse_from(args);
        match cli.command {
            Commands::Jetstream { action: JetstreamAction::Listen { filter_did, duration } } => {
                assert_eq!(filter_did, Some("did:plc:test".to_string()));
                assert_eq!(duration, None);
            }
            _ => panic!("Expected Jetstream Listen command"),
        }
    }

    #[test]
    fn test_cli_parse_jetstream_replay() {
        let args = vec!["tnbot", "jetstream", "replay", "--cursor", "12345"];
        let cli = Cli::parse_from(args);
        match cli.command {
            Commands::Jetstream { action: JetstreamAction::Replay { cursor, filter_did } } => {
                assert_eq!(cursor, 12345);
                assert_eq!(filter_did, None);
            }
            _ => panic!("Expected Jetstream Replay command"),
        }
    }

    #[test]
    fn test_cli_parse_bsky_post() {
        let args = vec!["tnbot", "bsky", "post", "Hello world"];
        let cli = Cli::parse_from(args);
        match cli.command {
            Commands::Bsky { action: BskyAction::Post { text } } => assert_eq!(text, "Hello world"),
            _ => panic!("Expected Bsky Post command"),
        }
    }

    #[test]
    fn test_cli_parse_status() {
        let args = vec!["tnbot", "status"];
        let cli = Cli::parse_from(args);
        assert!(matches!(cli.command, Commands::Status));
    }

    #[test]
    fn test_cli_parse_with_json_flag() {
        let args = vec!["tnbot", "--json", "status"];
        let cli = Cli::parse_from(args);
        assert!(cli.json);
    }

    #[test]
    fn test_cli_parse_with_verbose_flags() {
        let args = vec!["tnbot", "-vvv", "status"];
        let cli = Cli::parse_from(args);
        assert_eq!(cli.verbose, 3);
    }

    #[test]
    fn test_cli_parse_with_config() {
        let args = vec!["tnbot", "-c", "/path/to/config.toml", "status"];
        let cli = Cli::parse_from(args);
        assert_eq!(cli.config, Some(PathBuf::from("/path/to/config.toml")));
    }

    #[test]
    fn test_cli_parse_db_actions() {
        let migrate_args = vec!["tnbot", "db", "migrate"];
        let cli = Cli::parse_from(migrate_args);
        assert!(matches!(cli.command, Commands::Db { action: DbAction::Migrate }));

        let stats_args = vec!["tnbot", "db", "stats"];
        let cli = Cli::parse_from(stats_args);
        assert!(matches!(cli.command, Commands::Db { action: DbAction::Stats }));

        let threads_args = vec!["tnbot", "db", "threads"];
        let cli = Cli::parse_from(threads_args);
        assert!(matches!(cli.command, Commands::Db { action: DbAction::Threads }));
    }

    #[test]
    fn test_cli_parse_ai_prompt() {
        let args = vec!["tnbot", "ai", "prompt", "Hello AI"];
        let cli = Cli::parse_from(args);
        match cli.command {
            Commands::Ai { action: AiAction::Prompt { text } } => assert_eq!(text, "Hello AI"),
            _ => panic!("Expected AI Prompt command"),
        }
    }
}
