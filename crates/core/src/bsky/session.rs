//! Session management for Bluesky XRPC
//!
//! Handles JWT tokens, expiry tracking, and automatic refresh.

use crate::bsky::models::CreateSessionResponse;
use crate::error::Result;
use base64::Engine;
use chrono::{DateTime, Duration, Utc};

/// JWT claims for decoding expiry
#[derive(Debug, serde::Deserialize)]
struct JwtClaims {
    exp: i64,
}

/// Session information including tokens and metadata
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Session {
    /// User's DID
    pub did: String,
    /// User's handle
    pub handle: String,
    /// Short-lived access token (~2h)
    pub access_jwt: String,
    /// Long-lived refresh token (~90d)
    pub refresh_jwt: String,
    /// When the access token expires
    pub access_expires_at: DateTime<Utc>,
}

impl Session {
    /// Create a new session from createSession response
    pub fn from_create_response(response: CreateSessionResponse) -> Result<Self> {
        let access_expires_at = decode_expiry(&response.access_jwt).unwrap_or_else(|| Utc::now() + Duration::hours(2));

        Ok(Session {
            did: response.did,
            handle: response.handle,
            access_jwt: response.access_jwt,
            refresh_jwt: response.refresh_jwt,
            access_expires_at,
        })
    }

    /// Update session with refreshed tokens
    pub fn update_from_refresh(&mut self, access_jwt: String, refresh_jwt: String) -> Result<()> {
        let access_expires_at = decode_expiry(&access_jwt).unwrap_or_else(|| Utc::now() + Duration::hours(2));

        self.access_jwt = access_jwt;
        self.refresh_jwt = refresh_jwt;
        self.access_expires_at = access_expires_at;
        Ok(())
    }

    /// Check if the access token is expired or about to expire
    ///
    /// Returns true if the token expires within the given buffer
    pub fn is_expiring(&self, buffer_secs: i64) -> bool {
        let expiry_threshold = Utc::now() + Duration::seconds(buffer_secs);
        self.access_expires_at <= expiry_threshold
    }

    /// Check if the access token is already expired
    pub fn is_expired(&self) -> bool {
        self.access_expires_at <= Utc::now()
    }

    /// Get the authorization header value for requests
    pub fn auth_header(&self) -> String {
        format!("Bearer {}", self.access_jwt)
    }

    /// Get the refresh authorization header value
    pub fn refresh_auth_header(&self) -> String {
        format!("Bearer {}", self.refresh_jwt)
    }

    /// Get seconds until expiry
    pub fn seconds_until_expiry(&self) -> i64 {
        let now = Utc::now();
        if self.access_expires_at > now { (self.access_expires_at - now).num_seconds() } else { 0 }
    }
}

/// Decode JWT to get expiry timestamp
fn decode_expiry(jwt: &str) -> Option<DateTime<Utc>> {
    let parts: Vec<&str> = jwt.split('.').collect();
    if parts.len() != 3 {
        return None;
    }

    let payload = parts.get(1)?;
    let decoded = base64_decode(payload)?;
    let claims: JwtClaims = serde_json::from_str(&decoded).ok()?;

    DateTime::from_timestamp(claims.exp, 0)
}

/// Simple base64 decode (handles URL-safe base64)
fn base64_decode(input: &str) -> Option<String> {
    let padding_needed = (4 - input.len() % 4) % 4;
    let padded = format!("{}{}", input, "=".repeat(padding_needed));

    let standard = padded.replace('-', "+").replace('_', "/");

    base64::engine::general_purpose::STANDARD
        .decode(standard)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_expiry_check() {
        let session = Session {
            did: "did:plc:test".to_string(),
            handle: "test.bsky.social".to_string(),
            access_jwt: "fake".to_string(),
            refresh_jwt: "fake".to_string(),
            access_expires_at: Utc::now() + Duration::minutes(30),
        };

        assert!(!session.is_expiring(300));
        assert!(session.is_expiring(3600));
        assert!(!session.is_expired());
    }

    #[test]
    fn test_session_auth_headers() {
        let session = Session {
            did: "did:plc:test".to_string(),
            handle: "test.bsky.social".to_string(),
            access_jwt: "access123".to_string(),
            refresh_jwt: "refresh456".to_string(),
            access_expires_at: Utc::now() + Duration::hours(2),
        };

        assert_eq!(session.auth_header(), "Bearer access123");
        assert_eq!(session.refresh_auth_header(), "Bearer refresh456");
    }

    #[test]
    fn test_session_serde_roundtrip() {
        let session = Session {
            did: "did:plc:test".to_string(),
            handle: "test.bsky.social".to_string(),
            access_jwt: "access123".to_string(),
            refresh_jwt: "refresh456".to_string(),
            access_expires_at: Utc::now() + Duration::hours(2),
        };

        let json = serde_json::to_string(&session).expect("Session should serialize");
        let roundtrip: Session = serde_json::from_str(&json).expect("Session should deserialize");
        assert_eq!(roundtrip.did, session.did);
        assert_eq!(roundtrip.handle, session.handle);
        assert_eq!(roundtrip.access_jwt, session.access_jwt);
        assert_eq!(roundtrip.refresh_jwt, session.refresh_jwt);
    }
}
