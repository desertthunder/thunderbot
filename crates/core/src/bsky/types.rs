use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub did: String,
    pub handle: String,
    pub access_jwt: String,
    pub refresh_jwt: String,
}

#[derive(Debug, Deserialize)]
pub struct SessionResponse {
    pub did: String,
    pub handle: String,
    #[serde(rename = "accessJwt")]
    pub access_jwt: String,
    #[serde(rename = "refreshJwt")]
    pub refresh_jwt: String,
}

impl From<SessionResponse> for Session {
    fn from(response: SessionResponse) -> Self {
        Self {
            did: response.did,
            handle: response.handle,
            access_jwt: response.access_jwt,
            refresh_jwt: response.refresh_jwt,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CreateSessionRequest {
    pub identifier: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateRecordResponse {
    pub uri: String,
    pub cid: String,
}

#[derive(Debug, Serialize)]
pub struct CreateRecordRequest {
    pub repo: String,
    pub collection: String,
    pub record: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct PostRecordWrite {
    #[serde(rename = "$type")]
    pub record_type: String,
    pub text: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply: Option<ReplyRefWrite>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReplyRefWrite {
    pub root: StrongRefWrite,
    pub parent: StrongRefWrite,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StrongRefWrite {
    pub uri: String,
    pub cid: String,
}

#[derive(Debug, Deserialize)]
pub struct GetRecordResponse {
    pub uri: String,
    pub cid: String,
    pub value: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct ResolveHandleResponse {
    pub did: String,
}

#[derive(Debug, Deserialize)]
pub struct GetProfileResponse {
    pub did: String,
    pub handle: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub avatar: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct XrpcError {
    pub error: String,
    pub message: Option<String>,
}
