use super::repository::{Db, IdentityRow};
use anyhow::Result;
use chrono::{Duration, Utc};
use reqwest::Client;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct IdentityResolverConfig {
    pub pds_host: String,
    pub cache_ttl: Duration,
}

impl Default for IdentityResolverConfig {
    fn default() -> Self {
        Self { pds_host: "https://bsky.social".to_string(), cache_ttl: Duration::hours(24) }
    }
}

pub struct IdentityResolver {
    db: Db,
    client: Client,
    config: IdentityResolverConfig,
}

#[derive(Debug, Deserialize)]
struct ResolveHandleResponse {
    did: String,
}

impl IdentityResolver {
    pub fn new(db: Db, config: IdentityResolverConfig) -> Self {
        Self { db, client: Client::new(), config }
    }

    pub async fn resolve_did_to_handle(&self, did: &str) -> Result<String> {
        if let Some(identity) = self.db.get_identity(did).await?
            && self.is_fresh(&identity)
        {
            return Ok(identity.handle);
        }

        let handle = self.fetch_handle_from_pds(did).await?;

        self.db
            .save_identity(IdentityRow { did: did.to_string(), handle: handle.clone(), last_updated: Utc::now() })
            .await?;

        Ok(handle)
    }

    async fn fetch_handle_from_pds(&self, did: &str) -> Result<String> {
        tracing::debug!("Fetching handle for DID: {}", did);

        let url = format!(
            "{}/xrpc/com.atproto.identity.resolveHandle?handle={}",
            self.config.pds_host, did
        );

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Failed to resolve DID {}: {}", did, response.status()));
        }

        let result: ResolveHandleResponse = response.json().await?;
        Ok(result.did)
    }

    fn is_fresh(&self, identity: &IdentityRow) -> bool {
        let age = Utc::now() - identity.last_updated;
        age < self.config.cache_ttl
    }

    pub async fn batch_resolve(&self, dids: &[String]) -> Result<Vec<(String, String)>> {
        let mut results = Vec::new();

        for did in dids {
            if let Ok(handle) = self.resolve_did_to_handle(did).await {
                results.push((did.clone(), handle));
            }
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = IdentityResolverConfig::default();
        assert_eq!(config.pds_host, "https://bsky.social");
        assert_eq!(config.cache_ttl, Duration::hours(24));
    }
}
