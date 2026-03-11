use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind")]
pub enum JetstreamEvent {
    #[serde(rename = "commit")]
    Commit {
        did: String,
        time_us: i64,
        commit: CommitData,
    },
    #[serde(rename = "identity")]
    Identity {
        did: String,
        time_us: i64,
        identity: IdentityData,
    },
    #[serde(rename = "account")]
    Account {
        did: String,
        time_us: i64,
        account: AccountData,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CommitData {
    pub rev: String,
    pub operation: CommitOperation,
    pub collection: String,
    pub rkey: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cid: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CommitOperation {
    Create,
    Update,
    Delete,
}

impl Display for CommitOperation {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            CommitOperation::Create => f.write_str("create"),
            CommitOperation::Update => f.write_str("update"),
            CommitOperation::Delete => f.write_str("delete"),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct IdentityData {
    pub did: String,
    #[serde(default)]
    pub handle: Option<String>,
    pub seq: i64,
    pub time: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AccountData {
    pub active: bool,
    pub did: String,
    pub seq: i64,
    pub time: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Facet {
    pub index: FacetIndex,
    pub features: Vec<FacetFeature>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FacetIndex {
    #[serde(rename = "byteStart")]
    pub byte_start: i64,
    #[serde(rename = "byteEnd")]
    pub byte_end: i64,
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

impl CommitData {
    pub fn is_mention_of(&self, target_did: &str) -> bool {
        if self.collection != "app.bsky.feed.post" || self.operation != CommitOperation::Create {
            return false;
        }

        let Some(record) = self.record.as_ref() else {
            return false;
        };

        let Some(facets) = record.get("facets").and_then(|f| f.as_array()) else {
            return false;
        };

        for facet in facets {
            if let Ok(facet) = serde_json::from_value::<Facet>(facet.clone()) {
                for feature in &facet.features {
                    if let FacetFeature::Mention { did } = feature
                        && did == target_did
                    {
                        return true;
                    }
                }
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_commit_data_is_mention_of() {
        let record = serde_json::json!({
            "facets": [
                {
                    "index": { "byteStart": 0, "byteEnd": 8 },
                    "features": [
                        {
                            "$type": "app.bsky.richtext.facet#mention",
                            "did": "did:plc:bot123"
                        }
                    ]
                }
            ]
        });

        let commit = CommitData {
            rev: "test".to_string(),
            operation: CommitOperation::Create,
            collection: "app.bsky.feed.post".to_string(),
            rkey: "test".to_string(),
            record: Some(record),
            cid: None,
        };

        assert!(commit.is_mention_of("did:plc:bot123"));
        assert!(!commit.is_mention_of("did:plc:other"));
    }

    #[test]
    fn test_commit_data_not_post() {
        let commit = CommitData {
            rev: "test".to_string(),
            operation: CommitOperation::Create,
            collection: "app.bsky.feed.like".to_string(),
            rkey: "test".to_string(),
            record: None,
            cid: None,
        };

        assert!(!commit.is_mention_of("did:plc:bot123"));
    }

    #[test]
    fn test_deserialize_commit_event() {
        let json = r#"
        {
            "did": "did:plc:eygmaihciaxprqvxpfvl6flk",
            "time_us": 1725911162329308,
            "kind": "commit",
            "commit": {
                "rev": "3l3qo2vutsw2b",
                "operation": "create",
                "collection": "app.bsky.feed.post",
                "rkey": "3l3qo2vuowo2b",
                "record": {"text": "Hello", "$type": "app.bsky.feed.post"},
                "cid": "bafyrei..."
            }
        }
        "#;

        let event: JetstreamEvent = serde_json::from_str(json).unwrap();
        match event {
            JetstreamEvent::Commit { did, time_us, commit } => {
                assert_eq!(did, "did:plc:eygmaihciaxprqvxpfvl6flk");
                assert_eq!(time_us, 1725911162329308);
                assert_eq!(commit.collection, "app.bsky.feed.post");
                assert_eq!(commit.operation, CommitOperation::Create);
            }
            _ => panic!("Expected Commit event"),
        }
    }

    #[test]
    fn test_deserialize_identity_event() {
        let json = r#"
        {
            "did": "did:plc:test",
            "time_us": 1725516665234703,
            "kind": "identity",
            "identity": {
                "did": "did:plc:test",
                "handle": "user.bsky.social",
                "seq": 1409752997,
                "time": "2024-09-05T06:11:04.870Z"
            }
        }
        "#;

        let event: JetstreamEvent = serde_json::from_str(json).unwrap();
        match event {
            JetstreamEvent::Identity { identity, .. } => {
                assert_eq!(identity.handle.as_deref(), Some("user.bsky.social"));
            }
            _ => panic!("Expected Identity event"),
        }
    }

    #[test]
    fn test_deserialize_identity_event_without_handle() {
        let json = r#"
        {
            "did": "did:plc:test",
            "time_us": 1725516665234703,
            "kind": "identity",
            "identity": {
                "did": "did:plc:test",
                "seq": 1409752997,
                "time": "2024-09-05T06:11:04.870Z"
            }
        }
        "#;

        let event: JetstreamEvent = serde_json::from_str(json).unwrap();
        match event {
            JetstreamEvent::Identity { identity, .. } => {
                assert_eq!(identity.handle, None);
            }
            _ => panic!("Expected Identity event"),
        }
    }

    #[test]
    fn test_deserialize_account_event() {
        let json = r#"
        {
            "did": "did:plc:test",
            "time_us": 1725516665333808,
            "kind": "account",
            "account": {
                "active": true,
                "did": "did:plc:test",
                "seq": 1409753013,
                "time": "2024-09-05T06:11:04.870Z"
            }
        }
        "#;

        let event: JetstreamEvent = serde_json::from_str(json).unwrap();
        match event {
            JetstreamEvent::Account { account, .. } => {
                assert!(account.active);
            }
            _ => panic!("Expected Account event"),
        }
    }
}
