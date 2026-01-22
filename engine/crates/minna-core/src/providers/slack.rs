//! Slack provider implementation.
//!
//! Syncs messages from Slack channels and DMs, extracting relationship edges for Gravity Well.

use std::collections::HashMap;

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use regex::Regex;
use serde::Deserialize;
use tracing::{info, warn};

use crate::Document;
use crate::progress::emit_progress;
use minna_auth_bridge::TokenStore;

use super::{
    call_with_backoff, ExtractedEdge, NodeRef, NodeType, Relation,
    SyncContext, SyncProvider, SyncSummary,
};

/// Slack provider for syncing messages.
pub struct SlackProvider;

#[async_trait]
impl SyncProvider for SlackProvider {
    fn name(&self) -> &'static str {
        "slack"
    }

    fn display_name(&self) -> &'static str {
        "Slack"
    }

    async fn sync(
        &self,
        ctx: &SyncContext<'_>,
        since_days: Option<i64>,
        mode: Option<&str>,
    ) -> Result<SyncSummary> {
        info!("Starting Slack sync (since_days: {:?}, mode: {:?})", since_days, mode);

        // Load OAuth token
        let token_store = TokenStore::load(ctx.auth_path)?;
        let token = token_store
            .get(minna_auth_bridge::Provider::Slack)
            .ok_or_else(|| anyhow::anyhow!("missing slack token"))?;

        // Get own user ID for self-identification
        let auth_response = ctx.http_client
            .post("https://slack.com/api/auth.test")
            .header("Authorization", format!("Bearer {}", token.access_token))
            .send()
            .await?;
        let auth_test: SlackAuthTestResponse = auth_response.json().await?;
        let my_user_id = auth_test.user_id.clone().unwrap_or_default();
        info!("Slack sync context: my_user_id={}", my_user_id);

        // Build user directory cache
        let user_cache = self.build_user_cache(ctx, &token.access_token).await?;
        info!("Slack user directory cached: {} users", user_cache.len());

        let is_full_sync = mode == Some("full");
        let oldest = self.calculate_oldest(ctx, since_days, is_full_sync).await?;

        let channel_limit = self.get_channel_limit(is_full_sync);
        let message_limit = self.get_message_limit(is_full_sync);

        // Fetch channels
        let channels = self.fetch_channels(ctx, &token.access_token, channel_limit).await?;
        info!("Scanning messages in {} Slack channels", channels.len());

        // Separate DMs from regular channels
        let (dms, regular_channels): (Vec<_>, Vec<_>) = channels
            .into_iter()
            .partition(|c| c.is_im == Some(true) || c.is_mpim == Some(true));

        info!("Processing {} DMs and {} channels", dms.len(), regular_channels.len());

        let mut max_ts = oldest.parse::<f64>().unwrap_or(0.0);
        let mut docs_indexed = 0usize;
        let mut edges_extracted = 0usize;
        let mut channels_scanned = 0usize;

        // Process DMs first
        if !dms.is_empty() {
            emit_progress("slack", "syncing", "Checking your DMs...", Some(docs_indexed));
            let (indexed, edges, ts) = self
                .process_channels(
                    ctx,
                    &token.access_token,
                    &dms,
                    &user_cache,
                    &oldest,
                    max_ts,
                    is_full_sync,
                    message_limit,
                    &my_user_id,
                )
                .await?;
            docs_indexed += indexed;
            edges_extracted += edges;
            channels_scanned += dms.len();
            if ts > max_ts {
                max_ts = ts;
            }
        }

        // Process regular channels
        if !regular_channels.is_empty() {
            emit_progress("slack", "syncing", "Reading your channels...", Some(docs_indexed));
            let (indexed, edges, ts) = self
                .process_channels(
                    ctx,
                    &token.access_token,
                    &regular_channels,
                    &user_cache,
                    &oldest,
                    max_ts,
                    is_full_sync,
                    message_limit,
                    &my_user_id,
                )
                .await?;
            docs_indexed += indexed;
            edges_extracted += edges;
            channels_scanned += regular_channels.len();
            if ts > max_ts {
                max_ts = ts;
            }
        }

        // Update sync cursor
        let cursor = format!("{:.6}", max_ts);
        ctx.set_sync_cursor("slack", &cursor).await?;

        info!(
            "Slack sync complete: {} channels, {} docs, {} edges",
            channels_scanned, docs_indexed, edges_extracted
        );

        Ok(SyncSummary {
            provider: "slack".to_string(),
            items_scanned: channels_scanned,
            documents_processed: docs_indexed,
            updated_at: cursor,
        })
    }
}

impl SlackProvider {
    /// Build user ID -> name cache for @mention resolution.
    async fn build_user_cache(
        &self,
        ctx: &SyncContext<'_>,
        access_token: &str,
    ) -> Result<HashMap<String, String>> {
        let mut cache = HashMap::new();
        let mut cursor: Option<String> = None;

        loop {
            let mut params = vec![("limit", "1000".to_string())];
            if let Some(c) = cursor.as_ref() {
                params.push(("cursor", c.clone()));
            }

            let response = call_with_backoff("slack", || {
                ctx.http_client
                    .get("https://slack.com/api/users.list")
                    .header("Authorization", format!("Bearer {}", access_token))
                    .query(&params)
            })
            .await?;

            let payload: SlackUsersResponse = response.json().await?;
            if !payload.ok {
                break;
            }

            if let Some(members) = payload.members {
                for member in members {
                    let name = member
                        .profile
                        .real_name
                        .or(member.profile.display_name)
                        .unwrap_or_else(|| member.id.clone());
                    cache.insert(member.id, name);
                }
            }

            cursor = payload
                .response_metadata
                .and_then(|m| m.next_cursor)
                .filter(|c| !c.is_empty());

            if cursor.is_none() {
                break;
            }
        }

        Ok(cache)
    }

    /// Calculate oldest timestamp for sync window.
    async fn calculate_oldest(
        &self,
        ctx: &SyncContext<'_>,
        since_days: Option<i64>,
        is_full_sync: bool,
    ) -> Result<String> {
        if is_full_sync {
            let days = since_days.unwrap_or(90);
            info!("Slack: performing full sync (last {} days)", days);
            Ok(slack_ts_from_datetime(Utc::now() - chrono::Duration::days(days)))
        } else if let Some(days) = since_days {
            info!("Slack: performing quick sync (last {} days)", days);
            Ok(slack_ts_from_datetime(Utc::now() - chrono::Duration::days(days)))
        } else {
            let cursor = ctx.get_sync_cursor("slack").await?.unwrap_or_default();
            if cursor.is_empty() {
                info!("Slack: no cursor found, defaulting to 30 days");
                Ok(slack_ts_from_datetime(Utc::now() - chrono::Duration::days(30)))
            } else {
                info!("Slack: delta sync from cursor: {}", cursor);
                Ok(cursor)
            }
        }
    }

    fn get_channel_limit(&self, is_full_sync: bool) -> usize {
        if is_full_sync {
            std::env::var("MINNA_SLACK_CHANNEL_LIMIT_FULL")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1000)
        } else {
            std::env::var("MINNA_SLACK_CHANNEL_LIMIT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(200)
        }
    }

    fn get_message_limit(&self, is_full_sync: bool) -> usize {
        if is_full_sync {
            std::env::var("MINNA_SLACK_MESSAGE_LIMIT_FULL")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1000)
        } else {
            std::env::var("MINNA_SLACK_MESSAGE_LIMIT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(200)
        }
    }

    /// Fetch user's channels (public, private, DMs, group DMs).
    async fn fetch_channels(
        &self,
        ctx: &SyncContext<'_>,
        access_token: &str,
        limit: usize,
    ) -> Result<Vec<SlackChannel>> {
        let mut channels = Vec::new();
        let mut cursor: Option<String> = None;

        while channels.len() < limit {
            let mut params: Vec<(&str, String)> = vec![
                ("limit", "200".to_string()),
                ("types", "public_channel,private_channel,mpim,im".to_string()),
            ];
            if let Some(next) = cursor.as_ref() {
                params.push(("cursor", next.clone()));
            }

            let response = call_with_backoff("slack", || {
                ctx.http_client
                    .get("https://slack.com/api/users.conversations")
                    .header("Authorization", format!("Bearer {}", access_token))
                    .query(&params)
            })
            .await?;

            let payload: SlackChannelsResponse = response.json().await?;
            if !payload.ok {
                return Err(anyhow::anyhow!(
                    "Slack conversations.list failed: {}",
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

        Ok(channels)
    }

    /// Process a set of channels, indexing messages and extracting edges.
    #[allow(clippy::too_many_arguments)]
    async fn process_channels(
        &self,
        ctx: &SyncContext<'_>,
        access_token: &str,
        channels: &[SlackChannel],
        user_cache: &HashMap<String, String>,
        oldest: &str,
        mut max_ts: f64,
        is_full_sync: bool,
        message_limit: usize,
        my_user_id: &str,
    ) -> Result<(usize, usize, f64)> {
        let mut docs_indexed = 0usize;
        let mut edges_extracted = 0usize;

        for channel in channels {
            let channel_name = channel
                .name
                .as_ref()
                .or(channel.name_normalized.as_ref())
                .map(|s| s.as_str())
                .unwrap_or_else(|| {
                    if channel.is_im == Some(true) {
                        "DM"
                    } else {
                        "Unnamed"
                    }
                });

            info!("  -> Scanning channel: #{} ({})", channel_name, channel.id);
            emit_progress(
                "slack",
                "syncing",
                &format!("Scanning #{}", channel_name),
                Some(docs_indexed),
            );

            let mut history_cursor: Option<String> = None;

            loop {
                let mut params = vec![
                    ("channel", channel.id.clone()),
                    ("oldest", oldest.to_string()),
                    ("limit", "1000".to_string()),
                ];
                if let Some(c) = history_cursor.as_ref() {
                    params.push(("cursor", c.clone()));
                }

                let response = call_with_backoff("slack", || {
                    ctx.http_client
                        .get("https://slack.com/api/conversations.history")
                        .header("Authorization", format!("Bearer {}", access_token))
                        .query(&params)
                })
                .await?;

                let payload: SlackHistoryResponse = response.json().await?;
                if !payload.ok {
                    warn!(
                        "Slack history failed for channel {}: {:?}",
                        channel.id, payload.error
                    );
                    break;
                }

                if let Some(messages) = payload.messages {
                    if messages.is_empty() {
                        break;
                    }

                    for message in messages {
                        // Skip replies in main loop - handled via thread parent
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

                            let updated_at =
                                slack_ts_to_datetime(&message.ts).unwrap_or_else(Utc::now);
                            let permalink = slack_permalink(&channel.id, &message.ts);
                            let author_name = resolve_slack_name(message.user.as_ref(), user_cache);
                            let clean_body_text = clean_slack_text(text, user_cache);

                            let mut full_body = format!(
                                "# Slack Thread: #{}\n- Author: {}\n- Created: {}\n- URL: {}\n\n**{}**: {}",
                                channel_name,
                                author_name,
                                updated_at.to_rfc3339(),
                                permalink,
                                author_name,
                                clean_body_text
                            );

                            // Collect thread participants for edge extraction
                            let mut thread_participants: Vec<String> = Vec::new();
                            if let Some(ref user_id) = message.user {
                                thread_participants.push(user_id.clone());
                            }

                            // Fetch and consolidate thread replies
                            if let Some(reply_count) = message.reply_count {
                                if reply_count > 0 {
                                    let (reply_text, reply_users) = self
                                        .fetch_thread_replies(
                                            ctx,
                                            access_token,
                                            &channel.id,
                                            &message.ts,
                                            user_cache,
                                        )
                                        .await?;
                                    full_body.push_str(&reply_text);
                                    thread_participants.extend(reply_users);
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

                            ctx.index_document(doc).await?;
                            docs_indexed += 1;

                            // Extract and store edges
                            let edges = self.extract_edges_from_message(
                                &channel.id,
                                channel_name,
                                &message,
                                &thread_participants,
                                text,
                                user_cache,
                                my_user_id,
                                updated_at,
                            );
                            if !edges.is_empty() {
                                ctx.index_edges(&edges).await?;
                                edges_extracted += edges.len();
                            }

                            if docs_indexed % 20 == 0 {
                                emit_progress(
                                    "slack",
                                    "syncing",
                                    &format!("#{}: {} docs", channel_name, docs_indexed),
                                    Some(docs_indexed),
                                );
                            }
                        }
                    }
                }

                history_cursor = payload
                    .response_metadata
                    .and_then(|m| m.next_cursor)
                    .filter(|c| !c.is_empty());

                if history_cursor.is_none() || (!is_full_sync && docs_indexed > message_limit) {
                    break;
                }
            }
        }

        Ok((docs_indexed, edges_extracted, max_ts))
    }

    /// Fetch thread replies and return (formatted text, participant user IDs).
    async fn fetch_thread_replies(
        &self,
        ctx: &SyncContext<'_>,
        access_token: &str,
        channel_id: &str,
        thread_ts: &str,
        user_cache: &HashMap<String, String>,
    ) -> Result<(String, Vec<String>)> {
        let mut text = String::new();
        let mut users = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let mut params = vec![
                ("channel", channel_id.to_string()),
                ("ts", thread_ts.to_string()),
                ("limit", "100".to_string()),
            ];
            if let Some(c) = cursor.as_ref() {
                params.push(("cursor", c.clone()));
            }

            let response = call_with_backoff("slack", || {
                ctx.http_client
                    .get("https://slack.com/api/conversations.replies")
                    .header("Authorization", format!("Bearer {}", access_token))
                    .query(&params)
            })
            .await?;

            let payload: SlackHistoryResponse = response.json().await?;
            if !payload.ok {
                break;
            }

            if let Some(replies) = payload.messages {
                for reply in replies {
                    // Skip the parent
                    if reply.ts == thread_ts {
                        continue;
                    }

                    if let Some(ref user_id) = reply.user {
                        users.push(user_id.clone());
                    }

                    if let Some(r_text) = reply.text.as_ref() {
                        let r_author = resolve_slack_name(reply.user.as_ref(), user_cache);
                        let r_clean = clean_slack_text(r_text, user_cache);
                        text.push_str(&format!("\n\n**{}**: {}", r_author, r_clean));
                    }
                }
            }

            cursor = payload
                .response_metadata
                .and_then(|m| m.next_cursor)
                .filter(|c| !c.is_empty());

            if cursor.is_none() {
                break;
            }
        }

        Ok((text, users))
    }

    /// Extract relationship edges from a Slack message.
    #[allow(clippy::too_many_arguments)]
    fn extract_edges_from_message(
        &self,
        channel_id: &str,
        channel_name: &str,
        message: &SlackMessage,
        thread_participants: &[String],
        text: &str,
        user_cache: &HashMap<String, String>,
        _my_user_id: &str,
        observed_at: DateTime<Utc>,
    ) -> Vec<ExtractedEdge> {
        let mut edges = Vec::new();

        // Message node
        let message_node = NodeRef::new(
            NodeType::Message,
            "slack",
            format!("{}:{}", channel_id, message.ts),
        );

        // Channel node
        let channel_node = NodeRef::with_name(NodeType::Channel, "slack", channel_id, channel_name);

        // Edge: Message → Channel (PostedIn)
        edges.push(ExtractedEdge::new(
            message_node.clone(),
            channel_node.clone(),
            Relation::PostedIn,
            observed_at,
        ));

        // Edge: Author → Message (AuthorOf)
        if let Some(ref user_id) = message.user {
            let user_name = user_cache.get(user_id).cloned().unwrap_or_else(|| user_id.clone());
            let user_node = NodeRef::with_name(NodeType::User, "slack", user_id, &user_name);

            edges.push(ExtractedEdge::new(
                user_node.clone(),
                message_node.clone(),
                Relation::AuthorOf,
                observed_at,
            ));

            // Edge: Author → Channel (MemberOf) - inferred from posting
            edges.push(ExtractedEdge::new(
                user_node,
                channel_node.clone(),
                Relation::MemberOf,
                observed_at,
            ));
        }

        // Edge: Thread participants → Channel (MemberOf)
        for user_id in thread_participants {
            if Some(user_id) != message.user.as_ref() {
                let user_name = user_cache.get(user_id).cloned().unwrap_or_else(|| user_id.clone());
                let user_node = NodeRef::with_name(NodeType::User, "slack", user_id, &user_name);

                edges.push(ExtractedEdge::new(
                    user_node,
                    channel_node.clone(),
                    Relation::MemberOf,
                    observed_at,
                ));
            }
        }

        // Edge: @mentioned users → Message (MentionedIn)
        let mention_re = Regex::new(r"<@([A-Z0-9]+)>").unwrap();
        for cap in mention_re.captures_iter(text) {
            if let Some(user_id) = cap.get(1) {
                let user_id = user_id.as_str();
                let user_name = user_cache
                    .get(user_id)
                    .cloned()
                    .unwrap_or_else(|| user_id.to_string());
                let user_node =
                    NodeRef::with_name(NodeType::User, "slack", user_id, &user_name);

                edges.push(ExtractedEdge::new(
                    user_node,
                    message_node.clone(),
                    Relation::MentionedIn,
                    observed_at,
                ));
            }
        }

        edges
    }
}

// --- Helper Functions ---

fn slack_ts_from_datetime(dt: DateTime<Utc>) -> String {
    format!("{}.000000", dt.timestamp())
}

fn slack_ts_to_datetime(ts: &str) -> Option<DateTime<Utc>> {
    let secs = ts.split('.').next()?.parse::<i64>().ok()?;
    Utc.timestamp_opt(secs, 0).single()
}

fn slack_permalink(channel_id: &str, ts: &str) -> String {
    let ts_clean = ts.replace('.', "");
    format!(
        "https://slack.com/archives/{}/p{}",
        channel_id, ts_clean
    )
}

fn resolve_slack_name(user_id: Option<&String>, cache: &HashMap<String, String>) -> String {
    user_id
        .and_then(|id| cache.get(id))
        .cloned()
        .unwrap_or_else(|| "Unknown".to_string())
}

fn clean_slack_text(text: &str, user_cache: &HashMap<String, String>) -> String {
    let mention_re = Regex::new(r"<@([A-Z0-9]+)>").unwrap();
    mention_re
        .replace_all(text, |caps: &regex::Captures| {
            let user_id = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let name = user_cache
                .get(user_id)
                .cloned()
                .unwrap_or_else(|| user_id.to_string());
            format!("@{}", name)
        })
        .to_string()
}

// --- Slack API Response Types ---

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
struct SlackAuthTestResponse {
    #[allow(dead_code)]
    ok: bool,
    user_id: Option<String>,
    #[allow(dead_code)]
    error: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct SlackChannel {
    id: String,
    name: Option<String>,
    name_normalized: Option<String>,
    is_im: Option<bool>,
    is_mpim: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
struct SlackHistoryResponse {
    ok: bool,
    messages: Option<Vec<SlackMessage>>,
    #[allow(dead_code)]
    error: Option<String>,
    response_metadata: Option<SlackResponseMetadata>,
}

#[derive(Debug, Clone, Deserialize)]
struct SlackMessage {
    ts: String,
    user: Option<String>,
    text: Option<String>,
    thread_ts: Option<String>,
    reply_count: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
struct SlackUsersResponse {
    ok: bool,
    members: Option<Vec<SlackUser>>,
    #[allow(dead_code)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slack_ts_conversion() {
        let ts = "1704067200.000000";
        let dt = slack_ts_to_datetime(ts).unwrap();
        assert_eq!(dt.timestamp(), 1704067200);

        let back = slack_ts_from_datetime(dt);
        assert!(back.starts_with("1704067200"));
    }

    #[test]
    fn test_clean_slack_text() {
        let mut cache = HashMap::new();
        cache.insert("U12345".to_string(), "Alice".to_string());

        let text = "Hey <@U12345>, can you review this?";
        let cleaned = clean_slack_text(text, &cache);
        assert_eq!(cleaned, "Hey @Alice, can you review this?");
    }

    #[test]
    fn test_extract_mentions() {
        let re = Regex::new(r"<@([A-Z0-9]+)>").unwrap();
        let text = "Hey <@U12345> and <@U67890>, please review";

        let mentions: Vec<_> = re
            .captures_iter(text)
            .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
            .collect();

        assert_eq!(mentions, vec!["U12345", "U67890"]);
    }
}
