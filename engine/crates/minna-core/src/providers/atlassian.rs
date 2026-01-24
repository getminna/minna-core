//! Atlassian provider implementation.
//!
//! Syncs Jira issues and Confluence pages from Atlassian Cloud.

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use tracing::info;

use crate::Document;
use crate::progress::emit_progress;
use super::{SyncContext, SyncProvider, SyncSummary, call_with_backoff, calculate_since};

/// Atlassian provider for syncing Jira issues and Confluence pages.
pub struct AtlassianProvider;

#[async_trait]
impl SyncProvider for AtlassianProvider {
    fn name(&self) -> &'static str {
        "atlassian"
    }

    fn display_name(&self) -> &'static str {
        "Atlassian (Jira/Confluence)"
    }

    async fn sync(
        &self,
        ctx: &SyncContext<'_>,
        since_days: Option<i64>,
        mode: Option<&str>,
    ) -> Result<SyncSummary> {
        // Parse Basic Auth credentials (email:token)
        let (email, token) = ctx.registry.parse_basic_auth("atlassian")?;

        // Get cloud ID (required for API calls)
        let cloud_id = self.get_cloud_id(ctx, &email, &token).await?;
        info!("Connected to Atlassian cloud: {}", cloud_id);

        // Sync Jira issues
        let jira_result = self.sync_jira(ctx, &cloud_id, &email, &token, since_days, mode).await?;
        info!("Jira sync: {} issues indexed", jira_result.documents_processed);

        // Sync Confluence pages
        let confluence_result = self.sync_confluence(ctx, &cloud_id, &email, &token, since_days, mode).await?;
        info!("Confluence sync: {} pages indexed", confluence_result.documents_processed);

        // Update sync cursor
        let new_cursor = Utc::now().to_rfc3339();
        ctx.set_sync_cursor("atlassian", &new_cursor).await?;

        Ok(SyncSummary {
            provider: "atlassian".to_string(),
            items_scanned: jira_result.items_scanned + confluence_result.items_scanned,
            documents_processed: jira_result.documents_processed + confluence_result.documents_processed,
            updated_at: new_cursor,
        })
    }

    async fn discover(&self, ctx: &SyncContext<'_>) -> Result<serde_json::Value> {
        let (email, token) = ctx.registry.parse_basic_auth("atlassian")?;

        // Get accessible resources
        let resources = self.get_accessible_resources(ctx, &email, &token).await?;

        Ok(serde_json::json!({
            "provider": "atlassian",
            "status": "connected",
            "sites": resources.iter().map(|r| serde_json::json!({
                "id": r.id,
                "name": r.name,
                "url": r.url
            })).collect::<Vec<_>>()
        }))
    }
}

impl AtlassianProvider {
    /// Get the cloud ID for API calls.
    async fn get_cloud_id(
        &self,
        ctx: &SyncContext<'_>,
        email: &str,
        token: &str,
    ) -> Result<String> {
        let resources = self.get_accessible_resources(ctx, email, token).await?;

        resources
            .first()
            .map(|r| r.id.clone())
            .ok_or_else(|| anyhow!("No accessible Atlassian sites. Check your API token permissions."))
    }

    /// Get list of accessible Atlassian resources.
    async fn get_accessible_resources(
        &self,
        ctx: &SyncContext<'_>,
        email: &str,
        token: &str,
    ) -> Result<Vec<AtlassianResource>> {
        let response = call_with_backoff("atlassian", || {
            ctx.http_client
                .get("https://api.atlassian.com/oauth/token/accessible-resources")
                .basic_auth(email, Some(token))
        })
        .await?;

        let resources: Vec<AtlassianResource> = response.json().await?;
        Ok(resources)
    }

    /// Sync Jira issues.
    async fn sync_jira(
        &self,
        ctx: &SyncContext<'_>,
        cloud_id: &str,
        email: &str,
        token: &str,
        since_days: Option<i64>,
        mode: Option<&str>,
    ) -> Result<SyncSummary> {
        // Get cursor for delta sync
        let cursor_str = ctx.get_sync_cursor("jira").await?;
        let since = calculate_since(since_days, mode, cursor_str.as_deref());
        let since_jql = since.format("%Y-%m-%d").to_string();

        info!("Syncing Jira issues since {}", since_jql);

        let base_url = format!(
            "https://api.atlassian.com/ex/jira/{}/rest/api/3",
            cloud_id
        );

        let mut documents_processed = 0;
        let mut issues_scanned = 0;
        let mut start_at = 0;

        let issue_limit: usize = std::env::var("MINNA_JIRA_ISSUE_LIMIT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(100);

        // JQL to get recently updated issues
        let jql = format!("updated >= '{}' ORDER BY updated DESC", since_jql);

        loop {
            let response = call_with_backoff("jira", || {
                ctx.http_client
                    .get(format!("{}/search", base_url))
                    .basic_auth(email, Some(token))
                    .query(&[
                        ("jql", jql.as_str()),
                        ("startAt", &start_at.to_string()),
                        ("maxResults", "100"),
                        ("fields", "summary,description,status,assignee,reporter,updated,created,project,issuetype,priority"),
                    ])
            })
            .await?;

            let search_result: JiraSearchResponse = response.json().await?;

            for issue in &search_result.issues {
                issues_scanned += 1;

                // Build browse URL
                let browse_url = format!(
                    "https://api.atlassian.com/ex/jira/{}/browse/{}",
                    cloud_id, issue.key
                );

                // Convert ADF description to text
                let description = issue.fields.description.as_ref()
                    .map(|d| self.adf_to_text(d))
                    .unwrap_or_default();

                // Build document
                let doc = Document {
                    id: None,
                    uri: browse_url.clone(),
                    source: "jira".to_string(),
                    title: Some(format!("{}: {}", issue.key, issue.fields.summary)),
                    body: self.format_jira_body(issue, &description, &browse_url),
                    updated_at: parse_atlassian_timestamp(&issue.fields.updated)
                        .unwrap_or_else(Utc::now),
                };

                ctx.index_document(doc).await?;
                documents_processed += 1;

                if documents_processed % 10 == 0 {
                    emit_progress("jira", "syncing", &format!("{} issues indexed", documents_processed), Some(documents_processed));
                }
            }

            // Check pagination
            let total = search_result.total as usize;
            start_at += search_result.issues.len();

            if start_at >= total || start_at >= issue_limit {
                break;
            }
        }

        // Update Jira-specific cursor
        ctx.set_sync_cursor("jira", &Utc::now().to_rfc3339()).await?;

        Ok(SyncSummary {
            provider: "jira".to_string(),
            items_scanned: issues_scanned,
            documents_processed,
            updated_at: Utc::now().to_rfc3339(),
        })
    }

    /// Sync Confluence pages.
    async fn sync_confluence(
        &self,
        ctx: &SyncContext<'_>,
        cloud_id: &str,
        email: &str,
        token: &str,
        since_days: Option<i64>,
        mode: Option<&str>,
    ) -> Result<SyncSummary> {
        // Get cursor for delta sync
        let cursor_str = ctx.get_sync_cursor("confluence").await?;
        let since = calculate_since(since_days, mode, cursor_str.as_deref());

        info!("Syncing Confluence pages since {}", since.to_rfc3339());

        let base_url = format!(
            "https://api.atlassian.com/ex/confluence/{}/wiki/rest/api",
            cloud_id
        );

        let mut documents_processed = 0;
        let mut pages_scanned = 0;
        let mut next_link: Option<String> = None;

        let page_limit: usize = std::env::var("MINNA_CONFLUENCE_PAGE_LIMIT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(100);

        loop {
            let url = next_link.clone().unwrap_or_else(|| {
                format!("{}/content", base_url)
            });

            let mut request = ctx.http_client
                .get(&url)
                .basic_auth(email, Some(token));

            // Only add params on first request (not when following next link)
            if next_link.is_none() {
                request = request.query(&[
                    ("expand", "space,body.storage,version"),
                    ("limit", "25"),
                    ("orderby", "history.lastUpdated desc"),
                ]);
            }

            let response = call_with_backoff("confluence", || {
                ctx.http_client
                    .get(&url)
                    .basic_auth(email, Some(token))
                    .query(&[
                        ("expand", "space,body.storage,version"),
                        ("limit", "25"),
                    ])
            })
            .await?;

            let search_result: ConfluenceSearchResponse = response.json().await?;

            for page in &search_result.results {
                pages_scanned += 1;

                // Check if page was updated after since
                let updated = page.version.as_ref()
                    .and_then(|v| parse_atlassian_timestamp(&v.when));

                if let Some(updated_dt) = updated {
                    if updated_dt < since {
                        // Pages are sorted by lastUpdated desc, so we can stop
                        info!("Reached pages older than since timestamp");
                        break;
                    }
                }

                // Get page URL
                let page_url = page.links.as_ref()
                    .and_then(|l| l.webui.as_ref())
                    .map(|webui| format!("https://api.atlassian.com/ex/confluence/{}/wiki{}", cloud_id, webui))
                    .unwrap_or_else(|| format!("confluence://{}/{}", cloud_id, page.id));

                // Extract body content
                let content = page.body.as_ref()
                    .and_then(|b| b.storage.as_ref())
                    .map(|s| self.strip_html(&s.value))
                    .unwrap_or_default();

                // Build document
                let doc = Document {
                    id: None,
                    uri: page_url.clone(),
                    source: "confluence".to_string(),
                    title: Some(page.title.clone()),
                    body: self.format_confluence_body(page, &content, &page_url),
                    updated_at: updated.unwrap_or_else(Utc::now),
                };

                ctx.index_document(doc).await?;
                documents_processed += 1;

                if documents_processed % 10 == 0 {
                    emit_progress("confluence", "syncing", &format!("{} pages indexed", documents_processed), Some(documents_processed));
                }
            }

            // Check pagination
            next_link = search_result.links.as_ref()
                .and_then(|l| l.next.clone())
                .map(|n| format!("https://api.atlassian.com/ex/confluence/{}/wiki{}", cloud_id, n));

            if next_link.is_none() || pages_scanned >= page_limit {
                break;
            }
        }

        // Update Confluence-specific cursor
        ctx.set_sync_cursor("confluence", &Utc::now().to_rfc3339()).await?;

        Ok(SyncSummary {
            provider: "confluence".to_string(),
            items_scanned: pages_scanned,
            documents_processed,
            updated_at: Utc::now().to_rfc3339(),
        })
    }

    /// Convert Atlassian Document Format (ADF) to plain text.
    fn adf_to_text(&self, adf: &serde_json::Value) -> String {
        let mut text = String::new();
        self.extract_adf_text(adf, &mut text);
        text.trim().to_string()
    }

    fn extract_adf_text(&self, node: &serde_json::Value, output: &mut String) {
        // Check if this is a text node
        if let Some(text) = node.get("text").and_then(|t| t.as_str()) {
            output.push_str(text);
            return;
        }

        // Handle block types
        if let Some(node_type) = node.get("type").and_then(|t| t.as_str()) {
            match node_type {
                "paragraph" | "heading" => {
                    // Process content, then add newline
                    if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                        for child in content {
                            self.extract_adf_text(child, output);
                        }
                    }
                    output.push('\n');
                }
                "bulletList" | "orderedList" => {
                    if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                        for child in content {
                            output.push_str("- ");
                            self.extract_adf_text(child, output);
                        }
                    }
                }
                "listItem" => {
                    if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                        for child in content {
                            self.extract_adf_text(child, output);
                        }
                    }
                }
                "codeBlock" => {
                    output.push_str("```\n");
                    if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                        for child in content {
                            self.extract_adf_text(child, output);
                        }
                    }
                    output.push_str("\n```\n");
                }
                "blockquote" => {
                    output.push_str("> ");
                    if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                        for child in content {
                            self.extract_adf_text(child, output);
                        }
                    }
                }
                _ => {
                    // Recursively process content
                    if let Some(content) = node.get("content").and_then(|c| c.as_array()) {
                        for child in content {
                            self.extract_adf_text(child, output);
                        }
                    }
                }
            }
        }
    }

    /// Strip HTML tags from Confluence storage format.
    fn strip_html(&self, html: &str) -> String {
        // Simple regex-based HTML stripping
        let tag_re = regex::Regex::new(r"<[^>]+>").unwrap();
        let entity_re = regex::Regex::new(r"&[a-zA-Z]+;").unwrap();

        let mut text = tag_re.replace_all(html, " ").to_string();
        text = entity_re.replace_all(&text, " ").to_string();

        // Normalize whitespace
        let ws_re = regex::Regex::new(r"\s+").unwrap();
        text = ws_re.replace_all(&text, " ").trim().to_string();

        text
    }

    /// Format Jira issue body.
    fn format_jira_body(&self, issue: &JiraIssue, description: &str, url: &str) -> String {
        let mut body = String::new();

        body.push_str(&format!("# {}: {}\n\n", issue.key, issue.fields.summary));

        // Metadata
        if let Some(issue_type) = &issue.fields.issue_type {
            body.push_str(&format!("- Type: {}\n", issue_type.name));
        }
        if let Some(status) = &issue.fields.status {
            body.push_str(&format!("- Status: {}\n", status.name));
        }
        if let Some(priority) = &issue.fields.priority {
            body.push_str(&format!("- Priority: {}\n", priority.name));
        }
        if let Some(assignee) = &issue.fields.assignee {
            body.push_str(&format!("- Assignee: {}\n", assignee.display_name));
        }
        if let Some(reporter) = &issue.fields.reporter {
            body.push_str(&format!("- Reporter: {}\n", reporter.display_name));
        }
        if let Some(project) = &issue.fields.project {
            body.push_str(&format!("- Project: {}\n", project.name));
        }
        body.push_str(&format!("- Updated: {}\n", issue.fields.updated));
        body.push_str(&format!("- URL: {}\n", url));
        body.push('\n');

        // Description
        if !description.is_empty() {
            body.push_str(description);
        }

        body
    }

    /// Format Confluence page body.
    fn format_confluence_body(&self, page: &ConfluencePage, content: &str, url: &str) -> String {
        let mut body = String::new();

        body.push_str(&format!("# {}\n\n", page.title));

        // Metadata
        if let Some(space) = &page.space {
            body.push_str(&format!("- Space: {}\n", space.name));
        }
        body.push_str(&format!("- Type: {}\n", page.content_type));
        if let Some(version) = &page.version {
            body.push_str(&format!("- Updated: {}\n", version.when));
        }
        body.push_str(&format!("- URL: {}\n", url));
        body.push('\n');

        // Content
        if !content.is_empty() {
            body.push_str(content);
        }

        body
    }
}

/// Parse Atlassian timestamp format.
fn parse_atlassian_timestamp(ts: &str) -> Option<DateTime<Utc>> {
    // Try ISO 8601 format first
    DateTime::parse_from_rfc3339(ts)
        .or_else(|_| DateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%S%.3fZ"))
        .or_else(|_| DateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%S%.f%:z"))
        .map(|dt| dt.with_timezone(&Utc))
        .ok()
}

// ---- Atlassian API Response Types ----

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct AtlassianResource {
    id: String,
    url: String,
    name: String,
    #[serde(default)]
    scopes: Vec<String>,
}

// Jira types

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct JiraSearchResponse {
    total: i64,
    #[serde(rename = "startAt")]
    start_at: i64,
    #[serde(rename = "maxResults")]
    max_results: i64,
    issues: Vec<JiraIssue>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct JiraIssue {
    id: String,
    key: String,
    #[serde(rename = "self")]
    self_url: String,
    fields: JiraIssueFields,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct JiraIssueFields {
    summary: String,
    #[serde(default)]
    description: Option<serde_json::Value>,
    #[serde(default)]
    status: Option<JiraStatus>,
    #[serde(default)]
    assignee: Option<JiraUser>,
    #[serde(default)]
    reporter: Option<JiraUser>,
    #[serde(default)]
    updated: String,
    #[serde(default)]
    created: String,
    #[serde(default)]
    project: Option<JiraProject>,
    #[serde(default, rename = "issuetype")]
    issue_type: Option<JiraIssueType>,
    #[serde(default)]
    priority: Option<JiraPriority>,
}

#[derive(Debug, Deserialize)]
struct JiraStatus {
    name: String,
}

#[derive(Debug, Deserialize)]
struct JiraUser {
    #[serde(rename = "displayName")]
    display_name: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct JiraProject {
    key: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct JiraIssueType {
    name: String,
}

#[derive(Debug, Deserialize)]
struct JiraPriority {
    name: String,
}

// Confluence types

#[derive(Debug, Deserialize)]
struct ConfluenceSearchResponse {
    results: Vec<ConfluencePage>,
    #[serde(default, rename = "_links")]
    links: Option<ConfluenceLinks>,
}

#[derive(Debug, Deserialize)]
struct ConfluenceLinks {
    #[serde(default)]
    next: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ConfluencePage {
    id: String,
    title: String,
    #[serde(rename = "type")]
    content_type: String,
    #[serde(default)]
    space: Option<ConfluenceSpace>,
    #[serde(default)]
    body: Option<ConfluenceBody>,
    #[serde(default)]
    version: Option<ConfluenceVersion>,
    #[serde(default, rename = "_links")]
    links: Option<ConfluencePageLinks>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ConfluenceSpace {
    key: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct ConfluenceBody {
    #[serde(default)]
    storage: Option<ConfluenceStorage>,
}

#[derive(Debug, Deserialize)]
struct ConfluenceStorage {
    value: String,
}

#[derive(Debug, Deserialize)]
struct ConfluenceVersion {
    when: String,
}

#[derive(Debug, Deserialize)]
struct ConfluencePageLinks {
    #[serde(default)]
    webui: Option<String>,
}
