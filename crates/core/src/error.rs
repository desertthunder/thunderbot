use std::fmt::{self, Display, Formatter};

pub type Result<T> = std::result::Result<T, ThunderBotError>;

#[derive(Debug, Clone)]
pub enum ThunderBotError {
    AuthenticationFailed { service: String, suggestion: String },
    RateLimited { service: String, suggestion: String },
    DatabaseError { operation: String, suggestion: String },
    NetworkError { operation: String, suggestion: String },
    ConfigurationError { variable: String, suggestion: String },
    SessionExpired { service: String, suggestion: String },
    NotFound { resource: String, suggestion: String },
}

impl Display for ThunderBotError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthenticationFailed { service, suggestion } => {
                write!(
                    f,
                    "Authentication failed for {}\n\nSuggested fix: {}",
                    service, suggestion
                )
            }
            Self::RateLimited { service, suggestion } => {
                write!(f, "Rate limited by {}\n\nSuggested fix: {}", service, suggestion)
            }
            Self::DatabaseError { operation, suggestion } => {
                write!(
                    f,
                    "Database error during {}\n\nSuggested fix: {}",
                    operation, suggestion
                )
            }
            Self::NetworkError { operation, suggestion } => {
                write!(f, "Network error during {}\n\nSuggested fix: {}", operation, suggestion)
            }
            Self::ConfigurationError { variable, suggestion } => {
                write!(f, "Configuration error: {}\n\nSuggested fix: {}", variable, suggestion)
            }
            Self::SessionExpired { service, suggestion } => {
                write!(f, "Session expired for {}\n\nSuggested fix: {}", service, suggestion)
            }
            Self::NotFound { resource, suggestion } => {
                write!(f, "Resource not found: {}\n\nSuggested fix: {}", resource, suggestion)
            }
        }
    }
}

impl std::error::Error for ThunderBotError {}

pub fn authentication_failed_bluesky() -> ThunderBotError {
    ThunderBotError::AuthenticationFailed {
        service: "Bluesky".to_string(),
        suggestion: "Run `thunderbot bsky login` to refresh your session. Check that your app password is still valid at https://bsky.app/settings/app-passwords".to_string(),
    }
}

pub fn session_expired_bluesky() -> ThunderBotError {
    ThunderBotError::SessionExpired {
        service: "Bluesky".to_string(),
        suggestion: "Run `thunderbot bsky login` to refresh your session. Consider setting up automatic token refresh"
            .to_string(),
    }
}

pub fn rate_limited_bluesky() -> ThunderBotError {
    ThunderBotError::RateLimited {
        service: "Bluesky".to_string(),
        suggestion: "Wait a few minutes before trying again. Bluesky rate limits reset every 5 minutes. For more details, see https://docs.bsky.app/docs/advanced-guides/rate-limits".to_string(),
    }
}

pub fn authentication_failed_gemini() -> ThunderBotError {
    ThunderBotError::AuthenticationFailed {
        service: "Gemini API".to_string(),
        suggestion: "Check that GEMINI_API_KEY is set correctly. Your API key should start with 'AIza'. Get a new key at https://aistudio.google.com/app/apikey".to_string(),
    }
}

pub fn configuration_missing(var_name: &str) -> ThunderBotError {
    let suggestion = match var_name {
        "BSKY_HANDLE" => "Set BSKY_HANDLE in your .env file (e.g., BSKY_HANDLE=yourhandle.bsky.social)".to_string(),
        "BSKY_APP_PASSWORD" => {
            "Set BSKY_APP_PASSWORD in your .env file. Create an app password at https://bsky.app/settings/app-passwords"
                .to_string()
        }
        "GEMINI_API_KEY" => {
            "Set GEMINI_API_KEY in your .env file. Get a key at https://aistudio.google.com/app/apikey".to_string()
        }
        "DATABASE_URL" => "Set DATABASE_URL in your .env file (e.g., DATABASE_URL=file:bot.db)".to_string(),
        _ => format!("Set {} in your .env file", var_name),
    };

    ThunderBotError::ConfigurationError { variable: var_name.to_string(), suggestion }
}

pub fn database_connection_failed() -> ThunderBotError {
    ThunderBotError::DatabaseError {
        operation: "connection".to_string(),
        suggestion: "Check that DATABASE_URL is correct. For local SQLite, ensure the parent directory exists and is writable. Run `thunderbot config validate` to diagnose issues".to_string(),
    }
}

pub fn database_query_failed(operation: &str) -> ThunderBotError {
    ThunderBotError::DatabaseError {
        operation: operation.to_string(),
        suggestion: "Run `thunderbot db migrate` to ensure your database schema is up to date. If the issue persists, try `thunderbot db vacuum` to optimize the database".to_string(),
    }
}

pub fn network_error_bluesky(operation: &str) -> ThunderBotError {
    ThunderBotError::NetworkError {
        operation: format!("Bluesky {}", operation),
        suggestion: "Check your internet connection. If using a VPN, try disabling it temporarily. Verify PDS_HOST is correct (default: https://bsky.social)".to_string(),
    }
}

pub fn network_error_gemini(operation: &str) -> ThunderBotError {
    ThunderBotError::NetworkError {
        operation: format!("Gemini {}", operation),
        suggestion: "Check your internet connection. Verify GEMINI_API_KEY is valid. If behind a corporate firewall, ensure API access is allowed".to_string(),
    }
}

pub fn not_found_post(uri: &str) -> ThunderBotError {
    ThunderBotError::NotFound {
        resource: format!("Post: {}", uri),
        suggestion:
            "Verify the URI is correct and the post still exists. Bluesky posts can be deleted by their authors"
                .to_string(),
    }
}

pub fn not_found_thread(root_uri: &str) -> ThunderBotError {
    ThunderBotError::NotFound {
        resource: format!("Thread: {}", root_uri),
        suggestion: "The thread may not exist in your database yet. Try fetching the post first with `thunderbot bsky get-post <uri>`".to_string(),
    }
}

pub trait ErrorContext<T> {
    fn with_error_context(self, op: &str) -> Result<T>;
}

impl<T, E> ErrorContext<T> for std::result::Result<T, E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn with_error_context(self, op: &str) -> Result<T> {
        self.map_err(|e| {
            let error_str = e.to_string().to_lowercase();

            match error_str {
                s if s.contains("429") || s.contains("rate limit") => rate_limited_bluesky(),
                s if s.contains("401") || s.contains("unauthorized") || s.contains("authentication") => {
                    authentication_failed_bluesky()
                }
                s if s.contains("network") || s.contains("connection") || s.contains("timeout") => {
                    network_error_bluesky(op)
                }
                s if s.contains("not found") || s.contains("404") => not_found_post(op),
                _ => ThunderBotError::NetworkError {
                    operation: op.to_string(),
                    suggestion: format!("Error: {}. Check your configuration and try again.", e),
                },
            }
        })
    }
}

pub fn is_transient(error: &anyhow::Error) -> bool {
    let error_str = error.to_string().to_lowercase();
    error_str.contains("timeout")
        || error_str.contains("connection")
        || error_str.contains("429")
        || error_str.contains("rate limit")
        || error_str.contains("econnrefused")
}

pub fn is_permanent(error: &anyhow::Error) -> bool {
    !is_transient(error)
}
