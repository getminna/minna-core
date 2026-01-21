# **Minna Engineering Blueprint: Gravity Well Extension (v22.0)**

**Code Name:** The Gravity Well
**Status:** DRAFT — Pending Review
**Extends:** Blueprint v21.0 (The Context Engine)
**Author:** Claude
**Date:** January 21, 2026

---

## **1. Executive Summary**

This extension adds a **relationship graph** to Minna, enabling proximity-aware sync and retrieval. Instead of treating all synced data equally, Minna learns who and what you work with closely, and prioritizes accordingly.

**Key Addition:** A graph layer built from structured metadata extraction (no SLM required).

**Core Concept:** You are the center of gravity. Objects closer to you in the collaboration graph sync more frequently and rank higher in search results.

---

## **2. Design Principles**

1. **Structured over inferred**: Extract relationships from explicit fields and @mentions, not prose analysis
2. **Incremental enhancement**: Graph layer is additive; existing sync/search continues working
3. **Deterministic**: Same input data → same graph. Testable, debuggable.
4. **Provider-owned extraction**: Each provider knows its schema; extraction logic lives there
5. **Score later**: Build the graph first, add weighted scoring as a separate phase

---

## **3. Architecture Overview**

### **3.1 Extended Data Flow**

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         PROVIDER SYNC                                    │
│                                                                          │
│   API Response ──► Provider.sync() ──┬──► store_document()              │
│                                      │                                   │
│                                      └──► emit_edges()  ◄── NEW         │
│                                            (structured extraction)       │
└─────────────────────────────────────────────────────────────────────────┘
                                   │
                   ┌───────────────┼───────────────┐
                   ▼               ▼               ▼
            ┌──────────┐    ┌──────────┐    ┌──────────┐
            │ SQLite   │    │ sqlite-  │    │ SQLite   │
            │ FTS5     │    │ vec      │    │ Graph    │  ◄── NEW
            │ (docs)   │    │ (embed)  │    │ (edges)  │
            └──────────┘    └──────────┘    └──────────┘
                   │               │               │
                   └───────────────┼───────────────┘
                                   ▼
                          ┌──────────────┐
                          │ Ring Engine  │  ◄── NEW
                          │ (BFS + rank) │
                          └──────────────┘
                                   │
                                   ▼
                          ┌──────────────┐
                          │ MCP Search   │
                          │ (ring-aware) │
                          └──────────────┘
```

### **3.2 New Components**

| Component | Location | Responsibility |
|-----------|----------|----------------|
| `minna-graph` | `engine/crates/minna-graph/` | Graph storage, traversal, ring calculation |
| `EdgeExtractor` | Per-provider in `minna-core/providers/` | Structured relationship extraction |
| `RingEngine` | `minna-graph/src/rings.rs` | BFS ring assignment, caching |
| `GravityScheduler` | `minna-core/src/scheduler.rs` | Ring-aware sync prioritization |

---

## **4. Graph Schema**

### **4.1 Node Types**

```rust
pub enum NodeType {
    User,       // A person (you, collaborators)
    Issue,      // Linear, Jira, GitHub issue
    Project,    // Linear project, Jira project, GitHub repo
    Document,   // Notion page, Confluence page, Google Doc
    Channel,    // Slack channel, Discord channel
    Message,    // Slack message, Discord message
    PullRequest,// GitHub PR
    Thread,     // Slack thread, email thread
}
```

### **4.2 Relation Types**

```rust
pub enum Relation {
    // User ↔ Object
    AssignedTo,       // User is assigned to Issue/PR
    AuthorOf,         // User authored Document/Message/Issue
    MentionedIn,      // User @mentioned in Object
    ReviewerOf,       // User is reviewer on PR

    // User ↔ Container
    MemberOf,         // User is member of Channel/Project

    // Object ↔ Container
    BelongsTo,        // Issue belongs to Project
    PostedIn,         // Message posted in Channel

    // Object ↔ Object
    ChildOf,          // Page is child of Page
    DependsOn,        // Issue depends on Issue
    Blocks,           // Issue blocks Issue
    References,       // Document references Document
    ThreadOf,         // Message is reply in Thread
}
```

### **4.3 Database Schema**

```sql
-- Nodes table
CREATE TABLE graph_nodes (
    id TEXT PRIMARY KEY,              -- Canonical ID: "user:slack:U123"
    node_type TEXT NOT NULL,          -- "user", "issue", "project", etc.
    provider TEXT NOT NULL,           -- "slack", "linear", "github"
    external_id TEXT NOT NULL,        -- Provider's native ID
    display_name TEXT,                -- Human-readable name
    metadata JSON,                    -- Provider-specific metadata
    first_seen_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_seen_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,

    UNIQUE(provider, external_id)
);

-- Edges table
CREATE TABLE graph_edges (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    from_node TEXT NOT NULL REFERENCES graph_nodes(id),
    to_node TEXT NOT NULL REFERENCES graph_nodes(id),
    relation TEXT NOT NULL,           -- "assigned_to", "mentioned_in", etc.
    provider TEXT NOT NULL,           -- Source provider
    observed_at TIMESTAMP NOT NULL,   -- When this edge was observed
    metadata JSON,                    -- Optional: message_id, context

    UNIQUE(from_node, to_node, relation, provider)
);

-- Indexes for traversal
CREATE INDEX idx_edges_from ON graph_edges(from_node);
CREATE INDEX idx_edges_to ON graph_edges(to_node);
CREATE INDEX idx_edges_relation ON graph_edges(relation);
CREATE INDEX idx_edges_observed ON graph_edges(observed_at);

-- Ring cache (materialized, recomputed periodically)
CREATE TABLE ring_assignments (
    node_id TEXT PRIMARY KEY REFERENCES graph_nodes(id),
    ring INTEGER NOT NULL,            -- 0=Core, 1=Ring1, 2=Ring2, 3=Beyond
    distance INTEGER NOT NULL,        -- Graph distance from user
    computed_at TIMESTAMP NOT NULL,
    path JSON                         -- Optional: shortest path to user
);
```

### **4.4 Identity Resolution**

Same person across providers needs linking:

```sql
-- User identity mapping
CREATE TABLE user_identities (
    canonical_id TEXT PRIMARY KEY,    -- "user:canonical:alice@company.com"
    email TEXT UNIQUE,                -- Primary identifier
    display_name TEXT
);

CREATE TABLE user_identity_links (
    canonical_id TEXT REFERENCES user_identities(canonical_id),
    provider TEXT NOT NULL,
    provider_user_id TEXT NOT NULL,   -- "U123" for Slack, etc.
    PRIMARY KEY (provider, provider_user_id)
);
```

---

## **5. Structured Extraction**

### **5.1 Extractor Trait**

```rust
pub struct ExtractedEdge {
    pub from: NodeRef,
    pub to: NodeRef,
    pub relation: Relation,
    pub observed_at: DateTime<Utc>,
    pub metadata: Option<serde_json::Value>,
}

pub struct NodeRef {
    pub node_type: NodeType,
    pub provider: &'static str,
    pub external_id: String,
    pub display_name: Option<String>,
}

pub trait EdgeExtractor {
    fn extract_edges(&self, raw: &serde_json::Value) -> Vec<ExtractedEdge>;
}
```

### **5.2 Provider-Specific Extraction**

#### **Slack**

```rust
impl EdgeExtractor for SlackProvider {
    fn extract_edges(&self, message: &serde_json::Value) -> Vec<ExtractedEdge> {
        let mut edges = vec![];
        let author = message["user"].as_str().unwrap();
        let channel = message["channel"].as_str().unwrap();
        let ts = message["ts"].as_str().unwrap();
        let text = message["text"].as_str().unwrap_or("");

        // Author → Message
        edges.push(ExtractedEdge {
            from: NodeRef::user("slack", author),
            to: NodeRef::message("slack", ts),
            relation: Relation::AuthorOf,
            observed_at: parse_slack_ts(ts),
            metadata: None,
        });

        // Message → Channel
        edges.push(ExtractedEdge {
            from: NodeRef::message("slack", ts),
            to: NodeRef::channel("slack", channel),
            relation: Relation::PostedIn,
            observed_at: parse_slack_ts(ts),
            metadata: None,
        });

        // @mentions: <@U1234567890>
        for cap in SLACK_MENTION_RE.captures_iter(text) {
            let mentioned_user = &cap[1];
            edges.push(ExtractedEdge {
                from: NodeRef::user("slack", mentioned_user),
                to: NodeRef::message("slack", ts),
                relation: Relation::MentionedIn,
                observed_at: parse_slack_ts(ts),
                metadata: None,
            });
        }

        // Thread parent
        if let Some(thread_ts) = message.get("thread_ts") {
            edges.push(ExtractedEdge {
                from: NodeRef::message("slack", ts),
                to: NodeRef::thread("slack", thread_ts.as_str().unwrap()),
                relation: Relation::ThreadOf,
                observed_at: parse_slack_ts(ts),
                metadata: None,
            });
        }

        edges
    }
}

lazy_static! {
    static ref SLACK_MENTION_RE: Regex = Regex::new(r"<@(U[A-Z0-9]+)>").unwrap();
}
```

#### **Linear**

```rust
impl EdgeExtractor for LinearProvider {
    fn extract_edges(&self, issue: &serde_json::Value) -> Vec<ExtractedEdge> {
        let mut edges = vec![];
        let issue_id = issue["id"].as_str().unwrap();
        let updated_at = parse_iso8601(issue["updatedAt"].as_str().unwrap());

        // Assignee → Issue
        if let Some(assignee) = issue.get("assignee") {
            edges.push(ExtractedEdge {
                from: NodeRef::user("linear", assignee["id"].as_str().unwrap()),
                to: NodeRef::issue("linear", issue_id),
                relation: Relation::AssignedTo,
                observed_at: updated_at,
                metadata: None,
            });
        }

        // Creator → Issue
        if let Some(creator) = issue.get("creator") {
            edges.push(ExtractedEdge {
                from: NodeRef::user("linear", creator["id"].as_str().unwrap()),
                to: NodeRef::issue("linear", issue_id),
                relation: Relation::AuthorOf,
                observed_at: updated_at,
                metadata: None,
            });
        }

        // Issue → Project
        if let Some(project) = issue.get("project") {
            edges.push(ExtractedEdge {
                from: NodeRef::issue("linear", issue_id),
                to: NodeRef::project("linear", project["id"].as_str().unwrap()),
                relation: Relation::BelongsTo,
                observed_at: updated_at,
                metadata: None,
            });
        }

        // Issue dependencies (relations)
        if let Some(relations) = issue.get("relations").and_then(|r| r.as_array()) {
            for rel in relations {
                let rel_type = rel["type"].as_str().unwrap();
                let related_id = rel["relatedIssue"]["id"].as_str().unwrap();

                let relation = match rel_type {
                    "blocks" => Relation::Blocks,
                    "duplicate" => Relation::References,
                    "related" => Relation::References,
                    _ => continue,
                };

                edges.push(ExtractedEdge {
                    from: NodeRef::issue("linear", issue_id),
                    to: NodeRef::issue("linear", related_id),
                    relation,
                    observed_at: updated_at,
                    metadata: None,
                });
            }
        }

        edges
    }
}
```

#### **GitHub**

```rust
impl EdgeExtractor for GitHubProvider {
    fn extract_edges(&self, issue_or_pr: &serde_json::Value) -> Vec<ExtractedEdge> {
        let mut edges = vec![];
        let id = issue_or_pr["node_id"].as_str().unwrap();
        let is_pr = issue_or_pr.get("pull_request").is_some();
        let updated_at = parse_iso8601(issue_or_pr["updated_at"].as_str().unwrap());

        let node_ref = if is_pr {
            NodeRef::pull_request("github", id)
        } else {
            NodeRef::issue("github", id)
        };

        // Author
        if let Some(user) = issue_or_pr.get("user") {
            edges.push(ExtractedEdge {
                from: NodeRef::user("github", user["login"].as_str().unwrap()),
                to: node_ref.clone(),
                relation: Relation::AuthorOf,
                observed_at: updated_at,
                metadata: None,
            });
        }

        // Assignees
        if let Some(assignees) = issue_or_pr.get("assignees").and_then(|a| a.as_array()) {
            for assignee in assignees {
                edges.push(ExtractedEdge {
                    from: NodeRef::user("github", assignee["login"].as_str().unwrap()),
                    to: node_ref.clone(),
                    relation: Relation::AssignedTo,
                    observed_at: updated_at,
                    metadata: None,
                });
            }
        }

        // Requested reviewers (PRs only)
        if let Some(reviewers) = issue_or_pr.get("requested_reviewers").and_then(|r| r.as_array()) {
            for reviewer in reviewers {
                edges.push(ExtractedEdge {
                    from: NodeRef::user("github", reviewer["login"].as_str().unwrap()),
                    to: node_ref.clone(),
                    relation: Relation::ReviewerOf,
                    observed_at: updated_at,
                    metadata: None,
                });
            }
        }

        // Repository (Project equivalent)
        let repo = issue_or_pr["repository_url"].as_str().unwrap();
        let repo_name = repo.split('/').last().unwrap();
        edges.push(ExtractedEdge {
            from: node_ref.clone(),
            to: NodeRef::project("github", repo_name),
            relation: Relation::BelongsTo,
            observed_at: updated_at,
            metadata: None,
        });

        // @mentions in body
        if let Some(body) = issue_or_pr.get("body").and_then(|b| b.as_str()) {
            for cap in GITHUB_MENTION_RE.captures_iter(body) {
                edges.push(ExtractedEdge {
                    from: NodeRef::user("github", &cap[1]),
                    to: node_ref.clone(),
                    relation: Relation::MentionedIn,
                    observed_at: updated_at,
                    metadata: None,
                });
            }
        }

        edges
    }
}

lazy_static! {
    static ref GITHUB_MENTION_RE: Regex = Regex::new(r"@([a-zA-Z0-9_-]+)").unwrap();
}
```

### **5.3 Extraction Summary by Provider**

| Provider | Structured Fields | @Mention Extraction |
|----------|-------------------|---------------------|
| **Slack** | author, channel, thread_ts | `<@U...>` regex |
| **Linear** | assignee, creator, project, relations | API returns mentions |
| **GitHub** | author, assignees, reviewers, repo | `@username` regex |
| **Notion** | author, parent, mentions[] | API returns mentions |
| **Jira** | assignee, reporter, project, issuelinks | `[~accountId]` regex |
| **Confluence** | author, space, parent | API returns mentions |
| **Google Drive** | owner, sharedWith[] | N/A (no @mentions) |

---

## **6. Ring Engine**

### **6.1 Ring Definitions**

| Ring | Distance | Sync Behavior | Search Boost |
|------|----------|---------------|--------------|
| **Core** | 0 | — | — |
| **Ring 1** | 1 | Full sync, hourly | 2.0x |
| **Ring 2** | 2 | Partial sync, daily | 1.5x |
| **Beyond** | 3+ | On-demand only | 1.0x |

**Core** is you. Your messages, your issues, your documents.
**Ring 1** is direct: people assigned to your issues, people who @mention you, your projects.
**Ring 2** is one hop away: people assigned to issues in your projects, members of your channels.

### **6.2 Ring Calculation**

```rust
pub struct RingEngine {
    graph: Graph,
    user_node: NodeId,
}

impl RingEngine {
    pub fn compute_rings(&self) -> HashMap<NodeId, RingAssignment> {
        let mut assignments = HashMap::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        // Start BFS from user
        queue.push_back((self.user_node.clone(), 0, vec![self.user_node.clone()]));

        while let Some((node, distance, path)) = queue.pop_front() {
            if visited.contains(&node) {
                continue;
            }
            visited.insert(node.clone());

            let ring = match distance {
                0 => Ring::Core,
                1 => Ring::One,
                2 => Ring::Two,
                _ => Ring::Beyond,
            };

            assignments.insert(node.clone(), RingAssignment {
                ring,
                distance,
                path: path.clone(),
                computed_at: Utc::now(),
            });

            // Only traverse up to distance 3 (Beyond)
            if distance < 3 {
                for edge in self.graph.edges_from(&node) {
                    if !visited.contains(&edge.to) {
                        let mut new_path = path.clone();
                        new_path.push(edge.to.clone());
                        queue.push_back((edge.to.clone(), distance + 1, new_path));
                    }
                }

                // Also traverse reverse edges (bidirectional)
                for edge in self.graph.edges_to(&node) {
                    if !visited.contains(&edge.from) {
                        let mut new_path = path.clone();
                        new_path.push(edge.from.clone());
                        queue.push_back((edge.from.clone(), distance + 1, new_path));
                    }
                }
            }
        }

        assignments
    }

    pub fn get_ring(&self, node: &NodeId) -> Ring {
        self.assignments
            .get(node)
            .map(|a| a.ring)
            .unwrap_or(Ring::Beyond)
    }
}
```

### **6.3 Ring Cache Invalidation**

Rings are recomputed when:
1. **New edges added** — After each sync, if edge count changed
2. **Periodic refresh** — Every 6 hours regardless
3. **Manual trigger** — `minna gravity refresh`

```rust
impl RingEngine {
    pub fn should_recompute(&self, last_edge_count: usize, current_edge_count: usize) -> bool {
        let edge_delta = current_edge_count.abs_diff(last_edge_count);
        let hours_since_compute = (Utc::now() - self.last_computed_at).num_hours();

        edge_delta > 10 || hours_since_compute >= 6
    }
}
```

---

## **7. Delta Sync Mechanism**

### **7.1 Current State**

```rust
// Current: time-based, all-or-nothing
trait SyncProvider {
    async fn sync(&self, ctx: &SyncContext<'_>, since_days: Option<i64>, mode: Option<&str>) -> Result<SyncSummary>;
}
```

### **7.2 Extended Trait**

```rust
#[async_trait]
pub trait SyncProvider: Send + Sync {
    // Existing
    fn name(&self) -> &'static str;
    async fn sync(&self, ctx: &SyncContext<'_>, since_days: Option<i64>, mode: Option<&str>) -> Result<SyncSummary>;

    // New: Structured extraction
    fn extract_edges(&self, raw: &serde_json::Value) -> Vec<ExtractedEdge>;

    // New: Delta support (optional, providers can opt-in)
    fn supports_cursor_sync(&self) -> bool { false }
    async fn sync_with_cursor(&self, ctx: &SyncContext<'_>, cursor: Option<&str>) -> Result<CursorSyncResult> {
        Err(Error::NotSupported)
    }

    // New: Depth-aware sync (optional)
    fn supports_partial_sync(&self) -> bool { false }
    async fn sync_partial(&self, ctx: &SyncContext<'_>, object_ids: &[&str], fields: &[&str]) -> Result<SyncSummary> {
        Err(Error::NotSupported)
    }
}

pub struct CursorSyncResult {
    pub documents: Vec<Document>,
    pub edges: Vec<ExtractedEdge>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}
```

### **7.3 Ring-Aware Sync Scheduling**

```rust
pub struct GravityScheduler {
    ring_engine: RingEngine,
    provider_registry: ProviderRegistry,
}

impl GravityScheduler {
    pub async fn run_sync_cycle(&self) -> Result<SyncReport> {
        let mut report = SyncReport::default();

        // Phase 1: Ring 1 (full sync, always)
        let ring1_nodes = self.ring_engine.nodes_in_ring(Ring::One);
        for provider in self.provider_registry.enabled_providers() {
            let ring1_objects = self.get_objects_for_nodes(&ring1_nodes, provider.name());
            report.merge(provider.sync_objects(&ring1_objects, SyncDepth::Full).await?);
        }

        // Phase 2: Ring 2 (if time budget allows)
        if report.time_elapsed < self.time_budget * 0.7 {
            let ring2_nodes = self.ring_engine.nodes_in_ring(Ring::Two);
            for provider in self.provider_registry.enabled_providers() {
                if provider.supports_partial_sync() {
                    let ring2_objects = self.get_objects_for_nodes(&ring2_nodes, provider.name());
                    report.merge(provider.sync_partial(&ring2_objects, &["status", "updated_at"]).await?);
                }
            }
        }

        // Phase 3: Beyond (only if explicitly queried recently)
        let recently_queried_beyond = self.get_recently_queried_beyond_objects();
        if !recently_queried_beyond.is_empty() {
            for provider in self.provider_registry.enabled_providers() {
                report.merge(provider.sync_objects(&recently_queried_beyond, SyncDepth::Full).await?);
            }
        }

        Ok(report)
    }
}
```

### **7.4 Sync Frequency by Ring**

| Ring | Sync Frequency | Sync Depth | Rationale |
|------|----------------|------------|-----------|
| Ring 1 | Every hour | Full | Your direct collaborators; need fresh data |
| Ring 2 | Every 6 hours | Partial (metadata) | One hop away; staleness acceptable |
| Beyond | On query only | Full (single object) | Too much data to proactively sync |

---

## **8. Search Integration**

### **8.1 Ring-Boosted Hybrid Search**

```rust
impl SearchEngine {
    pub async fn search(&self, query: &str, user_id: &str) -> Result<Vec<SearchResult>> {
        // Existing: hybrid search (vector + FTS5)
        let raw_results = self.hybrid_search(query).await?;

        // New: ring-aware reranking
        let ring_engine = self.get_ring_engine(user_id);

        let mut scored_results: Vec<_> = raw_results
            .into_iter()
            .map(|r| {
                let ring = ring_engine.get_ring_for_document(&r.doc_id);
                let ring_boost = match ring {
                    Ring::Core => 3.0,
                    Ring::One => 2.0,
                    Ring::Two => 1.5,
                    Ring::Beyond => 1.0,
                };

                ScoredResult {
                    result: r,
                    final_score: r.base_score * ring_boost,
                    ring,
                }
            })
            .collect();

        scored_results.sort_by(|a, b| b.final_score.partial_cmp(&a.final_score).unwrap());

        Ok(scored_results.into_iter().map(|s| s.result).collect())
    }
}
```

### **8.2 Ring Filtering**

```rust
// MCP tool: search with ring filter
pub async fn search(query: &str, ring: Option<Ring>) -> Result<Vec<SearchResult>> {
    let results = engine.search(query).await?;

    match ring {
        Some(r) => Ok(results.into_iter().filter(|res| res.ring == r).collect()),
        None => Ok(results),
    }
}
```

---

## **9. CLI Extensions**

### **9.1 New Commands**

| Command | Description |
|---------|-------------|
| `minna gravity status` | Show ring distribution, node/edge counts |
| `minna gravity show` | List Ring 1 and Ring 2 members |
| `minna gravity explain <entity>` | Show why entity is in its ring |
| `minna gravity refresh` | Force ring recomputation |
| `minna gravity pin <entity>` | Manually promote to Ring 1 |
| `minna gravity unpin <entity>` | Remove manual promotion |

### **9.2 Example Output**

```bash
$ minna gravity status

Gravity Well Status
───────────────────
User: alice@company.com

Nodes: 1,247 total
  • Users: 45
  • Issues: 312
  • Documents: 189
  • Messages: 701

Edges: 3,891 total
  • Last updated: 2 minutes ago

Ring Distribution:
  • Ring 1 (Direct):   23 users, 450 objects
  • Ring 2 (Extended): 18 users, 620 objects
  • Beyond:            4 users, 177 objects

Sync Schedule:
  • Ring 1: Every hour (next: 23 mins)
  • Ring 2: Every 6 hours (next: 4h 12m)
```

```bash
$ minna gravity explain jordan@company.com

Jordan Chen (jordan@company.com)
────────────────────────────────
Ring: 2 (Extended)
Distance: 2 hops

Path to you:
  You → sarah@company.com (47 interactions)
       → jordan@company.com (mentioned in 12 issues)

Why Ring 2:
  • Jordan is not directly assigned to your issues
  • Jordan has not @mentioned you
  • Jordan works on Project Atlas (which you also work on)
  • Connection is through Sarah (your Ring 1)

Jordan's objects synced: 34 issues (metadata only)
Last sync: 3 hours ago

[Pin to Ring 1] [View Objects]
```

---

## **10. Implementation Phases**

### **Phase 1: Graph Foundation**
*Goal: Graph exists and is accurate*

- [ ] Add `minna-graph` crate with schema
- [ ] Implement `EdgeExtractor` for Slack, Linear, GitHub
- [ ] Add edge emission to sync pipeline
- [ ] Create `graph_nodes` and `graph_edges` tables
- [ ] Add `minna gravity status` CLI command
- [ ] Add basic identity resolution (email-based)

**Validation:** Graph accurately reflects structured relationships. Spot-check 10 random edges.

### **Phase 2: Ring Assignment**
*Goal: Rings computed correctly from graph distance*

- [ ] Implement `RingEngine` with BFS traversal
- [ ] Add `ring_assignments` table with caching
- [ ] Add `minna gravity show` and `minna gravity explain`
- [ ] Add ring invalidation triggers

**Validation:** Your direct collaborators are Ring 1. One-hop-away are Ring 2.

### **Phase 3: Ring-Aware Search**
*Goal: Search results improve for Ring 1 content*

- [ ] Add ring boost to hybrid search scoring
- [ ] Add ring filter parameter to MCP search tool
- [ ] Add ring indicator to search results

**Validation:** A/B test search relevance with and without ring boosting.

### **Phase 4: Ring-Aware Sync**
*Goal: Reduce sync volume, prioritize Ring 1*

- [ ] Implement `GravityScheduler` with ring-based scheduling
- [ ] Add `supports_partial_sync()` to providers that support it
- [ ] Add cursor-based sync for Slack, Linear (providers that support it)
- [ ] Track sync time budget and prioritize accordingly

**Validation:** Total sync time decreases. Ring 1 freshness unchanged. Ring 2+ acceptable staleness.

### **Phase 5: User Controls**
*Goal: Users can correct the algorithm*

- [ ] Add `minna gravity pin/unpin` for manual overrides
- [ ] Store manual pins in config
- [ ] Show manual pins distinctly in `gravity show`

**Validation:** Pinned entities behave as Ring 1 regardless of graph distance.

---

## **11. Future Enhancements (Post-V1)**

These are explicitly **out of scope** for initial implementation but designed-for:

| Enhancement | Description | When to Add |
|-------------|-------------|-------------|
| **Weighted scoring** | Edge weights based on interaction type | After validating basic rings work |
| **Decay functions** | Edges lose weight over time | After collecting timestamp data |
| **Query learning** | Auto-promote frequently queried Beyond objects | After collecting query logs |
| **Project focus mode** | Temporarily elevate all project members | User request |
| **SLM extraction** | Extract relationships from prose | If structured extraction proves insufficient |

---

## **12. Open Questions**

1. **Identity resolution complexity**: Email-based linking is simple but incomplete. Some users have different emails across providers. How much effort to invest here?

2. **Ring 2 partial sync**: Not all providers support field-level filtering. For those that don't, do we skip Ring 2 or do full sync less frequently?

3. **Graph size limits**: At what point does BFS become too slow? Do we need to cap traversal depth or node count?

4. **Multi-user future**: This design is single-user (your gravity well). If we add team workspaces later, do we need shared graphs or per-user graphs with overlap?

---

## **13. Success Metrics**

| Metric | Current | Target | How to Measure |
|--------|---------|--------|----------------|
| Search relevance | Baseline | +20% | User feedback on top-3 results |
| Sync volume | 100% | -40% | Bytes transferred per day |
| Ring 1 freshness | N/A | <1 hour stale | Max age of Ring 1 documents |
| Query latency | Baseline | No regression | p50/p95 search latency |

---

## **14. Relationship to v21 Blueprint**

This extension is **additive**. All v21 components remain unchanged:

| v21 Component | Change |
|---------------|--------|
| `minna-core` | Add edge emission to sync pipeline |
| `minna-ingest` | No change |
| `minna-vector` | No change |
| `minna-mcp` | Add ring parameter to search tool |
| `minna-server` | No change |
| `minna-cli` | Add `gravity` subcommand |
| Providers | Add `EdgeExtractor` impl to each |

New crate: `minna-graph` (graph storage + ring engine)

---

*End of Blueprint v22.0 Draft*
