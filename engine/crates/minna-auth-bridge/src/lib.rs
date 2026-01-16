use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use std::borrow::Cow;

use oauth2::{
    basic::BasicClient, AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken,
    EndpointNotSet, EndpointSet, RedirectUrl, RefreshToken, Scope, TokenResponse, TokenUrl,
};
use reqwest::redirect::Policy;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::info;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Slack,
    Github,
    Linear,
    Google,
}

impl Provider {
    pub fn as_str(&self) -> &'static str {
        match self {
            Provider::Slack => "slack",
            Provider::Github => "github",
            Provider::Linear => "linear",
            Provider::Google => "google",
        }
    }

    /// Get keychain account name for bot token (for providers that use bot+user tokens like Slack)
    fn bot_token_account(&self) -> String {
        format!("{}_bot_token", self.as_str())
    }

    /// Get keychain account name for user token or general access token
    fn user_token_account(&self) -> String {
        match self {
            Provider::Slack => "slack_user_token".to_string(),
            Provider::Github => "github_pat".to_string(),
            Provider::Google => "googleWorkspace_token".to_string(),
            _ => format!("{}_token", self.as_str()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthToken {
    pub provider: Provider,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub scope: Option<String>,
    pub token_type: Option<String>,
}

/// TokenStore now reads from macOS Keychain instead of JSON file
/// This matches the Swift CredentialManager implementation
#[derive(Debug, Clone)]
pub struct TokenStore {
    #[allow(dead_code)]
    path: PathBuf,  // Kept for API compatibility but not used
    service: String,
}

impl TokenStore {
    const KEYCHAIN_SERVICE: &'static str = "minna_ai";

    /// Load TokenStore (now just initializes keychain access)
    pub fn load(path: &Path) -> Result<Self> {
        // Create directory for compatibility with existing code
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        Ok(TokenStore {
            path: path.to_path_buf(),
            service: Self::KEYCHAIN_SERVICE.to_string(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Get token for a provider from macOS Keychain
    pub fn get(&self, provider: Provider) -> Option<AuthToken> {
        let account = provider.user_token_account();
        tracing::info!("Attempting to read token for {} from keychain account: {}", provider.as_str(), account);

        // Try to get user token first (primary token for most providers)
        let user_token = match self.get_keychain_token(&account) {
            Ok(token) => {
                tracing::info!("Successfully read token for {} (length: {})", provider.as_str(), token.len());
                token
            }
            Err(e) => {
                tracing::warn!("Failed to read token for {} from account {}: {}", provider.as_str(), account, e);

                // For Slack, try bot token as fallback
                if provider == Provider::Slack {
                    let bot_account = provider.bot_token_account();
                    tracing::info!("Trying fallback bot token account: {}", bot_account);
                    match self.get_keychain_token(&bot_account) {
                        Ok(token) => {
                            tracing::info!("Successfully read bot token for Slack (length: {})", token.len());
                            token
                        }
                        Err(e2) => {
                            tracing::warn!("Failed to read bot token: {}", e2);
                            return None;
                        }
                    }
                } else {
                    return None;
                }
            }
        };

        if user_token.is_empty() {
            tracing::warn!("Token for {} is empty", provider.as_str());
            return None;
        }

        tracing::info!("Returning token for {}", provider.as_str());
        Some(AuthToken {
            provider,
            access_token: user_token,
            refresh_token: None,  // Stored separately if needed
            expires_at: None,     // Could be enhanced to store metadata
            scope: None,
            token_type: Some("Bearer".to_string()),
        })
    }

    /// Set token for a provider in macOS Keychain
    pub fn set(&mut self, token: AuthToken) {
        let account = token.provider.user_token_account();
        if let Err(e) = self.set_keychain_token(&account, &token.access_token) {
            tracing::error!("Failed to save token to keychain for {}: {}", account, e);
        }

        // Save refresh token if present
        if let Some(refresh) = &token.refresh_token {
            let refresh_account = match token.provider {
                Provider::Google => "googleWorkspace_refresh_token".to_string(),
                _ => format!("{}_refresh_token", token.provider.as_str()),
            };
            if let Err(e) = self.set_keychain_token(&refresh_account, refresh) {
                tracing::error!("Failed to save refresh token to keychain: {}", e);
            }
        }
    }

    /// Save method kept for API compatibility (keychain saves are immediate)
    pub fn save(&self) -> Result<()> {
        // No-op: keychain writes are immediate in set()
        Ok(())
    }

    /// Reload method kept for API compatibility (keychain is always fresh)
    pub fn reload(&mut self) -> Result<()> {
        // No-op: keychain reads are always current in get()
        Ok(())
    }

    // Helper methods for keychain access
    // Using `security` command-line tool instead of keyring crate to avoid
    // cross-process Keychain access issues with macOS sandbox

    fn get_keychain_token(&self, account: &str) -> Result<String> {
        use std::process::Command;

        let output = Command::new("security")
            .args(["find-generic-password", "-s", &self.service, "-a", account, "-w"])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("Keychain read error: {}", stderr.trim()));
        }

        let token = String::from_utf8(output.stdout)?
            .trim()
            .to_string();

        Ok(token)
    }

    fn set_keychain_token(&self, account: &str, token: &str) -> Result<()> {
        use std::process::Command;

        // Try to delete existing entry first (ignore errors)
        let _ = Command::new("security")
            .args(["delete-generic-password", "-s", &self.service, "-a", account])
            .output();

        // Add new entry
        let output = Command::new("security")
            .args(["add-generic-password", "-s", &self.service, "-a", account, "-w", token])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("Keychain write error: {}", stderr.trim()));
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct OAuthConfig {
    pub client_id: String,
    pub client_secret: String,
    pub auth_url: String,
    pub token_url: String,
    pub redirect_uri: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AuthBridge {
    http_client: Client,
}

impl AuthBridge {
    pub fn new() -> Self {
        let http_client = Client::builder()
            .redirect(Policy::none())
            .build()
            .unwrap_or_else(|_| Client::new());
        Self { http_client }
    }

    pub fn authorize_url(
        &self,
        config: &OAuthConfig,
        scopes: &[&str],
    ) -> Result<(String, CsrfToken)> {
        let client = build_client(config)?;
        let mut req = client.authorize_url(CsrfToken::new_random);
        for scope in scopes {
            req = req.add_scope(Scope::new(scope.to_string()));
        }
        let (url, csrf) = req.url();
        Ok((url.to_string(), csrf))
    }

    pub async fn exchange_code(
        &self,
        provider: Provider,
        code: &str,
        config: &OAuthConfig,
    ) -> Result<AuthToken> {
        let client = build_client(config)?;
        let mut req = client.exchange_code(AuthorizationCode::new(code.to_string()));
        if let Some(redirect_uri) = &config.redirect_uri {
            req = req.set_redirect_uri(Cow::Owned(RedirectUrl::new(redirect_uri.to_string())?));
        }
        let token = req.request_async(&self.http_client).await?;

        let access_token = token.access_token().secret().to_string();
        let refresh_token = token.refresh_token().map(|t| t.secret().to_string());
        let expires_at = token
            .expires_in()
            .and_then(|d| chrono::Duration::from_std(d).ok())
            .map(|d| Utc::now() + d);
        let scope = token.scopes().map(|scopes| {
            scopes
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        });
        let token_type = Some(token.token_type().as_ref().to_string());

        info!("exchanged OAuth code for {} token", provider.as_str());
        Ok(AuthToken {
            provider,
            access_token,
            refresh_token,
            expires_at,
            scope,
            token_type,
        })
    }

    pub async fn refresh_token(
        &self,
        provider: Provider,
        refresh_token: &str,
        config: &OAuthConfig,
    ) -> Result<AuthToken> {
        let client = build_client(config)?;
        let token = client
            .exchange_refresh_token(&RefreshToken::new(refresh_token.to_string()))
            .request_async(&self.http_client)
            .await?;

        let access_token = token.access_token().secret().to_string();
        let refresh_token = token.refresh_token().map(|t| t.secret().to_string());
        let expires_at = token
            .expires_in()
            .and_then(|d| chrono::Duration::from_std(d).ok())
            .map(|d| Utc::now() + d);
        let scope = token.scopes().map(|scopes| {
            scopes
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        });
        let token_type = Some(token.token_type().as_ref().to_string());

        Ok(AuthToken {
            provider,
            access_token,
            refresh_token,
            expires_at,
            scope,
            token_type,
        })
    }
}

type ConfiguredClient = BasicClient<EndpointSet, EndpointNotSet, EndpointNotSet, EndpointNotSet, EndpointSet>;

fn build_client(config: &OAuthConfig) -> Result<ConfiguredClient> {
    let auth_url = AuthUrl::new(config.auth_url.clone())
        .map_err(|_| anyhow!("invalid auth_url"))?;
    let token_url = TokenUrl::new(config.token_url.clone())
        .map_err(|_| anyhow!("invalid token_url"))?;
    let mut client = BasicClient::new(ClientId::new(config.client_id.clone()))
        .set_client_secret(ClientSecret::new(config.client_secret.clone()))
        .set_auth_uri(auth_url)
        .set_token_uri(token_url);
    if let Some(redirect_uri) = &config.redirect_uri {
        client = client.set_redirect_uri(RedirectUrl::new(redirect_uri.clone())?);
    }
    Ok(client)
}
