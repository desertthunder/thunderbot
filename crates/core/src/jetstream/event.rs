use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum JetstreamEvent {
    Commit(CommitEvent),
    Identity(IdentityEvent),
    Account(AccountEvent),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CommitEvent {
    pub did: String,
    #[serde(rename = "timeUs")]
    pub time_us: i64,
    pub commit: CommitData,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CommitData {
    pub rev: String,
    pub operation: Operation,
    pub collection: String,
    pub rkey: String,
    #[serde(default)]
    pub record: Option<serde_json::Value>,
    #[serde(default)]
    pub cid: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Operation {
    Create,
    Update,
    Delete,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct IdentityEvent {
    pub did: String,
    #[serde(rename = "timeUs")]
    pub time_us: i64,
    pub identity: IdentityData,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct IdentityData {
    pub did: String,
    pub handle: String,
    pub seq: i64,
    pub time: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AccountEvent {
    pub did: String,
    #[serde(rename = "timeUs")]
    pub time_us: i64,
    pub account: AccountData,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AccountData {
    pub active: bool,
    pub did: String,
    pub seq: i64,
    pub time: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PostRecord {
    pub text: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(default)]
    pub facets: Vec<Facet>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Facet {
    pub index: ByteSlice,
    pub features: Vec<FacetFeature>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ByteSlice {
    #[serde(rename = "byteStart")]
    pub byte_start: usize,
    #[serde(rename = "byteEnd")]
    pub byte_end: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "$type")]
pub enum FacetFeature {
    #[serde(rename = "app.bsky.richtext.facet#mention")]
    Mention { did: String },
    #[serde(rename = "app.bsky.richtext.facet#link")]
    Link { uri: String },
    #[serde(rename = "app.bsky.richtext.facet#tag")]
    Tag { tag: String },
}
