//! Notion provider implementation.
//!
//! Syncs pages and database items from Notion workspaces.

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use tracing::{info, warn};

use crate::Document;
use crate::progress::emit_progress;
use super::{SyncContext, SyncProvider, SyncSummary, call_with_backoff, calculate_since};

/// Notion provider for syncing pages and database items.
pub struct NotionProvider;

// Notion API version header
const NOTION_VERSION: &str = "2022-06-28";

#[async_trait]
impl SyncProvider for NotionProvider {
    fn name(&self) -> &'static str {
        "notion"
    }

    fn display_name(&self) -> &'static str {
        "Notion"
    }

    async fn sync(
        &self,
        ctx: &SyncContext<'_>,
        since_days: Option<i64>,
        mode: Option<&str>,
    ) -> Result<SyncSummary> {
        let token = ctx.registry.load_token("notion")?;

        // Get existing cursor for delta sync
        let cursor_str = ctx.get_sync_cursor("notion").await?;
        let since = calculate_since(since_days, mode, cursor_str.as_deref());
        let since_str = since.to_rfc3339();

        info!("Syncing Notion pages since {}", since_str);

        let mut documents_processed = 0;
        let mut pages_scanned = 0;
        let mut pagination_cursor: Option<String> = None;

        // Get batch limit from env
        let page_limit: usize = std::env::var("MINNA_NOTION_PAGE_LIMIT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(100);

        loop {
            // Search for pages modified since our cursor
            let search_body = serde_json::json!({
                "filter": {
                    "property": "object",
                    "value": "page"
                },
                "sort": {
                    "direction": "descending",
                    "timestamp": "last_edited_time"
                },
                "start_cursor": pagination_cursor,
                "page_size": std::cmp::min(page_limit, 100)  // API max is 100
            });

            let response = call_with_backoff("notion", || {
                ctx.http_client
                    .post("https://api.notion.com/v1/search")
                    .bearer_auth(&token)
                    .header("Notion-Version", NOTION_VERSION)
                    .json(&search_body)
            })
            .await?;

            let search_result: NotionSearchResponse = response.json().await?;

            for page in &search_result.results {
                pages_scanned += 1;

                // Check if page was modified after our since timestamp
                let last_edited = page.last_edited_time.as_deref().unwrap_or("");
                if !last_edited.is_empty() && last_edited < since_str.as_str() {
                    // Pages are sorted by last_edited_time desc, so we can break early
                    info!("Reached pages older than since timestamp, stopping");
                    pagination_cursor = None;
                    break;
                }

                // Fetch page content (blocks)
                let content = match self.fetch_page_content(ctx, &token, &page.id).await {
                    Ok(c) => c,
                    Err(e) => {
                        warn!("Failed to fetch content for page {}: {}", page.id, e);
                        String::new()
                    }
                };

                // Extract title
                let title = self.extract_title(page);

                // Build document
                let doc = Document {
                    id: None,
                    uri: page.url.clone().unwrap_or_else(|| format!("notion://{}", page.id)),
                    source: "notion".to_string(),
                    title: title.clone(),
                    body: self.format_body(page, &title, &content),
                    updated_at: parse_notion_timestamp(last_edited)
                        .unwrap_or_else(Utc::now),
                };

                ctx.index_document(doc).await?;
                documents_processed += 1;

                // Emit progress periodically
                if documents_processed % 10 == 0 {
                    emit_progress("notion", "syncing", &format!("{} pages indexed", documents_processed), Some(documents_processed));
                }
            }

            // Check pagination
            if !search_result.has_more || pagination_cursor.is_none() && search_result.next_cursor.is_none() {
                break;
            }
            pagination_cursor = search_result.next_cursor;

            // Safety limit
            if pages_scanned >= page_limit {
                info!("Reached page limit ({}), stopping", page_limit);
                break;
            }
        }

        // Update sync cursor
        let new_cursor = Utc::now().to_rfc3339();
        ctx.set_sync_cursor("notion", &new_cursor).await?;

        info!("Notion sync complete: {} pages scanned, {} documents indexed", pages_scanned, documents_processed);

        Ok(SyncSummary {
            provider: "notion".to_string(),
            items_scanned: pages_scanned,
            documents_processed,
            updated_at: new_cursor,
        })
    }

    async fn discover(&self, ctx: &SyncContext<'_>) -> Result<serde_json::Value> {
        let token = ctx.registry.load_token("notion")?;

        // Quick search to count available pages
        let response = call_with_backoff("notion", || {
            ctx.http_client
                .post("https://api.notion.com/v1/search")
                .bearer_auth(&token)
                .header("Notion-Version", NOTION_VERSION)
                .json(&serde_json::json!({
                    "page_size": 1
                }))
        })
        .await?;

        // We can't get a total count from Notion API, so we just verify access
        if response.status().is_success() {
            Ok(serde_json::json!({
                "provider": "notion",
                "status": "connected",
                "message": "Notion integration connected successfully"
            }))
        } else {
            Ok(serde_json::json!({
                "provider": "notion",
                "status": "error",
                "message": "Failed to connect to Notion"
            }))
        }
    }
}

impl NotionProvider {
    /// Fetch all blocks (content) for a page.
    async fn fetch_page_content(
        &self,
        ctx: &SyncContext<'_>,
        token: &str,
        page_id: &str,
    ) -> Result<String> {
        let mut content = String::new();
        let mut cursor: Option<String> = None;

        loop {
            let url = format!(
                "https://api.notion.com/v1/blocks/{}/children{}",
                page_id,
                cursor.as_ref().map(|c| format!("?start_cursor={}", c)).unwrap_or_default()
            );

            let response = call_with_backoff("notion", || {
                ctx.http_client
                    .get(&url)
                    .bearer_auth(token)
                    .header("Notion-Version", NOTION_VERSION)
            })
            .await?;

            let blocks_result: NotionBlocksResponse = response.json().await?;

            for block in &blocks_result.results {
                let text = self.block_to_text(block);
                if !text.is_empty() {
                    content.push_str(&text);
                    content.push('\n');
                }

                // Recursively fetch children if present
                if block.has_children.unwrap_or(false) {
                    if let Ok(child_content) = Box::pin(self.fetch_page_content(ctx, token, &block.id)).await {
                        if !child_content.is_empty() {
                            content.push_str(&child_content);
                        }
                    }
                }
            }

            if !blocks_result.has_more {
                break;
            }
            cursor = blocks_result.next_cursor;
        }

        Ok(content.trim().to_string())
    }

    /// Convert a Notion block to plain text.
    fn block_to_text(&self, block: &NotionBlock) -> String {
        match block.block_type.as_str() {
            "paragraph" => self.extract_rich_text(&block.paragraph),
            "heading_1" => format!("# {}", self.extract_rich_text(&block.heading_1)),
            "heading_2" => format!("## {}", self.extract_rich_text(&block.heading_2)),
            "heading_3" => format!("### {}", self.extract_rich_text(&block.heading_3)),
            "bulleted_list_item" => format!("- {}", self.extract_rich_text(&block.bulleted_list_item)),
            "numbered_list_item" => format!("1. {}", self.extract_rich_text(&block.numbered_list_item)),
            "to_do" => {
                let checked = block.to_do.as_ref()
                    .and_then(|t| t.get("checked"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let mark = if checked { "[x]" } else { "[ ]" };
                format!("{} {}", mark, self.extract_rich_text(&block.to_do))
            }
            "toggle" => format!("> {}", self.extract_rich_text(&block.toggle)),
            "code" => {
                let lang = block.code.as_ref()
                    .and_then(|c| c.get("language"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                format!("```{}\n{}\n```", lang, self.extract_rich_text(&block.code))
            }
            "quote" => format!("> {}", self.extract_rich_text(&block.quote)),
            "callout" => format!("[!] {}", self.extract_rich_text(&block.callout)),
            "divider" => "---".to_string(),
            "table_of_contents" => "[Table of Contents]".to_string(),
            "child_page" => {
                let title = block.child_page.as_ref()
                    .and_then(|c| c.get("title"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("Untitled");
                format!("[Page: {}]", title)
            }
            "child_database" => {
                let title = block.child_database.as_ref()
                    .and_then(|c| c.get("title"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("Untitled");
                format!("[Database: {}]", title)
            }
            "image" | "video" | "file" | "pdf" => {
                format!("[{}]", block.block_type)
            }
            "bookmark" => {
                let url = block.bookmark.as_ref()
                    .and_then(|b| b.get("url"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                format!("[Bookmark: {}]", url)
            }
            "link_preview" => {
                let url = block.link_preview.as_ref()
                    .and_then(|l| l.get("url"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                format!("[Link: {}]", url)
            }
            "equation" => {
                let expr = block.equation.as_ref()
                    .and_then(|e| e.get("expression"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                format!("${}$", expr)
            }
            _ => String::new(),
        }
    }

    /// Extract plain text from rich_text array in a block content.
    fn extract_rich_text(&self, content: &Option<serde_json::Value>) -> String {
        content
            .as_ref()
            .and_then(|c| c.get("rich_text"))
            .and_then(|rt| rt.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| item.get("plain_text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default()
    }

    /// Extract title from page properties.
    fn extract_title(&self, page: &NotionObject) -> Option<String> {
        // Try to get title from properties
        page.properties.as_ref().and_then(|props| {
            // Look for common title property names
            for key in ["title", "Title", "Name", "name"] {
                if let Some(prop) = props.get(key) {
                    if let Some(title_arr) = prop.get("title").and_then(|t| t.as_array()) {
                        let title: String = title_arr
                            .iter()
                            .filter_map(|item| item.get("plain_text").and_then(|t| t.as_str()))
                            .collect();
                        if !title.is_empty() {
                            return Some(title);
                        }
                    }
                }
            }
            None
        })
    }

    /// Format the document body with metadata header.
    fn format_body(&self, page: &NotionObject, title: &Option<String>, content: &str) -> String {
        let mut body = String::new();

        // Title
        if let Some(t) = title {
            body.push_str(&format!("# {}\n\n", t));
        }

        // Metadata
        body.push_str("- Type: Notion Page\n");
        if let Some(edited) = &page.last_edited_time {
            body.push_str(&format!("- Last Edited: {}\n", edited));
        }
        if let Some(url) = &page.url {
            body.push_str(&format!("- URL: {}\n", url));
        }
        body.push('\n');

        // Content
        if !content.is_empty() {
            body.push_str(content);
        }

        body
    }
}

/// Parse Notion timestamp to DateTime.
fn parse_notion_timestamp(ts: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(ts)
        .map(|dt| dt.with_timezone(&Utc))
        .ok()
}

// ---- Notion API Response Types ----

#[derive(Debug, Deserialize)]
struct NotionSearchResponse {
    results: Vec<NotionObject>,
    #[serde(default)]
    next_cursor: Option<String>,
    #[serde(default)]
    has_more: bool,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct NotionObject {
    object: String,
    id: String,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    created_time: Option<String>,
    #[serde(default)]
    last_edited_time: Option<String>,
    #[serde(default)]
    properties: Option<serde_json::Value>,
    #[serde(default)]
    parent: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct NotionBlocksResponse {
    results: Vec<NotionBlock>,
    #[serde(default)]
    next_cursor: Option<String>,
    #[serde(default)]
    has_more: bool,
}

#[derive(Debug, Deserialize)]
struct NotionBlock {
    id: String,
    #[serde(rename = "type")]
    block_type: String,
    #[serde(default)]
    has_children: Option<bool>,

    // Block content fields (each block type has its own)
    #[serde(default)]
    paragraph: Option<serde_json::Value>,
    #[serde(default)]
    heading_1: Option<serde_json::Value>,
    #[serde(default)]
    heading_2: Option<serde_json::Value>,
    #[serde(default)]
    heading_3: Option<serde_json::Value>,
    #[serde(default)]
    bulleted_list_item: Option<serde_json::Value>,
    #[serde(default)]
    numbered_list_item: Option<serde_json::Value>,
    #[serde(default)]
    to_do: Option<serde_json::Value>,
    #[serde(default)]
    toggle: Option<serde_json::Value>,
    #[serde(default)]
    code: Option<serde_json::Value>,
    #[serde(default)]
    quote: Option<serde_json::Value>,
    #[serde(default)]
    callout: Option<serde_json::Value>,
    #[serde(default)]
    child_page: Option<serde_json::Value>,
    #[serde(default)]
    child_database: Option<serde_json::Value>,
    #[serde(default)]
    bookmark: Option<serde_json::Value>,
    #[serde(default)]
    link_preview: Option<serde_json::Value>,
    #[serde(default)]
    equation: Option<serde_json::Value>,
}
