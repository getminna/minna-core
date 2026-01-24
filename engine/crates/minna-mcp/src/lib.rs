use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::debug;

use minna_auth_bridge::{Provider, TokenStore};
use minna_core::{Checkpoint, CheckpointStore, LoadQuery};
use minna_graph::{GraphStore, Ring};
use minna_ingest::{Document, IngestionEngine};
use minna_vector::{Embedder, VectorStore};

#[derive(Debug, Serialize, Deserialize)]
pub struct ToolRequest {
    pub id: Option<String>,
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ToolResponse {
    pub id: Option<String>,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetContextParams {
    pub query: String,
    pub pack: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReadResourceParams {
    pub uri: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SaveStateParams {
    pub title: String,
    pub summary: String,
    pub task: String,
    pub next_steps: String,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default = "default_trigger")]
    pub trigger: String,
}

fn default_trigger() -> String {
    "manual".to_string()
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct LoadStateParams {
    pub title: Option<String>,
    pub version: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ContextItem {
    pub uri: String,
    pub source: String,
    pub title: Option<String>,
    pub score: f32,
    pub snippet: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ContextResult {
    pub mode: String,
    pub items: Vec<ContextItem>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResourceResult {
    pub uri: String,
    pub source: String,
    pub body: String,
}

#[derive(Clone)]
pub struct McpContext {
    pub ingest: Arc<IngestionEngine>,
    pub vector: Arc<VectorStore>,
    pub auth_store: Arc<RwLock<TokenStore>>,
    pub embedder: Arc<dyn Embedder>,
    pub graph: Option<Arc<GraphStore>>,
}

impl McpContext {
    pub fn new(
        ingest: IngestionEngine,
        vector: VectorStore,
        auth_store: TokenStore,
        embedder: Arc<dyn Embedder>,
    ) -> Self {
        Self {
            ingest: Arc::new(ingest),
            vector: Arc::new(vector),
            auth_store: Arc::new(RwLock::new(auth_store)),
            embedder,
            graph: None,
        }
    }

    /// Create with GraphStore for ring-boosted search.
    pub fn with_graph(
        ingest: IngestionEngine,
        vector: VectorStore,
        auth_store: TokenStore,
        embedder: Arc<dyn Embedder>,
        graph: GraphStore,
    ) -> Self {
        Self {
            ingest: Arc::new(ingest),
            vector: Arc::new(vector),
            auth_store: Arc::new(RwLock::new(auth_store)),
            embedder,
            graph: Some(Arc::new(graph)),
        }
    }
}

pub struct McpHandler {
    ctx: McpContext,
    router: SynchronousRouter,
}

impl McpHandler {
    pub fn new(ctx: McpContext) -> Self {
        let router = SynchronousRouter::new(ctx.auth_store.clone());
        Self { ctx, router }
    }

    pub async fn handle(&self, request: ToolRequest) -> ToolResponse {
        let tool = request.tool.clone().or(request.method.clone());
        let id = request.id.clone();
        match tool.as_deref() {
            Some("get_context") => match self.handle_get_context(request.params).await {
                Ok(result) => ToolResponse {
                    id,
                    ok: true,
                    result: Some(serde_json::to_value(result).unwrap_or_default()),
                    error: None,
                },
                Err(err) => ToolResponse {
                    id,
                    ok: false,
                    result: None,
                    error: Some(err.to_string()),
                },
            },
            Some("read_resource") => match self.handle_read_resource(request.params).await {
                Ok(result) => ToolResponse {
                    id,
                    ok: true,
                    result: Some(serde_json::to_value(result).unwrap_or_default()),
                    error: None,
                },
                Err(err) => ToolResponse {
                    id,
                    ok: false,
                    result: None,
                    error: Some(err.to_string()),
                },
            },
            Some("save_state") => match self.handle_save_state(request.params).await {
                Ok(result) => ToolResponse {
                    id,
                    ok: true,
                    result: Some(serde_json::json!({ "message": result })),
                    error: None,
                },
                Err(err) => ToolResponse {
                    id,
                    ok: false,
                    result: None,
                    error: Some(err.to_string()),
                },
            },
            Some("load_state") => match self.handle_load_state(request.params).await {
                Ok(result) => ToolResponse {
                    id,
                    ok: true,
                    result: Some(serde_json::json!({ "content": result })),
                    error: None,
                },
                Err(err) => ToolResponse {
                    id,
                    ok: false,
                    result: None,
                    error: Some(err.to_string()),
                },
            },
            _ => ToolResponse {
                id,
                ok: false,
                result: None,
                error: Some("unknown tool".to_string()),
            },
        }
    }

    async fn handle_save_state(&self, params: serde_json::Value) -> Result<String> {
        let params: SaveStateParams = serde_json::from_value(params)
            .map_err(|e| anyhow!("invalid save_state params: {}", e))?;

        let checkpoint = Checkpoint::new(
            params.title,
            params.summary,
            params.task,
            params.next_steps,
            params.files,
            params.trigger,
        );

        let store = CheckpointStore::default_path();
        let path = store.save(checkpoint)?;

        Ok(format!(
            "✅ Checkpoint saved to {}",
            path.display()
        ))
    }

    async fn handle_load_state(&self, params: serde_json::Value) -> Result<String> {
        let params: LoadStateParams = serde_json::from_value(params).unwrap_or_default();

        let query = match (params.title, params.version) {
            (Some(title), Some(version)) => LoadQuery::exact(title, version),
            (Some(title), None) => LoadQuery::by_title(title),
            (None, _) => LoadQuery::latest(),
        };

        let store = CheckpointStore::default_path();
        match store.load(query)? {
            Some(checkpoint) => Ok(checkpoint.to_markdown()),
            None => Err(anyhow!("no checkpoint found")),
        }
    }

    async fn handle_get_context(&self, params: serde_json::Value) -> Result<ContextResult> {
        let params = parse_get_context_params(params)?;
        let (query, inline_pack) = extract_pack(&params.query);
        let pack = params.pack.or(inline_pack);

        if let Some(sync) = self.router.try_sync(&query).await? {
            return Ok(ContextResult {
                mode: "instant_recall".to_string(),
                items: vec![ContextItem {
                    uri: sync.url.clone(),
                    source: sync.source,
                    title: sync.title,
                    score: 1.0,
                    snippet: truncate(&sync.markdown, 240),
                    content: Some(sync.markdown),
                }],
            });
        }

        let limit = params.limit.unwrap_or(6);
        let allowed_ids = if let Some(pack) = &pack {
            let ids = self.ctx.ingest.get_cluster_doc_ids(pack).await?;
            Some(ids.into_iter().collect::<HashSet<_>>())
        } else {
            None
        };

        let semantic = self
            .ctx
            .vector
            .search_semantic(&*self.ctx.embedder, &query, limit * 3)
            .await?;
        let keyword = self.ctx.ingest.search_keyword(&query, limit * 3).await?;

        let mut scores: HashMap<i64, f32> = HashMap::new();
        for (doc_id, score) in semantic {
            if let Some(filter) = &allowed_ids {
                if !filter.contains(&doc_id) {
                    continue;
                }
            }
            scores.insert(doc_id, score * 0.7);
        }
        for (rank, doc) in keyword.iter().enumerate() {
            if let Some(doc_id) = doc.id {
                if let Some(filter) = &allowed_ids {
                    if !filter.contains(&doc_id) {
                        continue;
                    }
                }
                let bonus = 0.3 * (1.0 / (rank as f32 + 1.0));
                *scores.entry(doc_id).or_insert(0.0) += bonus;
            }
        }

        // Apply ring boost if GraphStore is available (Gravity Well)
        if let Some(graph) = &self.ctx.graph {
            scores = self.apply_ring_boost(graph, scores).await;
        }

        let mut scored: Vec<(i64, f32)> = scores.into_iter().collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);

        let doc_ids: Vec<i64> = scored.iter().map(|(id, _)| *id).collect();
        let docs = self.ctx.ingest.fetch_documents_by_ids(&doc_ids).await?;
        let doc_map: HashMap<i64, Document> = docs
            .into_iter()
            .filter_map(|doc| doc.id.map(|id| (id, doc)))
            .collect();

        let items = scored
            .into_iter()
            .filter_map(|(id, score)| doc_map.get(&id).map(|doc| (doc.clone(), score)))
            .map(|(doc, score)| ContextItem {
                uri: doc.uri,
                source: doc.source,
                title: doc.title,
                score,
                snippet: truncate(&doc.body, 240),
                content: None,
            })
            .collect::<Vec<_>>();

        Ok(ContextResult {
            mode: "hybrid".to_string(),
            items,
        })
    }

    async fn handle_read_resource(&self, params: serde_json::Value) -> Result<ResourceResult> {
        let params: ReadResourceParams = serde_json::from_value(params)
            .map_err(|_| anyhow!("invalid read_resource params"))?;
        if let Some(doc) = self.ctx.ingest.get_document_by_uri(&params.uri).await? {
            return Ok(ResourceResult {
                uri: doc.uri,
                source: doc.source,
                body: doc.body,
            });
        }
        if let Some(sync) = self.router.fetch_url(&params.uri).await? {
            return Ok(ResourceResult {
                uri: sync.url,
                source: sync.source,
                body: sync.markdown,
            });
        }
        Err(anyhow!("resource not found"))
    }

    /// Apply ring-based boost to search scores.
    ///
    /// Documents associated with closer ring assignments get higher scores:
    /// - Core: 1.5x boost
    /// - Ring 1: 1.3x boost
    /// - Ring 2: 1.1x boost
    /// - Beyond: 1.0x (no boost)
    async fn apply_ring_boost(
        &self,
        graph: &GraphStore,
        mut scores: HashMap<i64, f32>,
    ) -> HashMap<i64, f32> {
        // Fetch documents to get their URIs
        let doc_ids: Vec<i64> = scores.keys().copied().collect();
        let docs = match self.ctx.ingest.fetch_documents_by_ids(&doc_ids).await {
            Ok(docs) => docs,
            Err(_) => return scores, // Fall back to unboosted scores
        };

        for doc in docs {
            let Some(doc_id) = doc.id else { continue };
            let Some(score) = scores.get_mut(&doc_id) else { continue };

            // Try to find a ring assignment for this document's entity
            // Construct potential node IDs from the document
            let node_ids = extract_node_ids_from_doc(&doc);

            let mut best_boost = 1.0f32;
            for node_id in node_ids {
                if let Ok(Some(assignment)) = graph.get_ring_assignment(&node_id).await {
                    let boost = ring_boost(assignment.ring);
                    if boost > best_boost {
                        best_boost = boost;
                    }
                }
            }

            if best_boost > 1.0 {
                debug!(
                    "Ring boost applied: doc_id={}, uri={}, boost={}",
                    doc_id, doc.uri, best_boost
                );
                *score *= best_boost;
            }
        }

        scores
    }
}

/// Get the boost multiplier for a ring.
fn ring_boost(ring: Ring) -> f32 {
    match ring {
        Ring::Core => 1.5,
        Ring::One => 1.3,
        Ring::Two => 1.1,
        Ring::Beyond => 1.0,
    }
}

/// Extract potential graph node IDs from a document.
///
/// Attempts to construct node IDs based on the document's source and URI.
fn extract_node_ids_from_doc(doc: &Document) -> Vec<String> {
    let mut ids = Vec::new();
    let source = &doc.source;
    let uri = &doc.uri;

    // Try to extract entity type and ID from URI patterns
    // Linear: linear://issue/ISSUE-123 -> issue:linear:ISSUE-123
    if source == "linear" {
        if let Some(id) = uri.strip_prefix("linear://issue/") {
            ids.push(format!("issue:linear:{}", id));
        }
    }

    // GitHub: github://org/repo/pr/123 -> pr:github:org/repo/123
    if source == "github" {
        if uri.contains("/pr/") || uri.contains("/pull/") {
            // Extract PR number from URI
            if let Some(num) = uri.split('/').next_back() {
                ids.push(format!("pr:github:{}", num));
            }
        } else if uri.contains("/issues/") {
            if let Some(num) = uri.split('/').next_back() {
                ids.push(format!("issue:github:{}", num));
            }
        }
    }

    // Slack: slack://C123/1234567890.123456 -> message:slack:1234567890.123456
    if source == "slack" {
        if let Some(ts) = uri.split('/').next_back() {
            if ts.contains('.') {
                ids.push(format!("message:slack:{}", ts));
            }
        }
    }

    // Notion: notion://page/xxx -> document:notion:xxx
    if source == "notion" {
        if let Some(id) = uri.strip_prefix("notion://page/") {
            ids.push(format!("document:notion:{}", id));
        }
    }

    // Google: google://doc/xxx -> document:google:xxx
    if source == "google" || source == "google_drive" {
        if let Some(id) = uri.strip_prefix("google://doc/") {
            ids.push(format!("document:google:{}", id));
        }
    }

    ids
}

fn parse_get_context_params(params: serde_json::Value) -> Result<GetContextParams> {
    if let Ok(parsed) = serde_json::from_value::<GetContextParams>(params.clone()) {
        return Ok(parsed);
    }
    if let Some(query) = params.as_str() {
        return Ok(GetContextParams {
            query: query.to_string(),
            pack: None,
            limit: None,
        });
    }
    Err(anyhow!("invalid get_context params"))
}

fn truncate(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut truncated = text.chars().take(max).collect::<String>();
    truncated.push_str("...");
    truncated
}

fn extract_pack(query: &str) -> (String, Option<String>) {
    let re = Regex::new(r#"pack\s*=\s*['"]?([^'"\s]+)['"]?"#).unwrap();
    if let Some(caps) = re.captures(query) {
        let pack = caps.get(1).map(|m| m.as_str().to_string());
        let cleaned = re.replace(query, "").to_string();
        return (cleaned.trim().to_string(), pack);
    }
    (query.to_string(), None)
}

#[derive(Debug, Clone)]
pub struct SyncContent {
    pub url: String,
    pub source: String,
    pub title: Option<String>,
    pub markdown: String,
}

#[derive(Debug, Clone)]
enum UrlKind {
    GithubPr { owner: String, repo: String, number: i64 },
    SlackThread { channel: String, ts: String },
    LinearIssue { identifier: String },
}

#[derive(Clone)]
pub struct UrlInterceptor {
    github: Regex,
    slack: Regex,
    linear: Regex,
}

impl Default for UrlInterceptor {
    fn default() -> Self {
        Self::new()
    }
}

impl UrlInterceptor {
    pub fn new() -> Self {
        Self {
            github: Regex::new(r"(?:https?://)?github\.com/([\w-]+)/([\w-]+)/pull/(\d+)")
                .unwrap(),
            slack: Regex::new(r"(?:https?://)?[\w\.-]*slack\.com/archives/([A-Z0-9]+)/p(\d+)")
                .unwrap(),
            linear: Regex::new(r"(?:https?://)?linear\.app/[\w-]+/issue/([\w-]+-\d+)")
                .unwrap(),
        }
    }

    fn detect(&self, text: &str) -> Vec<(String, UrlKind)> {
        let mut matches = Vec::new();
        for cap in self.github.captures_iter(text) {
            let owner = cap.get(1).unwrap().as_str().to_string();
            let repo = cap.get(2).unwrap().as_str().to_string();
            let number = cap.get(3).unwrap().as_str().parse::<i64>().unwrap_or(0);
            let url = cap.get(0).unwrap().as_str().to_string();
            matches.push((
                url,
                UrlKind::GithubPr {
                    owner,
                    repo,
                    number,
                },
            ));
        }
        for cap in self.slack.captures_iter(text) {
            let channel = cap.get(1).unwrap().as_str().to_string();
            let ts_raw = cap.get(2).unwrap().as_str();
            let ts = slack_ts(ts_raw);
            let url = cap.get(0).unwrap().as_str().to_string();
            matches.push((url, UrlKind::SlackThread { channel, ts }));
        }
        for cap in self.linear.captures_iter(text) {
            let identifier = cap.get(1).unwrap().as_str().to_string();
            let url = cap.get(0).unwrap().as_str().to_string();
            matches.push((url, UrlKind::LinearIssue { identifier }));
        }
        matches
    }
}

#[derive(Clone)]
pub struct SynchronousRouter {
    interceptor: UrlInterceptor,
    auth_store: Arc<RwLock<TokenStore>>,
    client: reqwest::Client,
}

impl SynchronousRouter {
    pub fn new(auth_store: Arc<RwLock<TokenStore>>) -> Self {
        Self {
            interceptor: UrlInterceptor::new(),
            auth_store,
            client: reqwest::Client::new(),
        }
    }

    pub async fn try_sync(&self, prompt: &str) -> Result<Option<SyncContent>> {
        let matches = self.interceptor.detect(prompt);
        if matches.is_empty() {
            return Ok(None);
        }
        let (url, kind) = matches[0].clone();
        self.fetch(kind, &url).await.map(Some)
    }

    pub async fn fetch_url(&self, url: &str) -> Result<Option<SyncContent>> {
        let matches = self.interceptor.detect(url);
        if matches.is_empty() {
            return Ok(None);
        }
        let (matched_url, kind) = matches[0].clone();
        self.fetch(kind, &matched_url).await.map(Some)
    }

    async fn fetch(&self, kind: UrlKind, url: &str) -> Result<SyncContent> {
        match kind {
            UrlKind::GithubPr { owner, repo, number } => {
                let token = self.get_token(Provider::Github).await?;
                let markdown = self.fetch_github_pr(&token, &owner, &repo, number).await?;
                Ok(SyncContent {
                    url: url.to_string(),
                    source: "github".to_string(),
                    title: Some(format!("{}/{} PR #{}", owner, repo, number)),
                    markdown,
                })
            }
            UrlKind::SlackThread { channel, ts } => {
                let token = self.get_token(Provider::Slack).await?;
                let markdown = self.fetch_slack_thread(&token, &channel, &ts).await?;
                Ok(SyncContent {
                    url: url.to_string(),
                    source: "slack".to_string(),
                    title: Some(format!("Slack thread {}", channel)),
                    markdown,
                })
            }
            UrlKind::LinearIssue { identifier } => {
                let token = self.get_token(Provider::Linear).await?;
                let markdown = self.fetch_linear_issue(&token, &identifier).await?;
                Ok(SyncContent {
                    url: url.to_string(),
                    source: "linear".to_string(),
                    title: Some(format!("Linear issue {}", identifier)),
                    markdown,
                })
            }
        }
    }

    async fn get_token(&self, provider: Provider) -> Result<String> {
        {
            let store = self.auth_store.read().await;
            if let Some(token) = store.get(provider) {
                return Ok(token.access_token);
            }
        }

        let mut store = self.auth_store.write().await;
        let _ = store.reload();
        store
            .get(provider)
            .map(|token| token.access_token)
            .ok_or_else(|| anyhow!("missing {} token", provider.as_str()))
    }

    async fn fetch_github_pr(
        &self,
        token: &str,
        owner: &str,
        repo: &str,
        number: i64,
    ) -> Result<String> {
        let url = format!(
            "https://api.github.com/repos/{}/{}/pulls/{}",
            owner, repo, number
        );
        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("token {}", token))
            .header("User-Agent", "minna-core")
            .send()
            .await?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("github fetch failed: {} - {}", status, body));
        }
        let payload: serde_json::Value = response.json().await?;
        let title = payload.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let state = payload.get("state").and_then(|v| v.as_str()).unwrap_or("");
        let body = payload.get("body").and_then(|v| v.as_str()).unwrap_or("");
        let user = payload
            .get("user")
            .and_then(|u| u.get("login"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let additions = payload.get("additions").and_then(|v| v.as_i64()).unwrap_or(0);
        let deletions = payload.get("deletions").and_then(|v| v.as_i64()).unwrap_or(0);
        let changed_files = payload
            .get("changed_files")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let html_url = payload.get("html_url").and_then(|v| v.as_str()).unwrap_or("");

        Ok(format!(
            "# {}\n\n- State: {}\n- Author: {}\n- Changes: +{} / -{} across {} files\n- URL: {}\n\n## Description\n{}",
            title, state, user, additions, deletions, changed_files, html_url, body
        ))
    }

    async fn fetch_slack_thread(&self, token: &str, channel: &str, ts: &str) -> Result<String> {
        let url = "https://slack.com/api/conversations.replies";
        let response = self
            .client
            .get(url)
            .header("Authorization", format!("Bearer {}", token))
            .query(&[("channel", channel), ("ts", ts)])
            .send()
            .await?;
        let payload: serde_json::Value = response.json().await?;
        let ok = payload.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
        if !ok {
            let err = payload
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            return Err(anyhow!("slack fetch failed: {}", err));
        }
        let mut out = String::from("# Slack Thread\n\n");
        if let Some(messages) = payload.get("messages").and_then(|v| v.as_array()) {
            for msg in messages {
                let user = msg.get("user").and_then(|v| v.as_str()).unwrap_or("unknown");
                let text = msg.get("text").and_then(|v| v.as_str()).unwrap_or("");
                let ts = msg.get("ts").and_then(|v| v.as_str()).unwrap_or("");
                out.push_str(&format!("- [{}] {}: {}\n", ts, user, text));
            }
        }
        Ok(out)
    }

    async fn fetch_linear_issue(&self, token: &str, identifier: &str) -> Result<String> {
        let url = "https://api.linear.app/graphql";
        let query = r#"
            query IssueByIdentifier($identifier: String!) {
                issues(filter: { identifier: { eq: $identifier } }) {
                    nodes { id title description state { name } assignee { name } url }
                }
            }
        "#;
        let payload = serde_json::json!({
            "query": query,
            "variables": { "identifier": identifier }
        });
        let response = self
            .client
            .post(url)
            .header("Authorization", token)
            .json(&payload)
            .send()
            .await?;
        let body: serde_json::Value = response.json().await?;
        let nodes = body
            .get("data")
            .and_then(|d| d.get("issues"))
            .and_then(|i| i.get("nodes"))
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        if nodes.is_empty() {
            return Err(anyhow!("linear issue not found"));
        }
        let issue = &nodes[0];
        let title = issue.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let description = issue
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let state = issue
            .get("state")
            .and_then(|v| v.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let assignee = issue
            .get("assignee")
            .and_then(|v| v.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("Unassigned");
        let url = issue.get("url").and_then(|v| v.as_str()).unwrap_or("");

        Ok(format!(
            "# {}\n\n- State: {}\n- Assignee: {}\n- URL: {}\n\n## Description\n{}",
            title, state, assignee, url, description
        ))
    }
}

fn slack_ts(raw: &str) -> String {
    if raw.len() <= 10 {
        return format!("{}.0000", raw);
    }
    let (secs, frac) = raw.split_at(10);
    format!("{}.{}", secs, frac)
}
