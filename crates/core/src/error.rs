#[derive(thiserror::Error, Debug)]
pub enum BotError {
    #[error("Configuration error: {0}")]
    Config(#[from] config::ConfigError),

    #[error("Configuration validation error: {0}")]
    Validation(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, BotError>;
