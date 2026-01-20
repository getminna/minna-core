//! Provider configuration schema and loading.
//!
//! Providers are configured via a TOML file at `~/.minna/providers.toml`.
//! This allows enabling/disabling providers and configuring auth metadata
//! without code changes.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

/// Root configuration structure for all providers.
#[derive(Debug, Clone, Deserialize)]
pub struct ProvidersConfig {
    #[serde(default)]
    pub providers: HashMap<String, ProviderConfig>,
}

/// Configuration for a single provider.
#[derive(Debug, Clone, Deserialize)]
pub struct ProviderConfig {
    /// Whether this provider is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Human-readable display name (e.g., "Slack", "GitHub").
    pub display_name: String,

    /// Authentication configuration.
    pub auth: AuthConfig,

    /// Optional base URL for API requests.
    #[serde(default)]
    pub api_base_url: Option<String>,

    /// Optional environment variable overrides (e.g., batch limits).
    #[serde(default)]
    pub env_vars: HashMap<String, String>,
}

fn default_true() -> bool {
    true
}

/// Authentication configuration variants.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthConfig {
    /// Simple keychain token (Bearer auth).
    Keychain {
        /// Keychain account name (e.g., "slack_user_token").
        account: String,
        /// Optional expected token prefix for validation (e.g., "xoxp-").
        #[serde(default)]
        token_prefix: Option<String>,
    },

    /// Keychain token with Basic Auth (email:token format).
    KeychainBasic {
        /// Keychain account name storing "email:token".
        account: String,
    },

    /// OAuth2 with refresh token support.
    OAuth {
        /// Keychain account for access token.
        token_account: String,
        /// Keychain account for refresh token.
        refresh_account: String,
        /// Keychain account for client ID.
        client_id_account: String,
        /// Keychain account for client secret.
        client_secret_account: String,
    },

    /// No authentication required (local providers).
    None,
}

impl ProvidersConfig {
    /// Load configuration from a TOML file.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            // Return default config if file doesn't exist
            return Ok(Self::default());
        }

        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        toml::from_str(&contents)
            .with_context(|| format!("Failed to parse config file: {}", path.display()))
    }

    /// Get configuration for a specific provider.
    pub fn get(&self, name: &str) -> Option<&ProviderConfig> {
        self.providers.get(name)
    }

    /// Check if a provider is enabled.
    pub fn is_enabled(&self, name: &str) -> bool {
        self.providers
            .get(name)
            .map(|c| c.enabled)
            .unwrap_or(false)
    }

    /// List all enabled provider names.
    pub fn enabled_providers(&self) -> Vec<&str> {
        self.providers
            .iter()
            .filter(|(_, c)| c.enabled)
            .map(|(name, _)| name.as_str())
            .collect()
    }
}

impl Default for ProvidersConfig {
    fn default() -> Self {
        Self::with_defaults()
    }
}

impl ProvidersConfig {
    /// Create a config with built-in provider defaults.
    /// Used when no config file exists.
    pub fn with_defaults() -> Self {
        let mut providers = HashMap::new();

        // Slack
        providers.insert(
            "slack".to_string(),
            ProviderConfig {
                enabled: true,
                display_name: "Slack".to_string(),
                auth: AuthConfig::Keychain {
                    account: "slack_user_token".to_string(),
                    token_prefix: Some("xoxp-".to_string()),
                },
                api_base_url: None,
                env_vars: HashMap::new(),
            },
        );

        // GitHub
        providers.insert(
            "github".to_string(),
            ProviderConfig {
                enabled: true,
                display_name: "GitHub".to_string(),
                auth: AuthConfig::Keychain {
                    account: "github_pat".to_string(),
                    token_prefix: Some("github_pat_".to_string()),
                },
                api_base_url: Some("https://api.github.com".to_string()),
                env_vars: HashMap::new(),
            },
        );

        // Linear
        providers.insert(
            "linear".to_string(),
            ProviderConfig {
                enabled: true,
                display_name: "Linear".to_string(),
                auth: AuthConfig::Keychain {
                    account: "linear_token".to_string(),
                    token_prefix: None,
                },
                api_base_url: Some("https://api.linear.app/graphql".to_string()),
                env_vars: HashMap::new(),
            },
        );

        // Google Workspace
        providers.insert(
            "google".to_string(),
            ProviderConfig {
                enabled: true,
                display_name: "Google Workspace".to_string(),
                auth: AuthConfig::OAuth {
                    token_account: "googleWorkspace_token".to_string(),
                    refresh_account: "googleWorkspace_refresh_token".to_string(),
                    client_id_account: "google_client_id".to_string(),
                    client_secret_account: "google_client_secret".to_string(),
                },
                api_base_url: None,
                env_vars: HashMap::new(),
            },
        );

        // Notion
        providers.insert(
            "notion".to_string(),
            ProviderConfig {
                enabled: true,
                display_name: "Notion".to_string(),
                auth: AuthConfig::Keychain {
                    account: "notion_token".to_string(),
                    token_prefix: Some("secret_".to_string()),
                },
                api_base_url: Some("https://api.notion.com/v1".to_string()),
                env_vars: HashMap::new(),
            },
        );

        // Atlassian (Jira + Confluence)
        providers.insert(
            "atlassian".to_string(),
            ProviderConfig {
                enabled: true,
                display_name: "Atlassian (Jira/Confluence)".to_string(),
                auth: AuthConfig::KeychainBasic {
                    account: "atlassian_token".to_string(),
                },
                api_base_url: Some("https://api.atlassian.com".to_string()),
                env_vars: HashMap::new(),
            },
        );

        // Cursor (local)
        providers.insert(
            "cursor".to_string(),
            ProviderConfig {
                enabled: true,
                display_name: "Cursor".to_string(),
                auth: AuthConfig::None,
                api_base_url: None,
                env_vars: HashMap::new(),
            },
        );

        // Claude Code (local)
        providers.insert(
            "claude_code".to_string(),
            ProviderConfig {
                enabled: true,
                display_name: "Claude Code".to_string(),
                auth: AuthConfig::None,
                api_base_url: None,
                env_vars: HashMap::new(),
            },
        );

        Self { providers }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ProvidersConfig::default();
        assert!(config.is_enabled("slack"));
        assert!(config.is_enabled("github"));
        assert!(config.is_enabled("notion"));
        assert!(config.is_enabled("atlassian"));
    }

    #[test]
    fn test_parse_toml() {
        let toml = r#"
[providers.custom]
enabled = true
display_name = "Custom Provider"
[providers.custom.auth]
type = "keychain"
account = "custom_token"
"#;
        let config: ProvidersConfig = toml::from_str(toml).unwrap();
        assert!(config.is_enabled("custom"));
        assert_eq!(config.get("custom").unwrap().display_name, "Custom Provider");
    }
}
