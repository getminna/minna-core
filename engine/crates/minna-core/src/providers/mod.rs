//! Extensible provider system for data synchronization.
//!
//! This module provides:
//! - `SyncProvider` trait that all providers implement
//! - `ProviderRegistry` for managing and dispatching to providers
//! - Configuration loading from TOML
//!
//! # Adding a New Provider
//!
//! 1. Add config to `~/.minna/providers.toml`
//! 2. Create a new file in `providers/` implementing `SyncProvider`
//! 3. Register in `ProviderRegistry::register_builtin_providers()`

pub mod config;

mod notion;
mod atlassian;

pub use config::{AuthConfig, ProviderConfig, ProvidersConfig};
pub use notion::NotionProvider;
pub use atlassian::AtlassianProvider;

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
// serde re-exported from config module

use crate::{Document, IngestionEngine, Embedder, VectorStore};

// Re-export graph types for providers to use
pub use minna_graph::{ExtractedEdge, GraphStore, NodeRef, Relation, NodeType};

// Re-export the main SyncSummary from lib.rs
// This is defined in lib.rs line ~1930 and used by all sync methods
pub use crate::SyncSummary;

/// Context passed to providers during sync operations.
///
/// Contains all the shared resources providers need to index documents.
pub struct SyncContext<'a> {
    /// Document storage engine.
    pub ingest: &'a IngestionEngine,
    /// Vector embeddings storage.
    pub vector: &'a VectorStore,
    /// Embedding model.
    pub embedder: &'a Arc<dyn Embedder>,
    /// HTTP client for API requests.
    pub http_client: &'a reqwest::Client,
    /// Provider registry for token loading.
    pub registry: &'a ProviderRegistry,
    /// Graph store for relationship tracking (Gravity Well).
    pub graph: &'a GraphStore,
}

impl<'a> SyncContext<'a> {
    /// Index a document (store + embed + vectorize).
    pub async fn index_document(&self, doc: Document) -> Result<i64> {
        let id = self.ingest.upsert_document(&doc).await?;
        let embedding = self.embedder.embed(&doc.body).await?;
        self.vector.upsert_embedding(id, &embedding).await?;
        Ok(id)
    }

    /// Get sync cursor for incremental syncing.
    pub async fn get_sync_cursor(&self, provider: &str) -> Result<Option<String>> {
        self.ingest.get_sync_cursor(provider).await
    }

    /// Set sync cursor after successful sync.
    pub async fn set_sync_cursor(&self, provider: &str, cursor: &str) -> Result<()> {
        self.ingest.set_sync_cursor(provider, cursor).await
    }

    /// Store extracted edges in the graph (Gravity Well).
    ///
    /// Upserts nodes and edges. The GraphStore handles node creation internally.
    pub async fn index_edges(&self, edges: &[ExtractedEdge]) -> Result<usize> {
        let mut count = 0;
        for edge in edges {
            // upsert_edge handles node creation internally
            self.graph.upsert_edge(edge).await?;
            count += 1;
        }
        Ok(count)
    }
}

/// Trait that all sync providers must implement.
#[async_trait]
pub trait SyncProvider: Send + Sync {
    /// Provider identifier (e.g., "notion", "slack").
    fn name(&self) -> &'static str;

    /// Human-readable display name (e.g., "Notion", "Slack").
    fn display_name(&self) -> &'static str;

    /// Sync documents from this provider.
    ///
    /// # Arguments
    /// * `ctx` - Shared context with storage and HTTP client
    /// * `since_days` - Optional number of days to look back
    /// * `mode` - Optional sync mode ("full", "sprint", etc.)
    async fn sync(
        &self,
        ctx: &SyncContext<'_>,
        since_days: Option<i64>,
        mode: Option<&str>,
    ) -> Result<SyncSummary>;

    /// Optional: Quick discovery scan for UI metadata.
    ///
    /// Returns counts, available resources, etc. without full sync.
    async fn discover(&self, _ctx: &SyncContext<'_>) -> Result<serde_json::Value> {
        Ok(serde_json::json!({
            "provider": self.name(),
            "error": "discovery not implemented"
        }))
    }

    /// Extract relationship edges from synced data.
    ///
    /// Called after sync to populate the Gravity Well graph.
    /// Default implementation returns empty - providers opt-in by overriding.
    ///
    /// # Arguments
    /// * `ctx` - Shared context with graph store
    /// * `doc` - The document that was just synced
    /// * `raw_data` - Optional raw API response for richer extraction
    async fn extract_edges(
        &self,
        _ctx: &SyncContext<'_>,
        _doc: &Document,
        _raw_data: Option<&serde_json::Value>,
    ) -> Result<Vec<ExtractedEdge>> {
        Ok(Vec::new())
    }
}

/// Registry managing all available providers.
pub struct ProviderRegistry {
    config: ProvidersConfig,
    providers: HashMap<String, Arc<dyn SyncProvider>>,
}

impl ProviderRegistry {
    /// Create a new registry, loading config from the specified path.
    ///
    /// Falls back to default config if file doesn't exist.
    pub fn new(config_path: &Path) -> Result<Self> {
        let config = ProvidersConfig::load(config_path)?;
        let providers = Self::register_builtin_providers(&config);
        Ok(Self { config, providers })
    }

    /// Create a registry with default configuration.
    pub fn with_defaults() -> Self {
        let config = ProvidersConfig::default();
        let providers = Self::register_builtin_providers(&config);
        Self { config, providers }
    }

    /// Register all built-in providers based on config.
    fn register_builtin_providers(config: &ProvidersConfig) -> HashMap<String, Arc<dyn SyncProvider>> {
        let mut map: HashMap<String, Arc<dyn SyncProvider>> = HashMap::new();

        // New extensible providers
        if config.is_enabled("notion") {
            map.insert("notion".to_string(), Arc::new(NotionProvider));
        }
        if config.is_enabled("atlassian") {
            map.insert("atlassian".to_string(), Arc::new(AtlassianProvider));
        }

        // Legacy providers will be migrated here:
        // if config.is_enabled("slack") {
        //     map.insert("slack".to_string(), Arc::new(SlackProvider));
        // }
        // if config.is_enabled("github") {
        //     map.insert("github".to_string(), Arc::new(GithubProvider));
        // }
        // if config.is_enabled("linear") {
        //     map.insert("linear".to_string(), Arc::new(LinearProvider));
        // }
        // if config.is_enabled("google") {
        //     map.insert("google".to_string(), Arc::new(GoogleProvider));
        // }

        map
    }

    /// Get a provider by name.
    pub fn get(&self, name: &str) -> Option<Arc<dyn SyncProvider>> {
        self.providers.get(name).cloned()
    }

    /// Get configuration for a provider.
    pub fn get_config(&self, name: &str) -> Option<&ProviderConfig> {
        self.config.get(name)
    }

    /// Check if a provider is registered and enabled.
    pub fn is_available(&self, name: &str) -> bool {
        self.providers.contains_key(name)
    }

    /// List all available (registered + enabled) provider names.
    pub fn list_available(&self) -> Vec<&str> {
        self.providers.keys().map(|s| s.as_str()).collect()
    }

    /// Load authentication token for a provider using its config.
    pub fn load_token(&self, name: &str) -> Result<String> {
        let config = self.get_config(name)
            .ok_or_else(|| anyhow!("Unknown provider: {}", name))?;

        match &config.auth {
            AuthConfig::Keychain { account, .. } => keychain_get(account),
            AuthConfig::KeychainBasic { account } => keychain_get(account),
            AuthConfig::OAuth { token_account, .. } => keychain_get(token_account),
            AuthConfig::None => Ok(String::new()),
        }
    }

    /// Load OAuth credentials (for providers that need refresh).
    pub fn load_oauth_credentials(&self, name: &str) -> Result<OAuthCredentials> {
        let config = self.get_config(name)
            .ok_or_else(|| anyhow!("Unknown provider: {}", name))?;

        match &config.auth {
            AuthConfig::OAuth {
                token_account,
                refresh_account,
                client_id_account,
                client_secret_account,
            } => Ok(OAuthCredentials {
                access_token: keychain_get(token_account)?,
                refresh_token: keychain_get(refresh_account).ok(),
                client_id: keychain_get(client_id_account)?,
                client_secret: keychain_get(client_secret_account)?,
            }),
            _ => Err(anyhow!("Provider {} does not use OAuth", name)),
        }
    }

    /// Parse Basic Auth credentials (email:token format).
    pub fn parse_basic_auth(&self, name: &str) -> Result<(String, String)> {
        let creds = self.load_token(name)?;
        let parts: Vec<&str> = creds.splitn(2, ':').collect();
        if parts.len() != 2 {
            return Err(anyhow!("Invalid Basic Auth format for {}. Expected 'email:token'", name));
        }
        Ok((parts[0].to_string(), parts[1].to_string()))
    }
}

/// OAuth credentials bundle.
#[derive(Debug, Clone)]
pub struct OAuthCredentials {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub client_id: String,
    pub client_secret: String,
}

/// Read a value from the macOS Keychain.
fn keychain_get(account: &str) -> Result<String> {
    use std::process::Command;

    let output = Command::new("security")
        .args(["find-generic-password", "-s", "minna_ai", "-a", account, "-w"])
        .output()
        .map_err(|e| anyhow!("Failed to run security command: {}", e))?;

    if !output.status.success() {
        return Err(anyhow!(
            "Token not found for '{}'. Run: minna add {}",
            account,
            account.replace("_token", "").replace("_pat", "")
        ));
    }

    let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if token.is_empty() {
        return Err(anyhow!("Empty token for '{}'", account));
    }

    Ok(token)
}

/// Calculate the "since" timestamp for sync operations.
pub fn calculate_since(
    since_days: Option<i64>,
    mode: Option<&str>,
    cursor: Option<&str>,
) -> DateTime<Utc> {
    let is_full = mode == Some("full");

    if is_full {
        // Full sync: default 90 days
        let days = since_days.unwrap_or(90);
        Utc::now() - chrono::Duration::days(days)
    } else if let Some(days) = since_days {
        // Explicit days override
        Utc::now() - chrono::Duration::days(days)
    } else if let Some(cursor_str) = cursor {
        // Use cursor if available
        DateTime::parse_from_rfc3339(cursor_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now() - chrono::Duration::days(30))
    } else {
        // Default: 30 days
        Utc::now() - chrono::Duration::days(30)
    }
}

/// HTTP request helper with exponential backoff for rate limiting.
pub async fn call_with_backoff<F>(
    provider: &str,
    mut builder_fn: F,
) -> Result<reqwest::Response>
where
    F: FnMut() -> reqwest::RequestBuilder,
{
    use std::time::Duration;
    use tokio::time::sleep;

    let mut retries = 0;
    let mut delay = Duration::from_secs(1);
    let max_retries = 8;

    loop {
        let response = builder_fn().send().await?;
        let status = response.status();

        if status.is_success() {
            return Ok(response);
        }

        if status.as_u16() == 429 {
            // Rate limited
            if retries >= max_retries {
                return Err(anyhow!("{}: Rate limited after {} retries", provider, retries));
            }

            // Check for Retry-After header
            let wait = response
                .headers()
                .get("Retry-After")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .map(Duration::from_secs)
                .unwrap_or(delay);

            tracing::warn!("{}: Rate limited, waiting {:?}", provider, wait);
            sleep(wait).await;

            retries += 1;
            delay = std::cmp::min(delay * 2, Duration::from_secs(60));
            continue;
        }

        if status.is_server_error() && retries < 3 {
            tracing::warn!("{}: Server error {}, retrying...", provider, status);
            sleep(delay).await;
            retries += 1;
            delay *= 2;
            continue;
        }

        if status.as_u16() == 403 {
            return Err(anyhow!("{}: Access forbidden (403). Check permissions.", provider));
        }

        return Err(anyhow!("{}: HTTP {} - {}", provider, status, response.text().await.unwrap_or_default()));
    }
}
