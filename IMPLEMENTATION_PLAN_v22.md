# Gravity Well Implementation Plan (Final)

**Project:** Minna Gravity Well (Blueprint v22)
**Status:** APPROVED
**Last Updated:** January 21, 2026
**Decision:** Migrate Linear first → if smooth, migrate all connectors

---

## Strategy: Linear-First Migration

Instead of adding hooks to legacy code, we migrate providers to the trait-based system with edge extraction built-in.

**Sequence:**
1. **Linear** (smallest, ~130 lines) → proof of concept
2. **If successful:** Slack (~617 lines) → GitHub (~145 lines)
3. **Defer:** Google (low graph value, complex)

**Why this order:**
- Linear is smallest, cleanest API (GraphQL)
- High graph signal (assignees, projects, issue links)
- If migration pattern works, apply to others
- Fail fast if trait system has issues

---

## Revised Sprint Plan

| Sprint | Duration | Focus | Providers |
|--------|----------|-------|-----------|
| **Sprint 1** | Weeks 1-2 | Graph schema + minna-graph crate | — |
| **Sprint 2** | Weeks 3-4 | Linear migration + edge extraction | Linear |
| **Sprint 3** | Weeks 5-6 | Slack + GitHub migration | Slack, GitHub |
| **Sprint 4** | Weeks 7-8 | RingEngine + LocalGit | LocalGit |
| **Sprint 5** | Weeks 9-10 | CLI + Search integration | — |
| **Sprint 6** | Weeks 11-12 | Sync scheduler | — |
| **Hardening** | Weeks 13-14 | Polish, testing, docs | — |

**Timeline increased by 2 weeks** due to full migration approach. Worth it for clean architecture.

---

## Sprint 1: Graph Foundation

**Goal:** Graph tables exist, minna-graph crate created, ready for extraction.

### File Changes

```
engine/
├── Cargo.toml                          # Add minna-graph to workspace
├── crates/
│   ├── minna-graph/                    # NEW CRATE
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                  # Public API
│   │       ├── schema.rs               # NodeType, Relation enums
│   │       ├── storage.rs              # Graph CRUD operations
│   │       └── identity.rs             # User identity linking
│   └── minna-ingest/
│       └── src/
│           └── lib.rs                  # ADD: graph tables to init_schema()
```

### Tasks

| ID | Task | File | Est |
|----|------|------|-----|
| 1.1 | Add `minna-graph` to workspace | `engine/Cargo.toml` | 1h |
| 1.2 | Create minna-graph crate scaffold | `minna-graph/Cargo.toml`, `lib.rs` | 2h |
| 1.3 | Define `NodeType`, `Relation` enums | `minna-graph/src/schema.rs` | 2h |
| 1.4 | Add `graph_nodes` table to minna-ingest | `minna-ingest/src/lib.rs` | 3h |
| 1.5 | Add `graph_edges` table (with `weight`) | `minna-ingest/src/lib.rs` | 3h |
| 1.6 | Add `user_identities` tables | `minna-ingest/src/lib.rs` | 2h |
| 1.7 | Add `ring_assignments` table | `minna-ingest/src/lib.rs` | 2h |
| 1.8 | Implement `GraphStore` with CRUD | `minna-graph/src/storage.rs` | 6h |
| 1.9 | Implement `edges_from()`, `edges_to()` | `minna-graph/src/storage.rs` | 4h |
| 1.10 | Define `ExtractedEdge`, `NodeRef` | `minna-graph/src/schema.rs` | 2h |
| 1.11 | Add `extract_edges()` to `SyncProvider` trait | `minna-core/src/providers/mod.rs` | 2h |
| 1.12 | Unit tests for graph operations | `minna-graph/tests/` | 6h |

### Schema Addition to minna-ingest

```rust
// In minna-ingest/src/lib.rs, add to init_schema():

// Graph nodes
sqlx::query(
    "CREATE TABLE IF NOT EXISTS graph_nodes (
        id TEXT PRIMARY KEY,
        node_type TEXT NOT NULL,
        provider TEXT NOT NULL,
        external_id TEXT NOT NULL,
        display_name TEXT,
        metadata JSON,
        first_seen_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        last_seen_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        UNIQUE(provider, external_id)
    )"
).execute(&self.pool).await?;

// Graph edges
sqlx::query(
    "CREATE TABLE IF NOT EXISTS graph_edges (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        from_node TEXT NOT NULL REFERENCES graph_nodes(id),
        to_node TEXT NOT NULL REFERENCES graph_nodes(id),
        relation TEXT NOT NULL,
        provider TEXT NOT NULL,
        observed_at TEXT NOT NULL,
        weight REAL NOT NULL DEFAULT 1.0,
        metadata JSON,
        UNIQUE(from_node, to_node, relation, provider)
    )"
).execute(&self.pool).await?;

// Indexes
sqlx::query("CREATE INDEX IF NOT EXISTS idx_edges_from ON graph_edges(from_node)").execute(&self.pool).await?;
sqlx::query("CREATE INDEX IF NOT EXISTS idx_edges_to ON graph_edges(to_node)").execute(&self.pool).await?;
sqlx::query("CREATE INDEX IF NOT EXISTS idx_edges_observed ON graph_edges(observed_at)").execute(&self.pool).await?;

// Identity linking
sqlx::query(
    "CREATE TABLE IF NOT EXISTS user_identities (
        canonical_id TEXT PRIMARY KEY,
        email TEXT UNIQUE,
        display_name TEXT
    )"
).execute(&self.pool).await?;

sqlx::query(
    "CREATE TABLE IF NOT EXISTS user_identity_links (
        canonical_id TEXT REFERENCES user_identities(canonical_id),
        provider TEXT NOT NULL,
        provider_user_id TEXT NOT NULL,
        PRIMARY KEY (provider, provider_user_id)
    )"
).execute(&self.pool).await?;

// Ring cache
sqlx::query(
    "CREATE TABLE IF NOT EXISTS ring_assignments (
        node_id TEXT PRIMARY KEY REFERENCES graph_nodes(id),
        ring INTEGER NOT NULL,
        distance INTEGER NOT NULL,
        effective_distance REAL NOT NULL,
        path JSON,
        computed_at TEXT NOT NULL
    )"
).execute(&self.pool).await?;
```

### SyncProvider Trait Extension

```rust
// In minna-core/src/providers/mod.rs

#[async_trait]
pub trait SyncProvider: Send + Sync {
    fn name(&self) -> &'static str;
    fn display_name(&self) -> &'static str;

    async fn sync(
        &self,
        ctx: &SyncContext<'_>,
        since_days: Option<i64>,
        mode: Option<&str>,
    ) -> Result<SyncSummary>;

    async fn discover(&self, ctx: &SyncContext<'_>) -> Result<serde_json::Value>;

    // NEW: Edge extraction (default no-op for backward compatibility)
    fn extract_edges(&self, _raw: &serde_json::Value) -> Vec<ExtractedEdge> {
        vec![]
    }
}
```

### Testing Criteria

```
□ minna-graph crate compiles
□ Graph tables created on fresh DB
□ Can insert 10,000 nodes in < 1s
□ Can insert 50,000 edges in < 5s
□ edges_from() returns correct edges
□ UNIQUE constraint prevents duplicates
□ SyncProvider trait compiles with new method
```

### Definition of Done

- [ ] `minna-graph` crate in workspace, compiles
- [ ] All tables migrate cleanly
- [ ] Unit tests pass (>80% coverage)
- [ ] `SyncProvider` trait has `extract_edges()` method
- [ ] Existing providers (Notion, Atlassian) still compile

---

## Sprint 2: Linear Migration

**Goal:** Migrate `sync_linear()` from legacy method to `SyncProvider` trait with edge extraction.

### Pre-Sprint: Understand Linear Legacy Code

```rust
// Current: minna-core/src/lib.rs:1016-1145 (~130 lines)
pub async fn sync_linear(&self, since_days: Option<i64>, mode: Option<&str>) -> Result<SyncSummary>
```

### File Changes

```
engine/crates/
├── minna-core/src/
│   ├── lib.rs                          # REMOVE sync_linear() method
│   └── providers/
│       ├── mod.rs                      # Register LinearProvider
│       └── linear.rs                   # NEW: LinearProvider implementation
├── minna-server/src/
│   └── main.rs                         # Update dispatch to use registry
```

### Tasks

| ID | Task | File | Est |
|----|------|------|-----|
| 2.1 | Create `linear.rs` provider file | `minna-core/src/providers/linear.rs` | 2h |
| 2.2 | Move Linear sync logic to `LinearProvider::sync()` | `providers/linear.rs` | 6h |
| 2.3 | Implement `extract_edges()` for Linear | `providers/linear.rs` | 4h |
| 2.4 | Extract: Assignee → Issue edges | `providers/linear.rs` | 2h |
| 2.5 | Extract: Creator → Issue edges | `providers/linear.rs` | 1h |
| 2.6 | Extract: Issue → Project edges | `providers/linear.rs` | 1h |
| 2.7 | Extract: Issue relations (blocks, depends) | `providers/linear.rs` | 2h |
| 2.8 | Register LinearProvider in registry | `providers/mod.rs` | 1h |
| 2.9 | Update daemon to use registry for Linear | `minna-server/src/main.rs` | 2h |
| 2.10 | Remove legacy `sync_linear()` from lib.rs | `minna-core/src/lib.rs` | 1h |
| 2.11 | Add `GraphStore` to `SyncContext` | `minna-core/src/providers/mod.rs` | 2h |
| 2.12 | Emit edges during sync | `providers/linear.rs` | 3h |
| 2.13 | Integration tests | `minna-core/tests/linear.rs` | 6h |
| 2.14 | Manual testing with real Linear account | Manual | 4h |

### LinearProvider Implementation

```rust
// minna-core/src/providers/linear.rs

use crate::providers::{SyncProvider, SyncContext, SyncSummary};
use minna_graph::{ExtractedEdge, NodeRef, Relation};

pub struct LinearProvider;

#[async_trait]
impl SyncProvider for LinearProvider {
    fn name(&self) -> &'static str { "linear" }
    fn display_name(&self) -> &'static str { "Linear" }

    async fn sync(
        &self,
        ctx: &SyncContext<'_>,
        since_days: Option<i64>,
        mode: Option<&str>,
    ) -> Result<SyncSummary> {
        // Move existing sync_linear() logic here
        let issues = self.fetch_issues(ctx, since_days).await?;

        for issue in &issues {
            // Store document (existing logic)
            ctx.ingest.store_document(&issue.to_document()).await?;

            // NEW: Extract and store edges
            let edges = self.extract_edges(&serde_json::to_value(issue)?);
            for edge in edges {
                ctx.graph.upsert_edge(&edge).await?;
            }
        }

        Ok(SyncSummary { ... })
    }

    fn extract_edges(&self, issue: &serde_json::Value) -> Vec<ExtractedEdge> {
        let mut edges = vec![];
        let issue_id = issue["id"].as_str().unwrap_or_default();
        let updated_at = parse_timestamp(issue["updatedAt"].as_str());

        // Assignee → Issue
        if let Some(assignee) = issue.get("assignee").and_then(|a| a.get("id")) {
            edges.push(ExtractedEdge {
                from: NodeRef::user("linear", assignee.as_str().unwrap()),
                to: NodeRef::issue("linear", issue_id),
                relation: Relation::AssignedTo,
                observed_at: updated_at,
                metadata: None,
            });
        }

        // Creator → Issue
        if let Some(creator) = issue.get("creator").and_then(|c| c.get("id")) {
            edges.push(ExtractedEdge {
                from: NodeRef::user("linear", creator.as_str().unwrap()),
                to: NodeRef::issue("linear", issue_id),
                relation: Relation::AuthorOf,
                observed_at: updated_at,
                metadata: None,
            });
        }

        // Issue → Project
        if let Some(project) = issue.get("project").and_then(|p| p.get("id")) {
            edges.push(ExtractedEdge {
                from: NodeRef::issue("linear", issue_id),
                to: NodeRef::project("linear", project.as_str().unwrap()),
                relation: Relation::BelongsTo,
                observed_at: updated_at,
                metadata: None,
            });
        }

        // Issue relations (blocks, depends_on)
        if let Some(relations) = issue.get("relations").and_then(|r| r.as_array()) {
            for rel in relations {
                let rel_type = rel["type"].as_str().unwrap_or_default();
                let related_id = rel["relatedIssue"]["id"].as_str().unwrap_or_default();

                let relation = match rel_type {
                    "blocks" => Relation::Blocks,
                    "is-blocked-by" => continue, // Skip reverse, we store one direction
                    "duplicate" | "related" => Relation::References,
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

    async fn discover(&self, ctx: &SyncContext<'_>) -> Result<serde_json::Value> {
        // Move existing discover logic
        todo!()
    }
}
```

### Testing Criteria

```
□ `minna sync linear` works with new provider
□ Same documents indexed as before (regression test)
□ graph_edges table populated after sync
□ Assignee edges created for assigned issues
□ Project edges created
□ Issue relation edges created (blocks, etc.)
□ No duplicate edges on re-sync
□ Performance: sync time within 10% of legacy
```

### Go/No-Go Decision Point

After Sprint 2, evaluate:

| Criteria | Pass | Fail Action |
|----------|------|-------------|
| Linear sync works | ✓ | Debug, don't proceed |
| Edge extraction accurate | ✓ | Fix extraction logic |
| Performance acceptable | ✓ | Optimize before proceeding |
| No regressions | ✓ | Fix regressions |

**If all pass:** Proceed to Sprint 3 (Slack + GitHub migration)
**If any fail:** Fix issues before proceeding, re-evaluate approach

---

## Sprint 3: Slack + GitHub Migration

**Goal:** Migrate remaining high-value providers to trait system.

**Only proceed if Sprint 2 Linear migration was successful.**

### Tasks: Slack Migration

| ID | Task | File | Est |
|----|------|------|-----|
| 3.1 | Create `slack.rs` provider file | `providers/slack.rs` | 2h |
| 3.2 | Move Slack sync logic (~617 lines) | `providers/slack.rs` | 10h |
| 3.3 | Implement `extract_edges()` for Slack | `providers/slack.rs` | 4h |
| 3.4 | Extract: Author → Message edges | `providers/slack.rs` | 2h |
| 3.5 | Extract: @mention edges (regex) | `providers/slack.rs` | 3h |
| 3.6 | Extract: Message → Channel edges | `providers/slack.rs` | 1h |
| 3.7 | Extract: Thread edges | `providers/slack.rs` | 2h |
| 3.8 | Integration tests | `tests/slack.rs` | 6h |

### Tasks: GitHub Migration

| ID | Task | File | Est |
|----|------|------|-----|
| 3.9 | Create `github.rs` provider file | `providers/github.rs` | 2h |
| 3.10 | Move GitHub sync logic (~145 lines) | `providers/github.rs` | 4h |
| 3.11 | Implement `extract_edges()` for GitHub | `providers/github.rs` | 4h |
| 3.12 | Extract: Author, Assignee, Reviewer edges | `providers/github.rs` | 3h |
| 3.13 | Extract: @mention edges (regex) | `providers/github.rs` | 2h |
| 3.14 | Integration tests | `tests/github.rs` | 4h |

### Slack Edge Extraction

```rust
fn extract_edges(&self, message: &serde_json::Value) -> Vec<ExtractedEdge> {
    let mut edges = vec![];
    let ts = message["ts"].as_str().unwrap_or_default();
    let channel = message["channel"].as_str().unwrap_or_default();
    let author = message["user"].as_str().unwrap_or_default();
    let text = message["text"].as_str().unwrap_or_default();
    let observed_at = parse_slack_ts(ts);

    // Author → Message
    edges.push(ExtractedEdge {
        from: NodeRef::user("slack", author),
        to: NodeRef::message("slack", ts),
        relation: Relation::AuthorOf,
        observed_at,
        metadata: None,
    });

    // Message → Channel
    edges.push(ExtractedEdge {
        from: NodeRef::message("slack", ts),
        to: NodeRef::channel("slack", channel),
        relation: Relation::PostedIn,
        observed_at,
        metadata: None,
    });

    // @mentions: <@U1234567890>
    let mention_re = Regex::new(r"<@(U[A-Z0-9]+)>").unwrap();
    for cap in mention_re.captures_iter(text) {
        edges.push(ExtractedEdge {
            from: NodeRef::user("slack", &cap[1]),
            to: NodeRef::message("slack", ts),
            relation: Relation::MentionedIn,
            observed_at,
            metadata: None,
        });
    }

    // Thread
    if let Some(thread_ts) = message.get("thread_ts").and_then(|t| t.as_str()) {
        if thread_ts != ts { // Not the parent message
            edges.push(ExtractedEdge {
                from: NodeRef::message("slack", ts),
                to: NodeRef::thread("slack", thread_ts),
                relation: Relation::ThreadOf,
                observed_at,
                metadata: None,
            });
        }
    }

    edges
}
```

### Testing Criteria

```
□ All three providers (Linear, Slack, GitHub) work via registry
□ Legacy sync methods removed from lib.rs
□ Graph populated with edges from all providers
□ @mention extraction works for Slack (<@U...>) and GitHub (@username)
□ No sync regressions
```

---

## Sprint 4: RingEngine + LocalGit

**Goal:** Ring calculation working, LocalGit extractor operational.

### Tasks: RingEngine

| ID | Task | File | Est |
|----|------|------|-----|
| 4.1 | Create `rings.rs` module | `minna-graph/src/rings.rs` | 2h |
| 4.2 | Implement `RingEngine` struct | `rings.rs` | 4h |
| 4.3 | Implement `edge_weight()` with decay | `rings.rs` | 4h |
| 4.4 | Implement ghost edge logic (90 days) | `rings.rs` | 2h |
| 4.5 | Implement `compute_rings()` weighted BFS | `rings.rs` | 8h |
| 4.6 | Add depth-3 hard cap | `rings.rs` | 1h |
| 4.7 | Implement ring cache persistence | `rings.rs` | 4h |
| 4.8 | Unit tests for decay math | `tests/decay.rs` | 4h |
| 4.9 | Integration tests for ring computation | `tests/rings.rs` | 6h |
| 4.10 | Benchmark on 10k/50k node graphs | `benches/rings.rs` | 4h |

### Tasks: LocalGit

| ID | Task | File | Est |
|----|------|------|-----|
| 4.11 | Add git2 dependency | `minna-graph/Cargo.toml` | 1h |
| 4.12 | Create `LocalGitExtractor` | `minna-graph/src/extractors/local_git.rs` | 6h |
| 4.13 | Implement commit walking (90-day cutoff) | `local_git.rs` | 4h |
| 4.14 | Implement file diff extraction | `local_git.rs` | 4h |
| 4.15 | Implement `get_file_collaborators()` | `local_git.rs` | 4h |
| 4.16 | Integration tests with test repo | `tests/local_git.rs` | 4h |

### Testing Criteria

```
□ Fresh 1-hop edge → Ring 1
□ Stale 1-hop edge (1 year) → Beyond
□ BFS < 100ms for 10k nodes
□ LocalGit extracts author → file edges
□ LocalGit respects 90-day cutoff
```

---

## Sprint 5: CLI + Search Integration

**Goal:** `minna gravity` commands work, search is ring-boosted.

### Tasks

| ID | Task | File | Est |
|----|------|------|-----|
| 5.1 | Create `gravity.rs` command module | `minna-cli/src/commands/gravity.rs` | 2h |
| 5.2 | Implement `gravity status` | `gravity.rs` | 4h |
| 5.3 | Implement `gravity show` | `gravity.rs` | 4h |
| 5.4 | Implement `gravity explain <entity>` | `gravity.rs` | 6h |
| 5.5 | Implement `gravity refresh` | `gravity.rs` | 2h |
| 5.6 | Implement `gravity pin/unpin` | `gravity.rs` | 4h |
| 5.7 | Implement `identity link suggest` | `gravity.rs` | 6h |
| 5.8 | Add ring boost to MCP search | `minna-mcp/src/lib.rs` | 4h |
| 5.9 | Add ring filter to search | `minna-mcp/src/lib.rs` | 3h |
| 5.10 | CLI integration tests | `tests/cli_gravity.rs` | 6h |

---

## Sprint 6: Sync Scheduler

**Goal:** Ring-aware sync scheduling operational.

### Tasks

| ID | Task | File | Est |
|----|------|------|-----|
| 6.1 | Create `scheduler.rs` module | `minna-core/src/scheduler.rs` | 4h |
| 6.2 | Implement Ring 1 hourly sync | `scheduler.rs` | 4h |
| 6.3 | Implement Ring 2 head-only sync | `scheduler.rs` | 6h |
| 6.4 | Implement Beyond on-demand | `scheduler.rs` | 4h |
| 6.5 | Add `SyncDepth` enum | `providers/mod.rs` | 2h |
| 6.6 | Integrate scheduler in daemon | `minna-server/src/main.rs` | 6h |
| 6.7 | Add sync budget tracking | `scheduler.rs` | 4h |
| 6.8 | Integration tests | `tests/scheduler.rs` | 6h |
| 6.9 | Measure sync volume reduction | Manual | 4h |

---

## Hardening: Weeks 13-14

- Bug fixes from Sprints 1-6
- Performance optimization
- Memory profiling
- Error handling audit
- User documentation
- Team dogfooding
- Address feedback

---

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Linear migration reveals trait issues | Low | High | Sprint 2 is go/no-go gate |
| Slack migration complex (617 lines) | Medium | Medium | Budget extra time, can split |
| git2 crate issues | Low | Medium | Fallback to shell git |
| BFS performance on large graphs | Low | High | Early benchmarking in Sprint 4 |
| Sync regressions | Medium | High | Comprehensive regression tests |

---

## Summary: Clean Architecture Path

```
Sprint 1: Foundation
    ↓
Sprint 2: Linear (proof of concept) ←── GO/NO-GO GATE
    ↓
Sprint 3: Slack + GitHub (if Sprint 2 passes)
    ↓
Sprint 4: RingEngine + LocalGit
    ↓
Sprint 5: CLI + Search
    ↓
Sprint 6: Scheduler
    ↓
Hardening
```

**Total timeline: 14 weeks** (was 12 with hooks approach)
**Benefit:** Clean, consistent architecture. No tech debt. Easier to maintain.

---

*Ready for execution.*
