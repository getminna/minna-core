use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::collections::HashMap;
use regex::Regex;

use anyhow::Result;
use chrono::{DateTime, Utc};
use reqwest::redirect::Policy;
use serde::{Deserialize, Serialize};
use base64::Engine;
use std::time::Duration;
use tracing::{info, warn};

pub mod progress;
pub mod providers;
pub mod scheduler;
pub mod tools;

pub use progress::{emit_progress, emit_result, emit_error, emit_warmup_progress, emit_ready};
pub use providers::{ProviderRegistry, SyncProvider, SyncContext, ProvidersConfig};
pub use scheduler::{SyncScheduler, SyncDepth, SchedulerConfig, ScheduledSync, SyncPlanner};
pub use tools::{Checkpoint, CheckpointStore, LoadQuery};
// SyncSummary is defined below and re-exported from providers for convenience

pub use minna_auth_bridge::{AuthToken, TokenStore};
pub use minna_ingest::{Document, IngestionEngine};
pub use minna_vector::{embedder_from_env_or_hash, Cluster, Embedder, VectorStore};

#[derive(Debug, Clone)]
pub struct MinnaPaths {
    pub base_dir: PathBuf,
    pub db_path: PathBuf,
    pub auth_path: PathBuf,
    pub socket_path: PathBuf,        // mcp.sock - AI clients (read-only)
    pub admin_socket_path: PathBuf,  // admin.sock - Swift app (control)
    pub entitlement_path: PathBuf,
}

impl MinnaPaths {
    pub fn from_env() -> Self {
        if let Some(dir) = std::env::var_os("MINNA_DATA_DIR") {
            let base_dir = PathBuf::from(dir);
            return Self::from_base(base_dir);
        }
        if let Some(home) = std::env::var_os("HOME") {
            let base_dir = PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("Minna");
            return Self::from_base(base_dir);
        }
        Self::from_base(PathBuf::from(".minna"))
    }

    pub fn from_base(base_dir: PathBuf) -> Self {
        let db_path = base_dir.join("minna.db");
        let auth_path = base_dir.join("auth.json");
        let socket_path = base_dir.join("mcp.sock");
        let admin_socket_path = base_dir.join("admin.sock");
        let entitlement_path = base_dir.join("entitlement.jwe");
        Self {
            base_dir,
            db_path,
            auth_path,
            socket_path,
            admin_socket_path,
            entitlement_path,
        }
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        std::fs::create_dir_all(&self.base_dir)?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct Core {
    pub ingest: IngestionEngine,
    pub vector: VectorStore,
    pub auth: TokenStore,
    pub embedder: Arc<dyn Embedder>,
    pub graph: minna_graph::GraphStore,
}

impl Core {
    pub async fn init(paths: &MinnaPaths) -> Result<Self> {
        info!("Initializing Minna Core...");
        paths.ensure_dirs()?;
        let ingest = IngestionEngine::new(&paths.db_path).await?;
        let vector = VectorStore::new(&paths.db_path).await?;
        let auth = TokenStore::load(&paths.auth_path)?;
        let embedder = embedder_from_env_or_hash();
        // Initialize GraphStore using the same pool as ingest
        let graph = minna_graph::GraphStore::new(ingest.pool().clone());
        // Ensure graph schema is initialized
        minna_graph::GraphStore::init_schema(ingest.pool()).await?;
        Ok(Self {
            ingest,
            vector,
            auth,
            embedder,
            graph,
        })
    }

    pub fn auth_path(&self) -> Result<PathBuf> {
        Ok(self.auth.path().to_path_buf())
    }

    pub async fn index_document(&self, doc: Document) -> Result<i64> {
        let id = self.ingest.upsert_document(&doc).await?;
        let embedding = self.embedder.embed(&doc.body).await?;
        self.vector.upsert_embedding(id, &embedding).await?;
        Ok(id)
    }

    pub async fn run_clustering(
        &self,
        min_similarity: f32,
        min_points: usize,
    ) -> Result<Vec<Cluster>> {
        let clusters = self.vector.cluster_documents(min_similarity, min_points).await?;
        let records = clusters
            .iter()
            .map(|cluster| minna_ingest::ClusterRecord {
                id: None,
                label: cluster.label.clone(),
                doc_ids: cluster.doc_ids.clone(),
                created_at: Utc::now(),
            })
            .collect::<Vec<_>>();
        self.ingest.store_clusters(&records).await?;
        Ok(clusters)
    }

    pub async fn reset_provider(&self, provider_id: &str) -> Result<()> {
        info!("Resetting provider: {}", provider_id);
        // 1. Delete sync cursor (prevents delta sync)
        self.ingest.set_sync_cursor(provider_id, "").await?;
        // 2. Delete documents from this provider
        self.ingest.delete_documents_by_source(provider_id).await?;
        // 3. Scrub orphaned embeddings
        self.vector.scrub_orphaned_embeddings().await?;
        Ok(())
    }

    /// Sync a provider using the extensible provider registry.
    ///
    /// This is the preferred method for new providers (Notion, Atlassian, etc.).
    /// Legacy providers (Slack, GitHub, etc.) still use the direct sync_* methods
    /// until they are migrated.
    pub async fn sync_via_registry(
        &self,
        registry: &ProviderRegistry,
        provider_name: &str,
        since_days: Option<i64>,
        mode: Option<&str>,
    ) -> Result<providers::SyncSummary> {
        let provider = registry.get(provider_name)
            .ok_or_else(|| anyhow::anyhow!("Unknown or disabled provider: {}", provider_name))?;

        // Create HTTP client
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .redirect(Policy::limited(5))
            .build()?;

        // Get graph store for Gravity Well
        let graph = self.ingest.graph_store();
        let auth_path = self.auth.path();

        // Create sync context
        let ctx = SyncContext {
            ingest: &self.ingest,
            vector: &self.vector,
            embedder: &self.embedder,
            http_client: &http_client,
            registry,
            graph: &graph,
            auth_path,
        };

        provider.sync(&ctx, since_days, mode).await
    }

    /// Discover resources for a provider using the extensible registry.
    pub async fn discover_via_registry(
        &self,
        registry: &ProviderRegistry,
        provider_name: &str,
    ) -> Result<serde_json::Value> {
        let provider = registry.get(provider_name)
            .ok_or_else(|| anyhow::anyhow!("Unknown or disabled provider: {}", provider_name))?;

        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;

        // Get graph store for Gravity Well
        let graph = self.ingest.graph_store();
        let auth_path = self.auth.path();

        let ctx = SyncContext {
            ingest: &self.ingest,
            vector: &self.vector,
            embedder: &self.embedder,
            http_client: &http_client,
            registry,
            graph: &graph,
            auth_path,
        };

        provider.discover(&ctx).await
    }
}

async fn call_with_backoff(
    provider: &str,
    mut builder_fn: impl FnMut() -> reqwest::RequestBuilder,
) -> Result<reqwest::Response> {
    let mut retries = 0;
    let mut delay = Duration::from_secs(1);
    let max_retries = 8;

    loop {
        let response = builder_fn().send().await?;
        let status = response.status();

        if status.is_success() {
            return Ok(response);
        }

        if status.as_u16() == 429 && retries < max_retries {
            let retry_after = response.headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .map(Duration::from_secs)
                .unwrap_or(delay);

            warn!(
                "[{}] Rate limited (429). Retrying in {:?} (attempt {}/{})",
                provider, retry_after, retries + 1, max_retries
            );
            emit_progress(provider, "syncing", &format!("Rate limited, waiting {:?}s...", retry_after.as_secs()), None);
            
            tokio::time::sleep(retry_after).await;
            retries += 1;
            delay *= 2;
            continue;
        }

        if status.is_server_error() && retries < 3 {
            warn!("[{}] Server error ({}). Retrying...", provider, status);
            tokio::time::sleep(delay).await;
            retries += 1;
            delay *= 2;
            continue;
        }

        // Don't retry on 403 Forbidden - it's a permanent permission error
        if status.as_u16() == 403 {
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("{} API failed ({}): {}", provider, status, body));
        }

        let body = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("{} API failed ({}): {}", provider, status, body));
    }
}

impl Core {

    pub async fn sync_github(
        &self,
        since_days: Option<i64>,
        mode: Option<&str>,
    ) -> Result<SyncSummary> {
        let is_full_sync = mode == Some("full");
        info!("Starting GitHub sync (since_days: {:?}, mode: {:?})", since_days, mode);

        let token_store = TokenStore::load(self.auth.path())?;
        let token = token_store
            .get(minna_auth_bridge::Provider::Github)
            .ok_or_else(|| anyhow::anyhow!("missing github token"))?;

        let since = if is_full_sync {
            let days = since_days.unwrap_or(90); // Default to 90 days (1 quarter)
            info!("GitHub: performing full sync (last {} days)", days);
            (Utc::now() - chrono::Duration::days(days)).to_rfc3339()
        } else if let Some(days) = since_days {
            info!("GitHub: performing quick sync (last {} days)", days);
            (Utc::now() - chrono::Duration::days(days)).to_rfc3339()
        } else {
            let cursor = self.ingest.get_sync_cursor("github_cursor").await?.unwrap_or_default();
            if cursor.is_empty() {
                info!("GitHub: no cursor found, defaulting to 30 days");
                (Utc::now() - chrono::Duration::days(30)).to_rfc3339()
            } else {
                info!("GitHub: performing delta sync from cursor: {}", cursor);
                cursor
            }
        };

        info!("GitHub sync window starting from: {}", since);

        let repo_limit = if is_full_sync {
            std::env::var("MINNA_GITHUB_REPO_LIMIT_FULL")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1000usize)
        } else {
            std::env::var("MINNA_GITHUB_REPO_LIMIT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(25usize)
        };

        let issue_limit = if is_full_sync {
            std::env::var("MINNA_GITHUB_ISSUE_LIMIT_FULL")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(500usize)
        } else {
            std::env::var("MINNA_GITHUB_ISSUE_LIMIT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(50usize)
        };

        let client = reqwest::Client::builder()
            .user_agent("minna-core")
            .redirect(Policy::none())
            .build()?;

        let mut repos = Vec::new();
        let mut page = 1;
        while repos.len() < repo_limit {
            let url = format!(
                "https://api.github.com/user/repos?per_page=100&page={}",
                page
            );
            let response = call_with_backoff("github", || {
                client.get(&url).header("Authorization", format!("token {}", token.access_token))
            }).await?;
            
            let mut batch: Vec<GithubRepo> = response.json().await?;
            if batch.is_empty() {
                break;
            }
            repos.append(&mut batch);
            if repos.len() >= repo_limit {
                break;
            }
            page += 1;
        }
        info!("Found {} GitHub repositories", repos.len());
        emit_progress("github", "syncing", &format!("Found {} repositories", repos.len()), Some(0));

        let mut docs_indexed = 0usize;
        let mut repos_scanned = 0usize;
        for repo in repos.into_iter().take(repo_limit) {
            repos_scanned += 1;
            let url = format!(
                "https://api.github.com/repos/{}/{}/issues?state=all&since={}&per_page={}",
                repo.owner.login, repo.name, since, issue_limit
            );
            let response = call_with_backoff("github", || {
                client.get(&url).header("Authorization", format!("token {}", token.access_token))
            }).await?;

            let issues: Vec<GithubIssue> = response.json().await.unwrap_or_default();
            for issue in issues {
                if issue.pull_request.is_none() {
                    continue;
                }
                let updated_at = DateTime::parse_from_rfc3339(&issue.updated_at)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());
                let body = issue.body.unwrap_or_default();
                let doc = Document {
                    id: None,
                    uri: issue.html_url.clone(),
                    source: "github".to_string(),
                    title: Some(issue.title.clone()),
                    body: format!(
                        "# {}\\n\\n- Repo: {}/{}\\n- Number: {}\\n- Updated: {}\\n- URL: {}\\n\\n{}",
                        issue.title,
                        repo.owner.login,
                        repo.name,
                        issue.number,
                        issue.updated_at,
                        issue.html_url,
                        body
                    ),
                    updated_at,
                };
                let _ = self.index_document(doc).await?;
                docs_indexed += 1;
                if docs_indexed.is_multiple_of(5) {
                    emit_progress("github", "syncing", &format!("Indexing issues: {} documents", docs_indexed), Some(docs_indexed));
                }
            }
        }

        let cursor = Utc::now().to_rfc3339();
        let _ = self.ingest.set_sync_cursor("github_cursor", &cursor).await;

        info!("GitHub sync complete: {} repos scanned, {} docs indexed", repos_scanned, docs_indexed);

        Ok(SyncSummary {
            provider: "github".to_string(),
            items_scanned: repos_scanned,
            documents_processed: docs_indexed,
            updated_at: cursor,
        })
    }

    pub async fn sync_slack(
        &self,
        since_days: Option<i64>,
        mode: Option<&str>,
    ) -> Result<SyncSummary> {
        info!("Starting Slack USER sync (since_days: {:?}, mode: {:?})", since_days, mode);
        let token_store = TokenStore::load(self.auth.path())?;
        let token = token_store
            .get(minna_auth_bridge::Provider::Slack)
            .ok_or_else(|| anyhow::anyhow!("missing slack token"))?;

        let client = reqwest::Client::builder()
            .user_agent("minna-core")
            .redirect(Policy::none())
            .build()?;

        // Get own user ID for mention detection
        let auth_response = client.post("https://slack.com/api/auth.test")
            .header("Authorization", format!("Bearer {}", token.access_token))
            .send().await?;
        let status = auth_response.status();
        let auth_test: SlackAuthTestResponse = auth_response.json().await
            .map_err(|e| anyhow::anyhow!("Failed to decode auth.test response (status {}): {}", status, e))?;
        let my_user_id = auth_test.user_id.clone().unwrap_or_default();
        info!("Slack sync context: my_user_id={}", my_user_id);

        // Build User Directory Cache (Standard Legacy Logic)
        let mut user_cache = HashMap::new();
        let mut user_cursor: Option<String> = None;
        loop {
            let mut u_params = vec![("limit", "1000".to_string())];
            if let Some(c) = user_cursor.as_ref() {
                u_params.push(("cursor", c.clone()));
            }
            let u_response = call_with_backoff("slack", || {
                client.get("https://slack.com/api/users.list")
                    .header("Authorization", format!("Bearer {}", token.access_token))
                    .query(&u_params)
            }).await?;
            let status = u_response.status();
            let u_payload: SlackUsersResponse = u_response.json().await
                .map_err(|e| anyhow::anyhow!("Failed to decode users.list response (status {}): {}", status, e))?;
            if !u_payload.ok { break; }
            if let Some(members) = u_payload.members {
                for member in members {
                    let name = member.profile.real_name
                        .or(member.profile.display_name)
                        .unwrap_or_else(|| member.id.clone());
                    user_cache.insert(member.id, name);
                }
            }
            user_cursor = u_payload.response_metadata
                .and_then(|m| m.next_cursor)
                .filter(|c: &String| !c.is_empty());
            if user_cursor.is_none() { break; }
        }
        info!("Slack user directory cached: {} users", user_cache.len());
        if let Some((id, name)) = user_cache.iter().next() {
            info!("  -> Sample resolution: {} -> {}", id, name);
        }

        let is_full_sync = mode == Some("full");
        let oldest = if is_full_sync {
            let days = since_days.unwrap_or(90); // Default to 90 days (1 quarter)
            info!("Slack: performing full sync (last {} days)", days);
            slack_ts_from_datetime(Utc::now() - chrono::Duration::days(days))
        } else if let Some(days) = since_days {
            info!("Slack: performing quick sync (last {} days)", days);
            slack_ts_from_datetime(Utc::now() - chrono::Duration::days(days))
        } else {
            let cursor = self.ingest.get_sync_cursor("slack").await?.unwrap_or_default();
            if cursor.is_empty() {
                info!("Slack: no cursor found, defaulting to 30 days");
                slack_ts_from_datetime(Utc::now() - chrono::Duration::days(30))
            } else {
                info!("Slack: performing delta sync from cursor: {}", cursor);
                cursor
            }
        };

        let start_date_str = slack_ts_to_datetime(&oldest)
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|| "unknown".to_string());
        info!("Slack sync window starting from: {} (ts: {})", start_date_str, oldest);

        let channel_limit = if is_full_sync {
            std::env::var("MINNA_SLACK_CHANNEL_LIMIT_FULL")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1000usize)
        } else {
            std::env::var("MINNA_SLACK_CHANNEL_LIMIT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(200usize) // Increased default
        };

        let message_limit = if is_full_sync {
            std::env::var("MINNA_SLACK_MESSAGE_LIMIT_FULL")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1000usize)
        } else {
            std::env::var("MINNA_SLACK_MESSAGE_LIMIT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(200usize)
        };

        let mut channels = Vec::new();
        let mut cursor: Option<String> = None;
        while channels.len() < channel_limit {
            let mut params: Vec<(&str, String)> = vec![
                ("limit", "200".to_string()),
                ("types", "public_channel,private_channel,mpim,im".to_string()),
            ];
            if let Some(next) = cursor.as_ref() {
                params.push(("cursor", next.clone()));
            }
            let response = call_with_backoff("slack", || {
                client.get("https://slack.com/api/users.conversations")
                    .header("Authorization", format!("Bearer {}", token.access_token))
                    .query(&params)
            }).await?;
            
            let status = response.status();
            let payload: SlackChannelsResponse = response.json().await
                .map_err(|e| anyhow::anyhow!("Failed to decode users.conversations response (status {}): {}", status, e))?;
            if !payload.ok {
                return Err(anyhow::anyhow!(
                    "slack conversations.list failed: {}",
                    payload.error.unwrap_or_else(|| "unknown".to_string())
                ));
            }
            if let Some(mut batch) = payload.channels {
                channels.append(&mut batch);
            }
            cursor = payload
                .response_metadata
                .and_then(|meta| meta.next_cursor)
                .filter(|c| !c.is_empty());
            if cursor.is_none() {
                break;
            }
        }
        let channels = channels.into_iter().collect::<Vec<_>>();
        info!("Scanning messages in {} Slack channels (including DMs and Private Groups)", channels.len());
        
        // Separate channels into DMs (individual + group) and regular channels (public + private)
        let mut dms = Vec::new();
        let mut regular_channels = Vec::new();
        
        for channel in &channels {
            // DMs: individual DMs (is_im) or group DMs (is_mpim)
            if channel.is_im == Some(true) || channel.is_mpim == Some(true) {
                dms.push(channel.clone());
            } else {
                // Regular channels: public or private channels
                regular_channels.push(channel.clone());
            }
        }
        
        info!("Processing {} DMs (individual + group) and {} channels (public + private)", dms.len(), regular_channels.len());

        let mut max_ts = oldest.parse::<f64>().unwrap_or(0.0);
        let mut docs_indexed = 0usize;
        let mut channels_scanned = 0usize;
        
        // Process DMs first
        if !dms.is_empty() {
            emit_progress("slack", "syncing", "Checking your DMs...", Some(docs_indexed));
            for channel in &dms {
                channels_scanned += 1;
                let channel_name = channel.name.as_ref()
                    .or(channel.name_normalized.as_ref())
                    .map(|s| s.as_str())
                    .unwrap_or_else(|| if channel.is_im == Some(true) { "DM" } else { "Unnamed" });
                info!("  -> Scanning channel: #{} ({})", channel_name, channel.id);
                emit_progress("slack", "syncing", &format!("Scanning #{}", channel_name), Some(docs_indexed));
                
                let mut history_cursor: Option<String> = None;
                loop {
                let mut params = vec![
                    ("channel", channel.id.clone()),
                    ("oldest", oldest.clone()),
                    ("limit", "1000".to_string()),
                ];
                if let Some(c) = history_cursor.as_ref() {
                    params.push(("cursor", c.clone()));
                }

                let response = call_with_backoff("slack", || {
                    client.get("https://slack.com/api/conversations.history")
                        .header("Authorization", format!("Bearer {}", token.access_token))
                        .query(&params)
                }).await?;

                let status = response.status();
                let payload: SlackHistoryResponse = response.json().await
                    .map_err(|e| anyhow::anyhow!("Failed to decode conversations.history response for channel {} (status {}): {}", channel.id, status, e))?;
                if !payload.ok {
                    warn!("Slack history failed for channel {}: {:?}", channel.id, payload.error);
                    break;
                }
                
                if let Some(messages) = payload.messages {
                    let channel_name = channel.name.as_ref()
                        .or(channel.name_normalized.as_ref())
                        .map(|s| s.as_str())
                        .unwrap_or_else(|| if channel.is_im == Some(true) { "DM" } else { "Unnamed" });
                    info!("  -> Found {} messages in page for channel {}", messages.len(), channel_name);
                    if messages.is_empty() { break; }
                    for message in messages {
                        // Skip replies in the main history loop (Legacy Logic: they are handled via parent replies)
                        if let Some(ref t_ts) = message.thread_ts {
                            if t_ts != &message.ts {
                                continue;
                            }
                        }

                        if let Some(text) = message.text.as_ref() {
                            let ts_val = message.ts.parse::<f64>().unwrap_or(0.0);
                            if ts_val > max_ts {
                                max_ts = ts_val;
                            }
                            let updated_at = slack_ts_to_datetime(&message.ts).unwrap_or_else(Utc::now);
                            let permalink = slack_permalink(&channel.id, &message.ts);
                            
                            let author_name = resolve_slack_name(message.user.as_ref(), &user_cache);
                            let clean_body_text = clean_slack_text(text, &user_cache);
                            
                            let channel_name = channel.name.as_ref()
                                .or(channel.name_normalized.as_ref())
                                .map(|s| s.as_str())
                                .unwrap_or_else(|| if channel.is_im == Some(true) { "DM" } else { "Unnamed" });
                            let mut full_body = format!(
                                "# Slack Thread: #{}\\n- Author: {}\\n- Created: {}\\n- URL: {}\\n\\n**{}**: {}",
                                channel_name,
                                author_name,
                                updated_at.to_rfc3339(),
                                permalink,
                                author_name,
                                clean_body_text
                            );

                            // Thread support: If this is a thread parent, fetch and CONSOLIDATE all replies (Legacy Standard)
                            if let Some(reply_count) = message.reply_count {
                                if reply_count > 0 {
                                    info!("    -> Consolidating {} replies for thread {}", reply_count, message.ts);
                                    let mut reply_cursor: Option<String> = None;
                                    loop {
                                        let mut r_params = vec![
                                            ("channel", channel.id.clone()),
                                            ("ts", message.ts.clone()),
                                            ("limit", "100".to_string()),
                                        ];
                                        if let Some(rc) = reply_cursor.as_ref() {
                                            r_params.push(("cursor", rc.clone()));
                                        }

                                        let r_response = call_with_backoff("slack", || {
                                            client.get("https://slack.com/api/conversations.replies")
                                                .header("Authorization", format!("Bearer {}", token.access_token))
                                                .query(&r_params)
                                        }).await?;

                                        let status = r_response.status();
                                        let r_payload: SlackHistoryResponse = r_response.json().await
                                            .map_err(|e| anyhow::anyhow!("Failed to decode conversations.replies response for thread {} (status {}): {}", message.ts, status, e))?;
                                        if !r_payload.ok { break; }

                                        if let Some(replies) = r_payload.messages {
                                            for reply in replies {
                                                // Skip the parent as it's already at the top
                                                if reply.ts == message.ts { continue; }
                                                
                                                if let Some(r_text) = reply.text.as_ref() {
                                                    let r_author = resolve_slack_name(reply.user.as_ref(), &user_cache);
                                                    let r_clean = clean_slack_text(r_text, &user_cache);
                                                    full_body.push_str(&format!("\\n\\n**{}**: {}", r_author, r_clean));
                                                }
                                            }
                                        }

                                        reply_cursor = r_payload.response_metadata
                                            .and_then(|m| m.next_cursor)
                                            .filter(|c| !c.is_empty());
                                        if reply_cursor.is_none() { break; }
                                    }
                                }
                            }

                            let doc = Document {
                                id: None,
                                uri: permalink.clone(),
                                source: "slack".to_string(),
                                title: Some(format!("#{} {}", channel_name, author_name)),
                                body: full_body,
                                updated_at,
                            };
                            self.index_document(doc).await?;
                            docs_indexed += 1;

                            if docs_indexed.is_multiple_of(20) {
                                emit_progress("slack", "syncing", &format!("Scanning #{}: {} docs", channel_name, docs_indexed), Some(docs_indexed));
                            }
                        }
                    }
                }

                history_cursor = payload.response_metadata
                    .and_then(|m| m.next_cursor)
                    .filter(|c: &String| !c.is_empty());
                
                    // Legacy Fix: Removing the arbitrary 1000 message limit for Full Sync
                    if history_cursor.is_none() || (!is_full_sync && docs_indexed > message_limit) {
                        break;
                    }
                }
            }
        }
        
        // Process regular channels second
        if !regular_channels.is_empty() {
            emit_progress("slack", "syncing", "Reading your channels...", Some(docs_indexed));
            for channel in &regular_channels {
                channels_scanned += 1;
                let channel_name = channel.name.as_ref()
                    .or(channel.name_normalized.as_ref())
                    .map(|s| s.as_str())
                    .unwrap_or_else(|| if channel.is_im == Some(true) { "DM" } else { "Unnamed" });
                info!("  -> Scanning channel: #{} ({})", channel_name, channel.id);
                emit_progress("slack", "syncing", &format!("Scanning #{}", channel_name), Some(docs_indexed));
                
                let mut history_cursor: Option<String> = None;
                loop {
                    let mut params = vec![
                        ("channel", channel.id.clone()),
                        ("oldest", oldest.clone()),
                        ("limit", "1000".to_string()),
                    ];
                    if let Some(c) = history_cursor.as_ref() {
                        params.push(("cursor", c.clone()));
                    }

                    let response = call_with_backoff("slack", || {
                        client.get("https://slack.com/api/conversations.history")
                            .header("Authorization", format!("Bearer {}", token.access_token))
                            .query(&params)
                    }).await?;

                    let status = response.status();
                    let payload: SlackHistoryResponse = response.json().await
                        .map_err(|e| anyhow::anyhow!("Failed to decode conversations.history response for channel {} (status {}): {}", channel.id, status, e))?;
                    if !payload.ok {
                        warn!("Slack history failed for channel {}: {:?}", channel.id, payload.error);
                        break;
                    }
                    
                    if let Some(messages) = payload.messages {
                        let channel_name = channel.name.as_ref()
                            .or(channel.name_normalized.as_ref())
                            .map(|s| s.as_str())
                            .unwrap_or_else(|| if channel.is_im == Some(true) { "DM" } else { "Unnamed" });
                        info!("  -> Found {} messages in page for channel {}", messages.len(), channel_name);
                        if messages.is_empty() { break; }
                        for message in messages {
                            // Skip replies in the main history loop (Legacy Logic: they are handled via parent replies)
                            if let Some(ref t_ts) = message.thread_ts {
                                if t_ts != &message.ts {
                                    continue;
                                }
                            }

                            if let Some(text) = message.text.as_ref() {
                                let ts_val = message.ts.parse::<f64>().unwrap_or(0.0);
                                if ts_val > max_ts {
                                    max_ts = ts_val;
                                }
                                let updated_at = slack_ts_to_datetime(&message.ts).unwrap_or_else(Utc::now);
                                let permalink = slack_permalink(&channel.id, &message.ts);
                                
                                let author_name = resolve_slack_name(message.user.as_ref(), &user_cache);
                                let clean_body_text = clean_slack_text(text, &user_cache);
                                
                                let channel_name = channel.name.as_ref()
                                    .or(channel.name_normalized.as_ref())
                                    .map(|s| s.as_str())
                                    .unwrap_or_else(|| if channel.is_im == Some(true) { "DM" } else { "Unnamed" });
                                let mut full_body = format!(
                                    "# Slack Thread: #{}\\n- Author: {}\\n- Created: {}\\n- URL: {}\\n\\n**{}**: {}",
                                    channel_name,
                                    author_name,
                                    updated_at.to_rfc3339(),
                                    permalink,
                                    author_name,
                                    clean_body_text
                                );

                                // Thread support: If this is a thread parent, fetch and CONSOLIDATE all replies (Legacy Standard)
                                if let Some(reply_count) = message.reply_count {
                                    if reply_count > 0 {
                                        info!("    -> Consolidating {} replies for thread {}", reply_count, message.ts);
                                        let mut reply_cursor: Option<String> = None;
                                        loop {
                                            let mut r_params = vec![
                                                ("channel", channel.id.clone()),
                                                ("ts", message.ts.clone()),
                                                ("limit", "100".to_string()),
                                            ];
                                            if let Some(rc) = reply_cursor.as_ref() {
                                                r_params.push(("cursor", rc.clone()));
                                            }

                                            let r_response = call_with_backoff("slack", || {
                                                client.get("https://slack.com/api/conversations.replies")
                                                    .header("Authorization", format!("Bearer {}", token.access_token))
                                                    .query(&r_params)
                                            }).await?;

                                            let status = r_response.status();
                                            let r_payload: SlackHistoryResponse = r_response.json().await
                                                .map_err(|e| anyhow::anyhow!("Failed to decode conversations.replies response for thread {} (status {}): {}", message.ts, status, e))?;
                                            if !r_payload.ok { break; }

                                            if let Some(replies) = r_payload.messages {
                                                for reply in replies {
                                                    // Skip the parent as it's already at the top
                                                    if reply.ts == message.ts { continue; }
                                                    
                                                    if let Some(r_text) = reply.text.as_ref() {
                                                        let r_author = resolve_slack_name(reply.user.as_ref(), &user_cache);
                                                        let r_clean = clean_slack_text(r_text, &user_cache);
                                                        full_body.push_str(&format!("\\n\\n**{}**: {}", r_author, r_clean));
                                                    }
                                                }
                                            }

                                            reply_cursor = r_payload.response_metadata
                                                .and_then(|m| m.next_cursor)
                                                .filter(|c| !c.is_empty());
                                            if reply_cursor.is_none() { break; }
                                        }
                                    }
                                }

                                let doc = Document {
                                    id: None,
                                    uri: permalink.clone(),
                                    source: "slack".to_string(),
                                    title: Some(format!("#{} {}", channel_name, author_name)),
                                    body: full_body,
                                    updated_at,
                                };
                                self.index_document(doc).await?;
                                docs_indexed += 1;

                                if docs_indexed.is_multiple_of(20) {
                                    emit_progress("slack", "syncing", &format!("Scanning #{}: {} docs", channel_name, docs_indexed), Some(docs_indexed));
                                }
                            }
                        }
                    }

                    history_cursor = payload.response_metadata
                        .and_then(|m| m.next_cursor)
                        .filter(|c: &String| !c.is_empty());
                    
                    // Legacy Fix: Removing the arbitrary 1000 message limit for Full Sync
                    if history_cursor.is_none() || (!is_full_sync && docs_indexed > message_limit) {
                        break;
                    }
                }
            }
        }

        let cursor = format!("{:.6}", max_ts);
        let _ = self.ingest.set_sync_cursor("slack", &cursor).await;

        info!("Slack sync complete: {} channels scanned, {} docs indexed", channels_scanned, docs_indexed);

        Ok(SyncSummary {
            provider: "slack".to_string(),
            items_scanned: channels_scanned,
            documents_processed: docs_indexed,
            updated_at: cursor,
        })
    }

    pub async fn discover_slack(&self) -> Result<serde_json::Value> {
        info!("Discovering Slack channels...");
        emit_progress("slack", "syncing", "Discovering Slack channels...", None);
        
        let token_store = TokenStore::load(self.auth.path())?;
        let token = token_store
            .get(minna_auth_bridge::Provider::Slack)
            .ok_or_else(|| anyhow::anyhow!("missing slack token"))?;

        let client = reqwest::Client::builder()
            .user_agent("minna-core")
            .build()?;

        emit_progress("slack", "syncing", "Verifying Slack authentication...", None);
        let auth_response = client.post("https://slack.com/api/auth.test")
            .header("Authorization", format!("Bearer {}", token.access_token))
            .send().await?;
        let status = auth_response.status();
        let auth_test: SlackAuthTestResponse = auth_response.json().await
            .map_err(|e| {
                let err_msg = format!("Failed to decode auth.test response in discover (status {}): {}", status, e);
                emit_error("slack", &err_msg);
                anyhow::anyhow!(err_msg)
            })?;
        let _my_user_id = auth_test.user_id.unwrap_or_default();

        let mut channels = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let mut params: Vec<(&str, String)> = vec![
                ("limit", "200".to_string()),
                ("types", "public_channel,private_channel,mpim,im".to_string()),
            ];
            if let Some(next) = cursor.as_ref() {
                params.push(("cursor", next.clone()));
            }
            let response = client.get("https://slack.com/api/users.conversations")
                .header("Authorization", format!("Bearer {}", token.access_token))
                .query(&params)
                .send().await?;
            
            let status = response.status();
            emit_progress("slack", "syncing", &format!("Fetching channels (page {})...", channels.len() / 200 + 1), None);
            
            // Capture response body before attempting to decode
            let response_bytes = response.bytes().await?;
            let response_text = String::from_utf8_lossy(&response_bytes);
            
            // Try to decode as JSON
            let payload: SlackChannelsResponse = serde_json::from_slice(&response_bytes)
                .map_err(|e| {
                    // Log the actual response for debugging
                    let preview = if response_text.len() > 500 {
                        format!("{}...", &response_text[..500])
                    } else {
                        response_text.to_string()
                    };
                    let err_msg = format!(
                        "Failed to decode users.conversations response in discover (status {}): {}. Response preview: {}",
                        status, e, preview
                    );
                    emit_error("slack", &err_msg);
                    anyhow::anyhow!(err_msg)
                })?;
            if !payload.ok { break; }
            if let Some(mut batch) = payload.channels {
                channels.append(&mut batch);
            }
            cursor = payload.response_metadata.and_then(|meta| meta.next_cursor).filter(|c| !c.is_empty());
            if cursor.is_none() { break; }
        }

        let mut public_count = 0;
        let mut private_count = 0;
        let mut im_count = 0;
        let mut mpim_count = 0;

        let mut channel_list = Vec::new();
        for c in &channels {
            let (c_type, is_public) = if c.is_im == Some(true) {
                im_count += 1;
                ("dm", false)
            } else if c.is_mpim == Some(true) {
                mpim_count += 1;
                ("group_dm", false)
            } else if c.is_channel == Some(true) && c.is_private == Some(false) {
                public_count += 1;
                ("public", true)
            } else if c.is_group == Some(true) || (c.is_channel == Some(true) && c.is_private == Some(true)) {
                private_count += 1;
                ("private", false)
            } else {
                // Fallback: treat as private channel if we can't determine
                private_count += 1;
                ("private", false)
            };

            let channel_name = c.name.as_ref()
                .or(c.name_normalized.as_ref())
                .cloned()
                .unwrap_or_else(|| if c.is_im == Some(true) { "DM".to_string() } else { "Unnamed".to_string() });
            channel_list.push(serde_json::json!({
                "id": c.id,
                "name": channel_name,
                "type": c_type,
                "is_public": is_public
            }));
        }

        emit_progress("slack", "syncing", &format!("Found {} channels total", channels.len()), None);
        
        let result = serde_json::json!({
            "provider": "slack",
            "channels": channel_list,
            "total_channels": channels.len(),
            "public_channels": public_count,
            "private_channels": private_count,
            "dms": im_count,
            "group_dms": mpim_count,
            "estimated_full_sync_minutes": (channels.len() as f64 * 0.5) as i32, // Rough estimate
            "estimated_quick_sync_minutes": (channels.len() as f64 * 0.1) as i32,
            "oldest_message_date": "Fetching...", // Async discovery of history is slow, so we stub
            "newest_message_date": Utc::now().format("%Y-%m-%d").to_string()
        });
        
        emit_progress("slack", "syncing", "Discovery complete", None);
        Ok(result)
    }

    pub async fn sync_linear(
        &self,
        since_days: Option<i64>,
        mode: Option<&str>,
    ) -> Result<SyncSummary> {
        let is_full_sync = mode == Some("full");
        info!("Starting Linear sync (since_days: {:?}, mode: {:?})", since_days, mode);

        let token_store = TokenStore::load(self.auth.path())?;
        let token = token_store
            .get(minna_auth_bridge::Provider::Linear)
            .ok_or_else(|| anyhow::anyhow!("missing linear token"))?;

        let since = if is_full_sync {
            let days = since_days.unwrap_or(90); // Default to 90 days
            info!("Linear: performing full sync (last {} days)", days);
            (Utc::now() - chrono::Duration::days(days)).to_rfc3339()
        } else if let Some(days) = since_days {
            info!("Linear: performing quick sync (last {} days)", days);
            (Utc::now() - chrono::Duration::days(days)).to_rfc3339()
        } else {
            let cursor = self.ingest.get_sync_cursor("linear").await?.unwrap_or_default();
            if cursor.is_empty() {
                info!("Linear: no cursor found, defaulting to 30 days");
                (Utc::now() - chrono::Duration::days(30)).to_rfc3339()
            } else {
                info!("Linear: performing delta sync from cursor: {}", cursor);
                cursor
            }
        };

        info!("Linear sync window starting from: {}", since);

        let limit = if is_full_sync {
            std::env::var("MINNA_LINEAR_ISSUE_LIMIT_FULL")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(500usize)
        } else {
            std::env::var("MINNA_LINEAR_ISSUE_LIMIT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(50usize)
        };
        emit_progress("linear", "syncing", "Searching for issues...", Some(0));

        let client = reqwest::Client::builder()
            .user_agent("minna-core")
            .redirect(Policy::none())
            .build()?;

        let mut after: Option<String> = None;
        let mut docs_indexed = 0usize;
        let mut max_updated = since.clone();
        loop {
            let query = r#"
                query Issues($since: DateTime!, $after: String, $first: Int!) {
                    issues(filter: { updatedAt: { gte: $since } }, first: $first, after: $after) {
                        nodes { identifier title description updatedAt url state { name } assignee { name } }
                        pageInfo { hasNextPage endCursor }
                    }
                }
            "#;
            let payload = serde_json::json!({
                "query": query,
                "variables": {
                    "since": since,
                    "after": after,
                    "first": limit as i64
                }
            });
            let response = call_with_backoff("linear", || {
                client.post("https://api.linear.app/graphql")
                    .header("Authorization", token.access_token.clone())
                    .json(&payload)
            }).await?;
            let body: LinearResponse = response.json().await?;
            if let Some(errors) = body.errors {
                return Err(anyhow::anyhow!("linear error: {}", errors[0].message));
            }
            let data = body.data.ok_or_else(|| anyhow::anyhow!("linear missing data"))?;
            for issue in data.issues.nodes {
                let updated_at = DateTime::parse_from_rfc3339(&issue.updated_at)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());
                if issue.updated_at > max_updated {
                    max_updated = issue.updated_at.clone();
                }
                let doc = Document {
                    id: None,
                    uri: issue.url.clone(),
                    source: "linear".to_string(),
                    title: Some(format!("{} {}", issue.identifier, issue.title)),
                    body: format!(
                        "# {}\\n\\n- State: {}\\n- Assignee: {}\\n- Updated: {}\\n- URL: {}\\n\\n{}",
                        issue.title,
                        issue.state.map(|s| s.name).unwrap_or_else(|| "Unknown".to_string()),
                        issue.assignee.map(|a| a.name).unwrap_or_else(|| "Unassigned".to_string()),
                        issue.updated_at,
                        issue.url,
                        issue.description.unwrap_or_default()
                    ),
                    updated_at,
                };
                self.index_document(doc).await?;
                docs_indexed += 1;
                if docs_indexed.is_multiple_of(10) {
                    emit_progress("linear", "syncing", &format!("Indexing: {} issues", docs_indexed), Some(docs_indexed));
                }
            }
            if data.issues.page_info.has_next_page {
                after = data.issues.page_info.end_cursor;
            } else {
                break;
            }
        }

        let _ = self.ingest.set_sync_cursor("linear", &max_updated).await;

        info!("Linear sync complete: {} docs indexed", docs_indexed);

        Ok(SyncSummary {
            provider: "linear".to_string(),
            items_scanned: 1,
            documents_processed: docs_indexed,
            updated_at: max_updated,
        })
    }

    /// Unified Google Workspace sync - syncs Drive, Calendar, and Gmail
    pub async fn sync_google_workspace(
        &self,
        since_days: Option<i64>,
        mode: Option<&str>,
    ) -> Result<SyncSummary> {
        let workspace_start = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
        info!("[GOOGLE_WORKSPACE] sync_google_workspace called: since_days={:?}, mode={:?}", since_days, mode);
        info!("Starting Google Workspace sync (Drive + Calendar + Gmail)");
        
        // Sync Drive
        let drive_start = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
        info!("[GOOGLE_WORKSPACE] Starting Drive sync, delay_from_start_ms={}", drive_start - workspace_start);
        emit_progress("google", "syncing", "Scanning your Drive...", None);
        let drive_summary = self.sync_google_drive(since_days, mode).await?;
        let drive_end = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
        info!("[GOOGLE_WORKSPACE] Drive sync completed: duration_ms={}, docs={}", drive_end - drive_start, drive_summary.documents_processed);
        
        // Sync Calendar
        let calendar_start = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
        info!("[GOOGLE_WORKSPACE] Starting Calendar sync, delay_from_start_ms={}", calendar_start - workspace_start);
        emit_progress("google", "syncing", "Looking at your calendar...", Some(drive_summary.documents_processed));
        let calendar_summary = self.sync_google_calendar(since_days, mode).await?;
        let calendar_end = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
        info!("[GOOGLE_WORKSPACE] Calendar sync completed: duration_ms={}, docs={}", calendar_end - calendar_start, calendar_summary.documents_processed);
        
        // Sync Gmail
        let gmail_start = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
        info!("[GOOGLE_WORKSPACE] Starting Gmail sync, delay_from_start_ms={}", gmail_start - workspace_start);
        emit_progress("google", "syncing", "Getting your email...", Some(drive_summary.documents_processed + calendar_summary.documents_processed));
        let gmail_summary = self.sync_gmail(since_days, mode).await?;
        let gmail_end = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
        info!("[GOOGLE_WORKSPACE] Gmail sync completed: duration_ms={}, docs={}", gmail_end - gmail_start, gmail_summary.documents_processed);
        
        let total_docs = drive_summary.documents_processed + calendar_summary.documents_processed + gmail_summary.documents_processed;
        
        Ok(SyncSummary {
            provider: "google_workspace".to_string(),
            items_scanned: drive_summary.items_scanned + calendar_summary.items_scanned + gmail_summary.items_scanned,
            documents_processed: total_docs,
            updated_at: gmail_summary.updated_at, // Use most recent
        })
    }

    pub async fn sync_google_drive(
        &self,
        since_days: Option<i64>,
        mode: Option<&str>,
    ) -> Result<SyncSummary> {
        let is_full_sync = mode == Some("full");
        info!("Starting Google Drive sync (since_days: {:?}, mode: {:?})", since_days, mode);

        let token_store = TokenStore::load(self.auth.path())?;
        let token = token_store
            .get(minna_auth_bridge::Provider::Google)
            .ok_or_else(|| anyhow::anyhow!("missing google token"))?;

        let since = if is_full_sync {
            let days = since_days.unwrap_or(90); // Default to 90 days
            info!("Google Drive: performing full sync (last {} days)", days);
            (Utc::now() - chrono::Duration::days(days)).format("%Y-%m-%dT%H:%M:%SZ").to_string()
        } else if let Some(days) = since_days {
            info!("Google Drive: performing quick sync (last {} days)", days);
            (Utc::now() - chrono::Duration::days(days)).format("%Y-%m-%dT%H:%M:%SZ").to_string()
        } else {
            let cursor = self.ingest.get_sync_cursor("google_drive").await?.unwrap_or_default();
            if cursor.is_empty() {
                info!("Google Drive: no cursor found, defaulting to 90 days");
                (Utc::now() - chrono::Duration::days(90)).format("%Y-%m-%dT%H:%M:%SZ").to_string()
            } else {
                info!("Google Drive: performing delta sync from cursor: {}", cursor);
                cursor
            }
        };

        info!("Google Drive sync window starting from: {}", since);

        let file_limit = if is_full_sync {
            std::env::var("MINNA_DRIVE_FILE_LIMIT_FULL")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1000usize)
        } else {
            std::env::var("MINNA_DRIVE_FILE_LIMIT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(50usize)
        };
        // Note: Top-level progress message is emitted by sync_google_workspace
        // This ensures progress is shown even if sync_google_drive is called directly
        emit_progress("google", "syncing", "Scanning your Drive...", Some(0));

        let max_bytes = std::env::var("MINNA_DOC_MAX_BYTES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(200_000usize);

        let client = reqwest::Client::builder()
            .user_agent("minna-core")
            .redirect(Policy::none())
            .build()?;

        // Get user's email for filtering (needed for some queries)
        // First, get user info to confirm token is valid
        let user_info_response = call_with_backoff("google_drive", || {
            client.get("https://www.googleapis.com/oauth2/v2/userinfo")
                .bearer_auth(&token.access_token)
        }).await?;
        let user_info: serde_json::Value = user_info_response.json().await?;
        let user_email = user_info.get("email").and_then(|e| e.as_str()).unwrap_or("me");
        info!("Google Workspace sync for user: {}", user_email);

        let mut page_token: Option<String> = None;
        let mut docs_indexed = 0usize;
        let mut max_updated = since.clone();
        loop {
            // Query: Files created by user OR shared with user
            // This covers: user's files, files shared with user, and files where user is collaborator
            let q = format!(
                "modifiedTime > '{}' and trashed = false and ('me' in owners or sharedWithMe=true)",
                since
            );
            info!("Google Drive API Query: {}", q);
            let mut params = vec![
                ("pageSize", "100".to_string()),
                (
                    "fields",
                    "nextPageToken,files(id,name,mimeType,modifiedTime,webViewLink,owners,shared)".to_string(),
                ),
                ("q", q),
            ];
            if let Some(token) = page_token.as_ref() {
                params.push(("pageToken", token.clone()));
            }
            let response = call_with_backoff("google_drive", || {
                client.get("https://www.googleapis.com/drive/v3/files")
                    .bearer_auth(&token.access_token)
                    .query(&params)
            }).await?;
            let payload: DriveListResponse = response.json().await?;
            if let Some(files) = payload.files {
                for file in files {
                    if docs_indexed >= file_limit {
                        break;
                    }
                    let updated_at = DateTime::parse_from_rfc3339(&file.modified_time)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now());
                    if file.modified_time > max_updated {
                        max_updated = file.modified_time.clone();
                    }
                    info!("  -> Syncing file: {} ({})", file.name, file.id);
                    emit_progress("google_drive", "syncing", &format!("Fetching {}", file.name), Some(docs_indexed));

                    // Try to fetch file content, but continue even if it fails (e.g., 403 permission errors)
                    let content = match fetch_drive_file(&client, &token.access_token, &file).await {
                        Ok(c) => c,
                        Err(e) => {
                            // Log the error but continue - some files may not be downloadable
                            let error_msg = e.to_string();
                            if error_msg.contains("403") || error_msg.contains("Forbidden") {
                                warn!("  -> Skipping file {} ({}): permission denied", file.name, file.id);
                            } else {
                                warn!("  -> Skipping file {} ({}): {}", file.name, file.id, error_msg);
                            }
                            String::new() // Use empty content, will create metadata-only document
                        }
                    };
                    
                    let body = if content.is_empty() {
                        format!(
                            "# {}\\n\\n- Type: {}\\n- Updated: {}\\n- URL: {}",
                            file.name,
                            file.mime_type,
                            file.modified_time,
                            file.web_view_link.clone().unwrap_or_default()
                        )
                    } else {
                        let clipped = truncate_bytes(&content, max_bytes);
                        format!(
                            "# {}\\n\\n- Type: {}\\n- Updated: {}\\n- URL: {}\\n\\n{}",
                            file.name,
                            file.mime_type,
                            file.modified_time,
                            file.web_view_link.clone().unwrap_or_default(),
                            clipped
                        )
                    };

                    let doc = Document {
                        id: None,
                        uri: file
                            .web_view_link
                            .clone()
                            .unwrap_or_else(|| format!("drive://{}", file.id)),
                        source: "google_drive".to_string(),
                        title: Some(file.name.clone()),
                        body,
                        updated_at,
                    };
                    self.index_document(doc).await?;
                    docs_indexed += 1;
                }
            }
            page_token = payload.next_page_token;
            if page_token.is_none() || docs_indexed >= file_limit {
                break;
            }
        }

        let _ = self
            .ingest
            .set_sync_cursor("google_drive", &max_updated)
            .await;

        info!("Google Drive sync complete: {} docs indexed", docs_indexed);

        Ok(SyncSummary {
            provider: "google_drive".to_string(),
            items_scanned: 1,
            documents_processed: docs_indexed,
            updated_at: max_updated,
        })
    }

    pub async fn sync_google_calendar(
        &self,
        since_days: Option<i64>,
        mode: Option<&str>,
    ) -> Result<SyncSummary> {
        let is_full_sync = mode == Some("full");
        info!("Starting Google Calendar sync (since_days: {:?}, mode: {:?})", since_days, mode);

        let token_store = TokenStore::load(self.auth.path())?;
        let token = token_store
            .get(minna_auth_bridge::Provider::Google)
            .ok_or_else(|| anyhow::anyhow!("missing google token"))?;

        // Get user's email for filtering
        let client = reqwest::Client::builder()
            .user_agent("minna-core")
            .redirect(Policy::none())
            .build()?;
        
        let user_info_response = call_with_backoff("google_calendar", || {
            client.get("https://www.googleapis.com/oauth2/v2/userinfo")
                .bearer_auth(&token.access_token)
        }).await?;
        let user_info: serde_json::Value = user_info_response.json().await?;
        let user_email = user_info.get("email").and_then(|e| e.as_str()).unwrap_or("");
        info!("Google Calendar sync for user: {}", user_email);

        let since = if is_full_sync {
            let days = since_days.unwrap_or(90);
            (Utc::now() - chrono::Duration::days(days)).format("%Y-%m-%dT%H:%M:%SZ").to_string()
        } else if let Some(days) = since_days {
            (Utc::now() - chrono::Duration::days(days)).format("%Y-%m-%dT%H:%M:%SZ").to_string()
        } else {
            let cursor = self.ingest.get_sync_cursor("google_calendar").await?.unwrap_or_default();
            if cursor.is_empty() {
                (Utc::now() - chrono::Duration::days(90)).format("%Y-%m-%dT%H:%M:%SZ").to_string()
            } else {
                cursor
            }
        };

        emit_progress("google", "syncing", "Looking at your calendar...", Some(0));

        // Get events from primary calendar where user is organizer OR attendee
        let time_min = since;
        let time_max = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        
        let mut page_token: Option<String> = None;
        let mut events_indexed = 0usize;
        let mut max_updated = time_max.clone();

        loop {
            let mut params = vec![
                ("timeMin", time_min.clone()),
                ("timeMax", time_max.clone()),
                ("singleEvents", "true".to_string()),
                ("orderBy", "startTime".to_string()),
                ("maxResults", "2500".to_string()), // Calendar API max
            ];
            if let Some(token) = page_token.as_ref() {
                params.push(("pageToken", token.clone()));
            }

            let response = call_with_backoff("google_calendar", || {
                client.get("https://www.googleapis.com/calendar/v3/calendars/primary/events")
                    .bearer_auth(&token.access_token)
                    .query(&params)
            }).await?;

            let payload: serde_json::Value = response.json().await?;
            let empty_vec: Vec<serde_json::Value> = vec![];
            let events = payload.get("items").and_then(|i| i.as_array()).unwrap_or(&empty_vec);

            for event in events {
                // Filter: user is organizer OR user is in attendees list
                let organizer_email = event.get("organizer")
                    .and_then(|o| o.get("email"))
                    .and_then(|e| e.as_str())
                    .unwrap_or("");
                
                let empty_attendees: Vec<serde_json::Value> = vec![];
                let attendees = event.get("attendees")
                    .and_then(|a| a.as_array())
                    .unwrap_or(&empty_attendees);
                
                let is_attendee = attendees.iter().any(|a| {
                    a.get("email").and_then(|e| e.as_str()) == Some(user_email)
                });

                // Only index if user is organizer or attendee
                if organizer_email == user_email || is_attendee {
                    let summary = event.get("summary").and_then(|s| s.as_str()).unwrap_or("(No title)");
                    let description = event.get("description").and_then(|d| d.as_str()).unwrap_or("");
                    let html_link = event.get("htmlLink").and_then(|h| h.as_str()).unwrap_or("");
                    let updated = event.get("updated").and_then(|u| u.as_str()).unwrap_or("");
                    
                    let body = format!(
                        "# {}\n\n- Description: {}\n- Link: {}\n- Updated: {}",
                        summary, description, html_link, updated
                    );

                    let doc = Document {
                        id: None,
                        uri: html_link.to_string(),
                        source: "google_calendar".to_string(),
                        title: Some(summary.to_string()),
                        body,
                        updated_at: DateTime::parse_from_rfc3339(updated)
                            .map(|dt| dt.with_timezone(&Utc))
                            .unwrap_or_else(|_| Utc::now()),
                    };
                    self.index_document(doc).await?;
                    events_indexed += 1;
                    
                    if updated > max_updated.as_str() {
                        max_updated = updated.to_string();
                    }
                }
            }

            page_token = payload.get("nextPageToken").and_then(|t| t.as_str()).map(|s| s.to_string());
            if page_token.is_none() {
                break;
            }
        }

        let _ = self.ingest.set_sync_cursor("google_calendar", &max_updated).await;

        info!("Google Calendar sync complete: {} events indexed", events_indexed);

        Ok(SyncSummary {
            provider: "google_calendar".to_string(),
            items_scanned: 1,
            documents_processed: events_indexed,
            updated_at: max_updated,
        })
    }

    pub async fn sync_gmail(
        &self,
        since_days: Option<i64>,
        mode: Option<&str>,
    ) -> Result<SyncSummary> {
        let is_full_sync = mode == Some("full");
        info!("Starting Gmail sync (since_days: {:?}, mode: {:?})", since_days, mode);

        let token_store = TokenStore::load(self.auth.path())?;
        let token = token_store
            .get(minna_auth_bridge::Provider::Google)
            .ok_or_else(|| anyhow::anyhow!("missing google token"))?;

        // Get user's email for filtering
        let client = reqwest::Client::builder()
            .user_agent("minna-core")
            .redirect(Policy::none())
            .build()?;
        
        let user_info_response = call_with_backoff("gmail", || {
            client.get("https://www.googleapis.com/oauth2/v2/userinfo")
                .bearer_auth(&token.access_token)
        }).await?;
        let user_info: serde_json::Value = user_info_response.json().await?;
        let user_email = user_info.get("email").and_then(|e| e.as_str()).unwrap_or("");
        info!("Gmail sync for user: {}", user_email);

        let since_timestamp = if is_full_sync {
            let days = since_days.unwrap_or(90);
            (Utc::now() - chrono::Duration::days(days)).timestamp()
        } else if let Some(days) = since_days {
            (Utc::now() - chrono::Duration::days(days)).timestamp()
        } else {
            let cursor = self.ingest.get_sync_cursor("gmail").await?.unwrap_or_default();
            if cursor.is_empty() {
                (Utc::now() - chrono::Duration::days(90)).timestamp()
            } else {
                cursor.parse().unwrap_or((Utc::now() - chrono::Duration::days(90)).timestamp())
            }
        };

        emit_progress("google", "syncing", "Getting your email...", Some(0));

        let mut page_token: Option<String> = None;
        let mut emails_indexed = 0usize;
        let mut max_updated = Utc::now().timestamp().to_string();

        // Build query: Priority emails OR emails sent by user OR emails with user in to/cc/bcc
        // Gmail query syntax: is:important OR from:me OR to:me OR cc:me OR bcc:me
        let query = format!(
            "after:{} (is:important OR from:{} OR to:{} OR cc:{} OR bcc:{})",
            since_timestamp, user_email, user_email, user_email, user_email
        );

        loop {
            let mut params = vec![
                ("q", query.clone()),
                ("maxResults", "500".to_string()), // Gmail API max per page
            ];
            if let Some(token) = page_token.as_ref() {
                params.push(("pageToken", token.clone()));
            }

            let response = call_with_backoff("gmail", || {
                client.get("https://www.googleapis.com/gmail/v1/users/me/messages")
                    .bearer_auth(&token.access_token)
                    .query(&params)
            }).await?;

            let payload: serde_json::Value = response.json().await?;
            let empty_msg_vec: Vec<serde_json::Value> = vec![];
            let messages = payload.get("messages").and_then(|m| m.as_array()).unwrap_or(&empty_msg_vec);

            for message_ref in messages {
                let message_id = message_ref.get("id").and_then(|i| i.as_str()).unwrap_or("");
                
                // Fetch full message details
                let msg_response = call_with_backoff("gmail", || {
                    client.get(format!("https://www.googleapis.com/gmail/v1/users/me/messages/{}", message_id))
                        .bearer_auth(&token.access_token)
                        .query(&[("format", "full")])
                }).await?;

                let msg_data: serde_json::Value = msg_response.json().await?;
                let empty_payload = serde_json::json!({});
                let payload_data = msg_data.get("payload").unwrap_or(&empty_payload);
                let empty_headers: Vec<serde_json::Value> = vec![];
                let headers = payload_data.get("headers").and_then(|h| h.as_array()).unwrap_or(&empty_headers);
                
                let subject = headers.iter()
                    .find(|h| h.get("name").and_then(|n| n.as_str()) == Some("Subject"))
                    .and_then(|h| h.get("value").and_then(|v| v.as_str()))
                    .unwrap_or("(No subject)");
                
                let from = headers.iter()
                    .find(|h| h.get("name").and_then(|n| n.as_str()) == Some("From"))
                    .and_then(|h| h.get("value").and_then(|v| v.as_str()))
                    .unwrap_or("");
                
                let snippet = msg_data.get("snippet").and_then(|s| s.as_str()).unwrap_or("");
                let thread_id = msg_data.get("threadId").and_then(|t| t.as_str()).unwrap_or("");
                let internal_date = msg_data.get("internalDate").and_then(|d| d.as_str()).unwrap_or("");
                
                let body = format!(
                    "# {}\n\n- From: {}\n- Snippet: {}\n- Thread ID: {}\n- Date: {}",
                    subject, from, snippet, thread_id, internal_date
                );

                let doc = Document {
                    id: None,
                    uri: format!("https://mail.google.com/mail/u/0/#inbox/{}", thread_id),
                    source: "gmail".to_string(),
                    title: Some(subject.to_string()),
                    body,
                    updated_at: internal_date.parse::<i64>()
                        .ok()
                        .and_then(|ts| DateTime::from_timestamp(ts / 1000, 0))
                        .unwrap_or_else(Utc::now),
                };
                self.index_document(doc).await?;
                emails_indexed += 1;
                
                if let Ok(ts) = internal_date.parse::<i64>() {
                    if ts > max_updated.parse::<i64>().unwrap_or(0) {
                        max_updated = (ts / 1000).to_string();
                    }
                }
            }

            page_token = payload.get("nextPageToken").and_then(|t| t.as_str()).map(|s| s.to_string());
            if page_token.is_none() {
                break;
            }
        }

        let _ = self.ingest.set_sync_cursor("gmail", &max_updated).await;

        info!("Gmail sync complete: {} emails indexed", emails_indexed);

        Ok(SyncSummary {
            provider: "gmail".to_string(),
            items_scanned: 1,
            documents_processed: emails_indexed,
            updated_at: max_updated,
        })
    }

    pub async fn discover_google_drive(&self) -> Result<serde_json::Value> {
        // #region agent log
        let log_path = "/Users/wp/Antigravity/.cursor/debug.log";
        let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
        let _ = std::fs::OpenOptions::new().create(true).append(true).open(log_path).and_then(|mut f| {
            use std::io::Write;
            writeln!(f, r#"{{"timestamp":{},"location":"minna-core/src/lib.rs:discover_google_drive:entry","message":"discover_google_drive called","data":{{"sessionId":"debug-session","runId":"run1","hypothesisId":"A"}}}}"#, timestamp)
        });
        // #endregion agent log
        
        info!("Discovering Google Drive files...");
        emit_progress("google_drive", "syncing", "Discovering Google Drive files...", None);
        
        // #region agent log
        let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
        let _ = std::fs::OpenOptions::new().create(true).append(true).open(log_path).and_then(|mut f| {
            use std::io::Write;
            writeln!(f, r#"{{"timestamp":{},"location":"minna-core/src/lib.rs:discover_google_drive:after_emit","message":"After emit_progress","data":{{"sessionId":"debug-session","runId":"run1","hypothesisId":"A"}}}}"#, timestamp)
        });
        // #endregion agent log
        
        let token_store = TokenStore::load(self.auth.path())?;
        
        // #region agent log
        let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
        let _ = std::fs::OpenOptions::new().create(true).append(true).open(log_path).and_then(|mut f| {
            use std::io::Write;
            writeln!(f, r#"{{"timestamp":{},"location":"minna-core/src/lib.rs:discover_google_drive:token_loaded","message":"Token store loaded","data":{{"sessionId":"debug-session","runId":"run1","hypothesisId":"B"}}}}"#, timestamp)
        });
        // #endregion agent log
        
        let token = token_store
            .get(minna_auth_bridge::Provider::Google)
            .ok_or_else(|| {
                // #region agent log
                let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
                let _ = std::fs::OpenOptions::new().create(true).append(true).open(log_path).and_then(|mut f| {
                    use std::io::Write;
                    writeln!(f, r#"{{"timestamp":{},"location":"minna-core/src/lib.rs:discover_google_drive:no_token","message":"Missing Google token","data":{{"sessionId":"debug-session","runId":"run1","hypothesisId":"B"}}}}"#, timestamp)
                });
                // #endregion agent log
                anyhow::anyhow!("missing google token")
            })?;

        // #region agent log
        let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
        let _ = std::fs::OpenOptions::new().create(true).append(true).open(log_path).and_then(|mut f| {
            use std::io::Write;
            writeln!(f, r#"{{"timestamp":{},"location":"minna-core/src/lib.rs:discover_google_drive:token_found","message":"Google token found","data":{{"token_length":{},"sessionId":"debug-session","runId":"run1","hypothesisId":"B"}}}}"#, timestamp, token.access_token.len())
        });
        // #endregion agent log

        let client = reqwest::Client::builder()
            .user_agent("minna-core")
            .redirect(Policy::none())
            .build()?;

        emit_progress("google_drive", "syncing", "Querying Google Drive API...", None);
        
        // Quick discovery: just count files modified in last 90 days
        // Query: Files created by user OR shared with user (covers user's files and files where user is collaborator)
        let since = (Utc::now() - chrono::Duration::days(90)).format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let q = format!("modifiedTime > '{}' and trashed = false and ('me' in owners or sharedWithMe=true)", since);
        
        // #region agent log
        let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
        let _ = std::fs::OpenOptions::new().create(true).append(true).open(log_path).and_then(|mut f| {
            use std::io::Write;
            writeln!(f, r#"{{"timestamp":{},"location":"minna-core/src/lib.rs:discover_google_drive:before_loop","message":"Starting API query loop","data":{{"query":"{}","sessionId":"debug-session","runId":"run1","hypothesisId":"C"}}}}"#, timestamp, q)
        });
        // #endregion agent log
        
        let mut total_files = 0usize;
        let mut page_token: Option<String> = None;
        let mut pages = 0usize;
        
        loop {
            let mut params = vec![
                ("pageSize", "1000".to_string()), // Use max page size for discovery
                ("fields", "nextPageToken,files(id,name,mimeType,modifiedTime)".to_string()),
                ("q", q.clone()),
            ];
            if let Some(token) = page_token.as_ref() {
                params.push(("pageToken", token.clone()));
            }
            
            emit_progress("google_drive", "syncing", &format!("Scanning page {}...", pages + 1), None);
            
            // #region agent log
            let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
            let _ = std::fs::OpenOptions::new().create(true).append(true).open(log_path).and_then(|mut f| {
                use std::io::Write;
                writeln!(f, r#"{{"timestamp":{},"location":"minna-core/src/lib.rs:discover_google_drive:before_api_call","message":"Calling Google Drive API","data":{{"page":{},"sessionId":"debug-session","runId":"run1","hypothesisId":"C"}}}}"#, timestamp, pages + 1)
            });
            // #endregion agent log
            
            let response = call_with_backoff("google_drive", || {
                client.get("https://www.googleapis.com/drive/v3/files")
                    .bearer_auth(&token.access_token)
                    .query(&params)
            }).await.map_err(|e| {
                // #region agent log
                let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
                let _ = std::fs::OpenOptions::new().create(true).append(true).open(log_path).and_then(|mut f| {
                    use std::io::Write;
                    writeln!(f, r#"{{"timestamp":{},"location":"minna-core/src/lib.rs:discover_google_drive:api_error","message":"Google Drive API call failed","data":{{"error":"{}","sessionId":"debug-session","runId":"run1","hypothesisId":"C"}}}}"#, timestamp, e)
                });
                // #endregion agent log
                let err_msg = format!("Google Drive API call failed during discovery: {}", e);
                emit_error("google_drive", &err_msg);
                anyhow::anyhow!(err_msg)
            })?;
            
            // #region agent log
            let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
            let _ = std::fs::OpenOptions::new().create(true).append(true).open(log_path).and_then(|mut f| {
                use std::io::Write;
                writeln!(f, r#"{{"timestamp":{},"location":"minna-core/src/lib.rs:discover_google_drive:api_success","message":"Google Drive API call succeeded","data":{{"status":{},"sessionId":"debug-session","runId":"run1","hypothesisId":"C"}}}}"#, timestamp, response.status())
            });
            // #endregion agent log
            
            let status = response.status();
            let payload: DriveListResponse = response.json().await
                .map_err(|e| {
                    // #region agent log
                    let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
                    let _ = std::fs::OpenOptions::new().create(true).append(true).open(log_path).and_then(|mut f| {
                        use std::io::Write;
                        writeln!(f, r#"{{"timestamp":{},"location":"minna-core/src/lib.rs:discover_google_drive:decode_error","message":"Failed to decode Drive API response","data":{{"status":{},"error":"{}","sessionId":"debug-session","runId":"run1","hypothesisId":"D"}}}}"#, timestamp, status, e)
                    });
                    // #endregion agent log
                    let err_msg = format!("Failed to decode Drive API response in discover (status {}): {}", status, e);
                    emit_error("google_drive", &err_msg);
                    anyhow::anyhow!(err_msg)
                })?;
            
            // #region agent log
            let file_count = payload.files.as_ref().map(|f| f.len()).unwrap_or(0);
            let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
            let _ = std::fs::OpenOptions::new().create(true).append(true).open(log_path).and_then(|mut f| {
                use std::io::Write;
                writeln!(f, r#"{{"timestamp":{},"location":"minna-core/src/lib.rs:discover_google_drive:page_processed","message":"Page processed","data":{{"page":{},"files_in_page":{},"has_next":{},"sessionId":"debug-session","runId":"run1","hypothesisId":"C"}}}}"#, timestamp, pages + 1, file_count, page_token.is_some())
            });
            // #endregion agent log
            
            if let Some(files) = payload.files {
                total_files += files.len();
            }
            
            page_token = payload.next_page_token;
            pages += 1;
            
            // Limit discovery to first 10 pages to avoid timeout (1000 files per page = 10k files max)
            if page_token.is_none() || pages >= 10 {
                break;
            }
        }

        emit_progress("google_drive", "syncing", &format!("Found {} files", total_files), None);
        
        // Return in a format compatible with SlackDiscoveryResult for UI consistency
        let result = serde_json::json!({
            "provider": "google_drive",
            "total_channels": total_files, // Map to total_channels for UI compatibility
            "public_channels": 0, // Google Drive doesn't have public/private channels
            "private_channels": total_files,
            "dms": 0,
            "group_dms": 0,
            "total_files": total_files,
            "pages_scanned": pages,
            "estimated_full_sync_minutes": (total_files as f64 * 0.1) as i32, // Rough estimate: 0.1 min per file
            "estimated_quick_sync_minutes": ((total_files.min(500)) as f64 * 0.1) as i32,
            "oldest_message_date": since, // Map to oldest_message_date for UI compatibility
            "newest_message_date": Utc::now().format("%Y-%m-%d").to_string()
        });
        
        emit_progress("google_drive", "syncing", "Discovery complete", None);
        Ok(result)
    }

    pub async fn discover_github(&self) -> Result<serde_json::Value> {
        info!("Discovering GitHub repositories...");
        emit_progress("github", "syncing", "Discovering GitHub repositories...", None);
        
        let token_store = TokenStore::load(self.auth.path())?;
        let token = token_store
            .get(minna_auth_bridge::Provider::Github)
            .ok_or_else(|| {
                let err_msg = "missing github token";
                emit_error("github", err_msg);
                anyhow::anyhow!(err_msg)
            })?;

        let client = reqwest::Client::builder()
            .user_agent("minna-core")
            .redirect(Policy::none())
            .build()?;

        emit_progress("github", "syncing", "Querying GitHub API...", None);
        
        let mut repos = Vec::new();
        let mut page = 1;
        let mut public_count = 0usize;
        let mut private_count = 0usize;
        
        // Limit discovery to first 10 pages (100 repos per page = 1000 repos max)
        while repos.len() < 1000 && page <= 10 {
            let url = format!(
                "https://api.github.com/user/repos?per_page=100&page={}&sort=updated",
                page
            );
            
            let response = call_with_backoff("github", || {
                client.get(&url).header("Authorization", format!("token {}", token.access_token))
            }).await.map_err(|e| {
                let err_msg = format!("GitHub API call failed during discovery: {}", e);
                emit_error("github", &err_msg);
                anyhow::anyhow!(err_msg)
            })?;
            
            let status = response.status();
            if !status.is_success() {
                let error_text = response.text().await.unwrap_or_else(|_| "Unable to read error response".to_string());
                let err_msg = format!("GitHub API returned error status {}: {}", status, error_text);
                emit_error("github", &err_msg);
                return Err(anyhow::anyhow!(err_msg));
            }
            
            let mut batch: Vec<GithubRepo> = response.json().await.map_err(|e| {
                let err_msg = format!("Failed to decode GitHub API response in discover (status {}): {}", status, e);
                emit_error("github", &err_msg);
                anyhow::anyhow!(err_msg)
            })?;
            
            if batch.is_empty() {
                break;
            }
            
            // Count public vs private repos
            for repo in &batch {
                if repo.private.unwrap_or(false) {
                    private_count += 1;
                } else {
                    public_count += 1;
                }
            }
            
            repos.append(&mut batch);
            emit_progress("github", "syncing", &format!("Found {} repositories...", repos.len()), None);
            
            page += 1;
        }
        
        let total_repos = repos.len();
        emit_progress("github", "syncing", &format!("Found {} repositories", total_repos), None);
        
        // Return in a format compatible with SlackDiscoveryResult for UI consistency
        let result = serde_json::json!({
            "provider": "github",
            "total_channels": total_repos, // Map to total_channels for UI compatibility
            "public_channels": public_count,
            "private_channels": private_count,
            "dms": 0, // Not applicable for GitHub
            "group_dms": 0, // Not applicable for GitHub
            "estimated_full_sync_minutes": (total_repos as f64 * 2.0) as i32, // Rough estimate: 2 min per repo (issues + comments)
            "estimated_quick_sync_minutes": ((total_repos.min(25)) as f64 * 1.0) as i32, // Quick sync: 1 min per repo, max 25 repos
            "oldest_message_date": "Fetching...", // Discovery doesn't fetch issue dates
            "newest_message_date": Utc::now().format("%Y-%m-%d").to_string()
        });
        
        emit_progress("github", "syncing", "Discovery complete", None);
        Ok(result)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncSummary {
    pub provider: String,
    pub items_scanned: usize,
    pub documents_processed: usize,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
struct GithubRepo {
    name: String,
    owner: GithubOwner,
    #[serde(default)]
    private: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
struct GithubOwner {
    login: String,
}

#[derive(Debug, Clone, Deserialize)]
struct GithubIssue {
    number: i64,
    title: String,
    body: Option<String>,
    html_url: String,
    updated_at: String,
    pull_request: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
struct SlackChannelsResponse {
    ok: bool,
    channels: Option<Vec<SlackChannel>>,
    error: Option<String>,
    response_metadata: Option<SlackResponseMetadata>,
}

#[derive(Debug, Clone, Deserialize)]
struct SlackResponseMetadata {
    next_cursor: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct SlackAuthTestResponse {
    ok: bool,
    user_id: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct SlackChannel {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    name_normalized: Option<String>,
    #[serde(default)]
    is_im: Option<bool>,
    #[serde(default)]
    is_mpim: Option<bool>,
    #[serde(default)]
    is_channel: Option<bool>,
    #[serde(default)]
    is_group: Option<bool>,
    #[serde(default)]
    is_private: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
struct SlackHistoryResponse {
    ok: bool,
    messages: Option<Vec<SlackMessage>>,
    error: Option<String>,
    response_metadata: Option<SlackResponseMetadata>,
}

#[derive(Debug, Clone, Deserialize)]
struct SlackMessage {
    ts: String,
    text: Option<String>,
    user: Option<String>,
    thread_ts: Option<String>,
    reply_count: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct SlackUsersResponse {
    ok: bool,
    members: Option<Vec<SlackUser>>,
    error: Option<String>,
    response_metadata: Option<SlackResponseMetadata>,
}

#[derive(Debug, Clone, Deserialize)]
struct SlackUser {
    id: String,
    profile: SlackUserProfile,
}

#[derive(Debug, Clone, Deserialize)]
struct SlackUserProfile {
    real_name: Option<String>,
    display_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct LinearResponse {
    data: Option<LinearData>,
    errors: Option<Vec<LinearError>>,
}

#[derive(Debug, Clone, Deserialize)]
struct LinearError {
    message: String,
}

#[derive(Debug, Clone, Deserialize)]
struct LinearData {
    issues: LinearIssues,
}

#[derive(Debug, Clone, Deserialize)]
struct LinearIssues {
    nodes: Vec<LinearIssue>,
    #[serde(rename = "pageInfo")]
    page_info: LinearPageInfo,
}

#[derive(Debug, Clone, Deserialize)]
struct LinearPageInfo {
    #[serde(rename = "hasNextPage")]
    has_next_page: bool,
    #[serde(rename = "endCursor")]
    end_cursor: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct LinearIssue {
    identifier: String,
    title: String,
    description: Option<String>,
    #[serde(rename = "updatedAt")]
    updated_at: String,
    url: String,
    state: Option<LinearState>,
    assignee: Option<LinearUser>,
}

#[derive(Debug, Clone, Deserialize)]
struct LinearState {
    name: String,
}

#[derive(Debug, Clone, Deserialize)]
struct LinearUser {
    name: String,
}

#[derive(Debug, Clone, Deserialize)]
struct DriveListResponse {
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
    files: Option<Vec<DriveFile>>,
}

#[derive(Debug, Clone, Deserialize)]
struct DriveFile {
    id: String,
    name: String,
    #[serde(rename = "mimeType")]
    mime_type: String,
    #[serde(rename = "modifiedTime")]
    modified_time: String,
    #[serde(rename = "webViewLink")]
    web_view_link: Option<String>,
}

fn slack_ts_to_datetime(ts: &str) -> Option<DateTime<Utc>> {
    let seconds = ts.parse::<f64>().ok()?;
    let secs = seconds.trunc() as i64;
    let nanos = ((seconds.fract() * 1_000_000_000.0) as u32).min(999_999_999);
    DateTime::<Utc>::from_timestamp(secs, nanos)
}

fn slack_ts_from_datetime(dt: DateTime<Utc>) -> String {
    let ts = dt.timestamp_millis() as f64 / 1000.0;
    format!("{:.6}", ts)
}

fn slack_permalink(channel_id: &str, ts: &str) -> String {
    let compact = ts.replace('.', "");
    format!("https://slack.com/archives/{}/p{}", channel_id, compact)
}

async fn fetch_drive_file(
    client: &reqwest::Client,
    token: &str,
    file: &DriveFile,
) -> Result<String> {
    if file.mime_type == "application/vnd.google-apps.document" {
        let url = format!(
            "https://www.googleapis.com/drive/v3/files/{}/export",
            file.id
        );
            let response = call_with_backoff("google_drive", || {
                client.get(&url)
                .bearer_auth(token)
                .query(&[("mimeType", "text/plain")])
            }).await?;
        return Ok(response.text().await.unwrap_or_default());
    }

    if file.mime_type == "application/vnd.google-apps.spreadsheet" {
        let url = format!(
            "https://www.googleapis.com/drive/v3/files/{}/export",
            file.id
        );
        let response = call_with_backoff("google_drive", || {
            client.get(&url)
            .bearer_auth(token)
            .query(&[("mimeType", "text/csv")])
        }).await?;
        return Ok(response.text().await.unwrap_or_default());
    }

    if file.mime_type.starts_with("text/") {
        let url = format!("https://www.googleapis.com/drive/v3/files/{}", file.id);
        let response = call_with_backoff("google_drive", || {
            client.get(&url)
            .bearer_auth(token)
            .query(&[("alt", "media")])
        }).await?;
        return Ok(response.text().await.unwrap_or_default());
    }

    Ok(String::new())
}

fn truncate_bytes(input: &str, max_bytes: usize) -> String {
    if input.len() <= max_bytes {
        return input.to_string();
    }
    let mut out = String::new();
    for ch in input.chars() {
        if out.len() + ch.len_utf8() > max_bytes {
            break;
        }
        out.push(ch);
    }
    out.push_str("\n\n[truncated]");
    out
}

#[derive(Debug, Clone)]
pub struct EntitlementStatus {
    pub is_pro: bool,
    pub reason: String,
    pub checked_at: DateTime<Utc>,
}

pub fn check_entitlement(entitlement_path: &Path) -> EntitlementStatus {
    let checked_at = Utc::now();
    let allow_insecure = std::env::var("MINNA_PRO_BYPASS")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let Ok(contents) = std::fs::read_to_string(entitlement_path) else {
        return EntitlementStatus {
            is_pro: false,
            reason: "missing entitlement file".to_string(),
            checked_at,
        };
    };

    if allow_insecure && !contents.trim().is_empty() {
        return EntitlementStatus {
            is_pro: true,
            reason: "bypass enabled via MINNA_PRO_BYPASS".to_string(),
            checked_at,
        };
    }

    let parts: Vec<&str> = contents.trim().split('.').collect();
    if parts.len() != 5 {
        return EntitlementStatus {
            is_pro: false,
            reason: "invalid JWE format".to_string(),
            checked_at,
        };
    }

    let header = base64_url_decode(parts[0]);
    if header.is_err() {
        return EntitlementStatus {
            is_pro: false,
            reason: "invalid JWE header".to_string(),
            checked_at,
        };
    }

    info!("Entitlement present but not verified; supply verifier to enable Pro features.");
    EntitlementStatus {
        is_pro: false,
        reason: "unverified JWE (verification not configured)".to_string(),
        checked_at,
    }
}

fn base64_url_decode(input: &str) -> Result<Vec<u8>> {
    let mut s = input.replace('-', "+").replace('_', "/");
    while !s.len().is_multiple_of(4) {
        s.push('=');
    }
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(s)
        .map_err(|e| anyhow::anyhow!(e))?;
    Ok(decoded)
}

fn clean_slack_text(text: &str, user_cache: &HashMap<String, String>) -> String {
    let re = Regex::new(r"<@([A-Z0-9]+)>").unwrap();
    re.replace_all(text, |caps: &regex::Captures| {
        let user_id = &caps[1];
        if let Some(name) = user_cache.get(user_id) {
            format!("@{}", name)
        } else {
            caps[0].to_string()
        }
    }).to_string()
}

fn resolve_slack_name(user_id: Option<&String>, user_cache: &HashMap<String, String>) -> String {
    user_id.and_then(|id| user_cache.get(id).cloned()).unwrap_or_else(|| "unknown".to_string())
}
