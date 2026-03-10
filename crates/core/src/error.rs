#[derive(thiserror::Error, Debug)]
pub enum BotError {
    #[error("Configuration error: {0}")]
    Config(#[from] config::ConfigError),

    #[error("Configuration validation error: {0}")]
    Validation(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Database error: {0}")]
    Database(String),

    #[error("XRPC HTTP error: {0}")]
    XrpcHttp(String),

    #[error("XRPC authentication error: {0}")]
    XrpcAuthentication(String),

    #[error("XRPC rate limit exceeded: {0}")]
    XrpcRateLimit(String),

    #[error("XRPC invalid request: {0}")]
    XrpcInvalidRequest(String),

    #[error("XRPC forbidden: {0}")]
    XrpcForbidden(String),

    #[error("XRPC server error: {0}")]
    XrpcServerError(String),

    #[error("Session expired and could not be refreshed")]
    SessionExpired,

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("XRPC configuration error: {0}")]
    XrpcConfig(String),
}

pub type Result<T> = std::result::Result<T, BotError>;

/// XRPC error response from the server
#[derive(Debug, Clone, serde::Deserialize)]
pub struct XrpcErrorResponse {
    pub error: String,
    pub message: String,
}

impl BotError {
    /// Create an error from an HTTP status code and error response
    pub fn from_xrpc_status(status: reqwest::StatusCode, error_response: Option<XrpcErrorResponse>) -> Self {
        let message = error_response
            .as_ref()
            .map(|e| format!("{}: {}", e.error, e.message))
            .unwrap_or_else(|| format!("HTTP {}", status));

        match status.as_u16() {
            400 => BotError::XrpcInvalidRequest(message),
            401 => BotError::XrpcAuthentication(message),
            403 => BotError::XrpcForbidden(message),
            429 => BotError::XrpcRateLimit(message),
            500..=599 => BotError::XrpcServerError(message),
            _ => BotError::XrpcHttp(message),
        }
    }
}

impl From<reqwest::Error> for BotError {
    fn from(err: reqwest::Error) -> Self {
        BotError::XrpcHttp(err.to_string())
    }
}

impl From<serde_json::Error> for BotError {
    fn from(err: serde_json::Error) -> Self {
        BotError::Serialization(err.to_string())
    }
}
