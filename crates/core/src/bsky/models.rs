//! XRPC types and data structures

use serde::{Deserialize, Serialize};

/// Strong reference to a record (URI + CID)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StrongRef {
    pub uri: String,
    pub cid: String,
}

/// Reply reference for threading
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplyRef {
    pub root: StrongRef,
    pub parent: StrongRef,
}

/// Facet for rich text annotations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Facet {
    pub index: FacetIndex,
    pub features: Vec<FacetFeature>,
}

/// Byte index for facets
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FacetIndex {
    #[serde(rename = "byteStart")]
    pub byte_start: usize,
    #[serde(rename = "byteEnd")]
    pub byte_end: usize,
}

/// Facet feature types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "$type")]
pub enum FacetFeature {
    #[serde(rename = "app.bsky.richtext.facet#mention")]
    Mention { did: String },
    #[serde(rename = "app.bsky.richtext.facet#link")]
    Link { uri: String },
    #[serde(rename = "app.bsky.richtext.facet#tag")]
    Tag { tag: String },
}

/// Post record for creating posts
#[derive(Debug, Clone, Serialize)]
pub struct PostRecord {
    #[serde(rename = "$type")]
    pub r#type: String,
    pub text: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply: Option<ReplyRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facets: Option<Vec<Facet>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub langs: Option<Vec<String>>,
}

impl PostRecord {
    /// Create a new simple post
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            r#type: "app.bsky.feed.post".to_string(),
            text: text.into(),
            created_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            reply: None,
            facets: None,
            langs: Some(vec!["en".to_string()]),
        }
    }

    /// Create a reply to a post
    pub fn reply(text: impl Into<String>, root: StrongRef, parent: StrongRef) -> Self {
        Self {
            r#type: "app.bsky.feed.post".to_string(),
            text: text.into(),
            created_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            reply: Some(ReplyRef { root, parent }),
            facets: None,
            langs: Some(vec!["en".to_string()]),
        }
    }
}

/// Create record request
#[derive(Debug, Clone, Serialize)]
pub struct CreateRecordRequest {
    pub repo: String,
    pub collection: String,
    pub record: serde_json::Value,
}

/// Create record response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRecordResponse {
    pub uri: String,
    pub cid: String,
}

/// Get record response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetRecordResponse {
    pub uri: String,
    pub cid: String,
    pub value: serde_json::Value,
}

/// Create session request
#[derive(Debug, Clone, Serialize)]
pub struct CreateSessionRequest {
    pub identifier: String,
    pub password: String,
}

/// Create session response
#[derive(Debug, Clone, Deserialize)]
pub struct CreateSessionResponse {
    #[serde(rename = "accessJwt")]
    pub access_jwt: String,
    #[serde(rename = "refreshJwt")]
    pub refresh_jwt: String,
    pub handle: String,
    pub did: String,
    #[serde(rename = "didDoc")]
    pub did_doc: Option<serde_json::Value>,
}

/// Refresh session response
#[derive(Debug, Clone, Deserialize)]
pub struct RefreshSessionResponse {
    #[serde(rename = "accessJwt")]
    pub access_jwt: String,
    #[serde(rename = "refreshJwt")]
    pub refresh_jwt: String,
    pub handle: String,
    pub did: String,
}

/// Resolve handle response
#[derive(Debug, Clone, Deserialize)]
pub struct ResolveHandleResponse {
    pub did: String,
}

/// Get profile response
#[derive(Debug, Clone, Deserialize)]
pub struct GetProfileResponse {
    pub did: String,
    pub handle: String,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "followersCount")]
    pub followers_count: Option<i64>,
    #[serde(rename = "followsCount")]
    pub follows_count: Option<i64>,
    #[serde(rename = "postsCount")]
    pub posts_count: Option<i64>,
}

/// Parsed AT URI
#[derive(Debug, Clone)]
pub struct AtUri {
    pub repo: String,
    pub collection: String,
    pub rkey: String,
}

impl AtUri {
    /// Parse an AT URI string
    pub fn parse(uri: &str) -> Option<Self> {
        let parts: Vec<&str> = uri.split('/').collect();
        if parts.len() >= 5 && parts[0] == "at:" && parts[1].is_empty() {
            Some(AtUri {
                repo: parts[2].to_string(),
                collection: parts[3..parts.len() - 1].join("/"),
                rkey: parts.last()?.to_string(),
            })
        } else {
            None
        }
    }

    /// Convert back to string
    pub fn as_string(&self) -> String {
        format!("at://{}/{}/{}", self.repo, self.collection, self.rkey)
    }
}

#[cfg(test)]
mod tests {
    use super::AtUri;

    #[test]
    fn test_parse_at_uri() {
        let uri = "at://did:plc:abc123/app.bsky.feed.post/3k43tv4rft22g";
        let parsed = AtUri::parse(uri).expect("Should parse valid AT URI");
        assert_eq!(parsed.repo, "did:plc:abc123");
        assert_eq!(parsed.collection, "app.bsky.feed.post");
        assert_eq!(parsed.rkey, "3k43tv4rft22g");
        assert_eq!(parsed.as_string(), uri);
    }

    #[test]
    fn test_parse_invalid_at_uri() {
        assert!(AtUri::parse("https://bsky.app").is_none());
        assert!(AtUri::parse("at://did:plc:abc123/app.bsky.feed.post").is_none());
    }
}
