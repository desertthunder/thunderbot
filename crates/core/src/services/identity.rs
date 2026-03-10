//! Identity resolution module
//!
//! This module provides DID -> Handle resolution with TTL-based caching.
//!
//! Resolution logic:
//! 1. Check `identities` table for DID.
//! 2. If missing or stale (>24h), query `com.atproto.identity.resolveHandle` via XRPC.
//! 3. Upsert result.
//! 4. Expose `resolve_did_to_handle(did) -> String` helper.

use crate::db::models::{CreateIdentityParams, Identity};
use crate::db::repository::IdentityRepository;
use crate::error::BotError;
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use std::sync::Arc;

/// Default TTL for identity cache entries (24 hours)
pub const IDENTITY_TTL_HOURS: i64 = 24;

/// XRPC response for resolveHandle
#[derive(Debug, Clone, Deserialize)]
struct ResolveHandleResponse {
    did: String,
}

/// XRPC response for getProfile
#[derive(Debug, Clone, Deserialize)]
struct GetProfileResponse {
    // TODO: Useful for DID verification and debugging, currently unused but kept for completeness
    #[allow(dead_code)]
    did: String,
    handle: String,
    #[serde(rename = "displayName")]
    display_name: Option<String>,
}

/// Service for resolving and caching identities
pub struct IdentityResolver<R: IdentityRepository> {
    repo: R,
    pds_host: String,
    client: reqwest::Client,
    ttl_hours: i64,
}

impl<R: IdentityRepository> IdentityResolver<R> {
    /// Create a new identity resolver
    pub fn new(repo: R, pds_host: String) -> Self {
        Self { repo, pds_host, client: reqwest::Client::new(), ttl_hours: IDENTITY_TTL_HOURS }
    }

    /// Create a new identity resolver with custom TTL
    pub fn with_ttl(repo: R, pds_host: String, ttl_hours: i64) -> Self {
        Self { repo, pds_host, client: reqwest::Client::new(), ttl_hours }
    }

    /// Resolve a DID to a handle
    ///
    /// First checks the cache, then falls back to the PDS if needed.
    /// Updates the cache after a successful fetch.
    pub async fn resolve_did_to_handle(&self, did: &str) -> Result<String, BotError> {
        if let Some(identity) = self.repo.get_by_did(did).await? {
            if !self.is_stale(&identity) {
                tracing::debug!("Cache hit for DID {} -> {}", did, identity.handle);
                return Ok(identity.handle);
            }
            tracing::debug!("Cache entry for DID {} is stale, refreshing", did);
        } else {
            tracing::debug!("Cache miss for DID {}, fetching from PDS", did);
        }

        match self.fetch_identity_from_pds(did).await {
            Ok((handle, display_name)) => {
                let now = Utc::now().to_rfc3339();
                let params = CreateIdentityParams {
                    did: did.to_string(),
                    handle: handle.clone(),
                    display_name,
                    last_updated: now,
                };
                self.repo.upsert_identity(params).await?;
                tracing::info!("Cached identity {} -> {}", did, handle);
                Ok(handle)
            }
            Err(e) => match self.repo.get_by_did(did).await? {
                Some(identity) => {
                    tracing::warn!("Failed to refresh identity for {}, using stale entry: {}", did, e);
                    Ok(identity.handle)
                }
                None => Err(e),
            },
        }
    }

    /// Resolve a handle to a DID
    ///
    /// First checks the cache, then falls back to the PDS if needed.
    pub async fn resolve_handle_to_did(&self, handle: &str) -> Result<String, BotError> {
        match self.repo.get_by_handle(handle).await? {
            Some(identity) => {
                if !self.is_stale(&identity) {
                    tracing::debug!("Cache hit for handle {} -> {}", handle, identity.did);
                    return Ok(identity.did);
                }

                tracing::debug!("Cache entry for handle {} is stale, refreshing", handle);
            }
            None => tracing::debug!("Cache miss for handle {}, fetching from PDS", handle),
        }

        match self.fetch_did_from_pds(handle).await {
            Ok(did) => {
                match self.fetch_profile_from_pds(&did).await {
                    Ok((resolved_handle, display_name)) => {
                        let now = Utc::now().to_rfc3339();
                        let params = CreateIdentityParams {
                            did: did.clone(),
                            handle: resolved_handle,
                            display_name,
                            last_updated: now,
                        };
                        self.repo.upsert_identity(params).await?;
                        tracing::info!("Cached identity {} -> {}", did, handle);
                    }
                    Err(e) => {
                        tracing::warn!("Failed to fetch profile for {}: {}", did, e);

                        let now = Utc::now().to_rfc3339();
                        let params = CreateIdentityParams {
                            did: did.clone(),
                            handle: handle.to_string(),
                            display_name: None,
                            last_updated: now,
                        };
                        self.repo.upsert_identity(params).await?;
                    }
                }
                Ok(did)
            }
            Err(e) => match self.repo.get_by_handle(handle).await? {
                Some(identity) => {
                    tracing::warn!("Failed to refresh DID for handle {}, using stale entry: {}", handle, e);
                    Ok(identity.did)
                }
                None => Err(e),
            },
        }
    }

    /// Get an identity from cache (no fetch)
    pub async fn get_cached(&self, did: &str) -> Result<Option<Identity>, BotError> {
        self.repo.get_by_did(did).await
    }

    /// Check if an identity entry is stale
    fn is_stale(&self, identity: &Identity) -> bool {
        match DateTime::parse_from_rfc3339(&identity.last_updated) {
            Ok(last_updated) => {
                let stale_threshold = Utc::now() - Duration::hours(self.ttl_hours);
                last_updated < stale_threshold
            }
            Err(e) => {
                tracing::warn!("Failed to parse last_updated for {}: {}", identity.did, e);
                true
            }
        }
    }

    /// Refresh all stale identities
    ///
    /// Returns the number of identities refreshed.
    pub async fn refresh_stale_identities(&self) -> Result<usize, BotError> {
        let stale_threshold = (Utc::now() - Duration::hours(self.ttl_hours)).to_rfc3339();
        let stale = self.repo.get_stale_identities(&stale_threshold).await?;

        let mut refreshed = 0;
        for identity in stale {
            tracing::debug!("Refreshing stale identity: {}", identity.did);
            match self.fetch_identity_from_pds(&identity.did).await {
                Ok((handle, display_name)) => {
                    let now = Utc::now().to_rfc3339();
                    let params =
                        CreateIdentityParams { did: identity.did.clone(), handle, display_name, last_updated: now };
                    self.repo.upsert_identity(params).await?;
                    refreshed += 1;
                }
                Err(e) => tracing::warn!("Failed to refresh identity {}: {}", identity.did, e),
            }
        }

        tracing::info!("Refreshed {} stale identities", refreshed);
        Ok(refreshed)
    }

    /// Fetch identity information from PDS using resolveHandle
    ///
    /// For resolving DID -> Handle, we need to use getProfile because resolveHandle goes handle -> DID
    async fn fetch_identity_from_pds(&self, did: &str) -> Result<(String, Option<String>), BotError> {
        self.fetch_profile_from_pds(did).await
    }

    /// Fetch profile from PDS using app.bsky.actor.getProfile
    async fn fetch_profile_from_pds(&self, did: &str) -> Result<(String, Option<String>), BotError> {
        let url = format!("{}/xrpc/app.bsky.actor.getProfile?actor={}", self.pds_host, did);

        tracing::debug!("Fetching profile from: {}", url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| BotError::Validation(format!("HTTP request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(BotError::Validation(format!("PDS returned error {}: {}", status, body)));
        }

        let profile: GetProfileResponse = response
            .json()
            .await
            .map_err(|e| BotError::Validation(format!("Failed to parse response: {}", e)))?;

        Ok((profile.handle, profile.display_name))
    }

    /// Fetch DID from PDS using resolveHandle
    async fn fetch_did_from_pds(&self, handle: &str) -> Result<String, BotError> {
        let url = format!(
            "{}/xrpc/com.atproto.identity.resolveHandle?handle={}",
            self.pds_host, handle
        );

        tracing::debug!("Resolving handle from: {}", url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| BotError::Validation(format!("HTTP request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(BotError::Validation(format!("PDS returned error {}: {}", status, body)));
        }

        let result: ResolveHandleResponse = response
            .json()
            .await
            .map_err(|e| BotError::Validation(format!("Failed to parse response: {}", e)))?;

        Ok(result.did)
    }

    /// Get the TTL hours setting
    pub fn ttl_hours(&self) -> i64 {
        self.ttl_hours
    }
}

/// Shared identity resolver wrapped in Arc for thread-safe sharing
pub type SharedIdentityResolver<R> = Arc<IdentityResolver<R>>;

/// Create a shared identity resolver
pub fn create_shared_resolver<R: IdentityRepository>(repo: R, pds_host: String) -> SharedIdentityResolver<R> {
    Arc::new(IdentityResolver::new(repo, pds_host))
}
