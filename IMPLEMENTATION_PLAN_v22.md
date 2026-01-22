# Gravity Well Implementation Plan (Revised)

**Project:** Minna Gravity Well (Blueprint v22)
**Status:** VALIDATED AGAINST CODEBASE
**Last Updated:** January 21, 2026

---

## Codebase Reality Check

### Current Architecture

```
engine/crates/
├── minna-core/          # Sync engine + legacy provider methods
│   └── src/
│       ├── lib.rs       # Core struct with sync_slack(), sync_github(), sync_linear()
│       └── providers/   # NEW trait-based system (only Notion, Atlassian)
├── minna-ingest/        # Document storage (SQLite + FTS5)
├── minna-vector/        # Embeddings (sqlite-vec)
├── minna-mcp/           # MCP protocol
├── minna-server/        # Daemon
├── minna-cli/           # CLI (commands: add, sync, status, daemon, setup, remove)
└── minna-auth-bridge/   # Keychain
```

### Provider Status

| Provider | Current Implementation | Edge Extraction Strategy |
|----------|----------------------|-------------------------|
| **Slack** | Legacy: `Core::sync_slack()` ~600 lines in lib.rs | Add extraction hooks to existing method |
| **Linear** | Legacy: `Core::sync_linear()` ~400 lines in lib.rs | Add extraction hooks to existing method |
| **GitHub** | Legacy: `Core::sync_github()` ~200 lines in lib.rs | Add extraction hooks to existing method |
| **Google** | Legacy: `Core::sync_google()` | Defer (low graph value) |
| **Notion** | New: `SyncProvider` trait | Implement `extract_edges()` in trait |
| **Atlassian** | New: `SyncProvider` trait | Implement `extract_edges()` in trait |
| **LocalGit** | Does not exist | New standalone extractor |

### Key Decision: Don't Migrate Legacy Providers

Migrating `sync_slack()`, `sync_linear()`, `sync_github()` to the trait system is a separate project. For Gravity Well, we:

1. Add edge extraction hooks to existing legacy methods
2. Implement trait-based extraction for Notion/Atlassian
3. Create new LocalGit extractor

---

## Revised Sprint Plan

| Sprint | Duration | Focus | Risk |
|--------|----------|-------|------|
| **Sprint 1** | Weeks 1-2 | Graph schema in minna-ingest + minna-graph crate | Low |
| **Sprint 2** | Weeks 3-4 | Edge extraction for legacy providers + LocalGit | Medium |
| **Sprint 3** | Weeks 5-6 | RingEngine with temporal decay | Medium |
| **Sprint 4** | Weeks 7-8 | CLI + Search integration | Low |
| **Sprint 5** | Weeks 9-10 | Sync scheduler | Medium |
| **Hardening** | Weeks 11-12 | Polish, testing, docs | Low |

---

## Sprint 1: Graph Foundation

**Goal:** Graph tables exist in minna-ingest, minna-graph crate created.

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
| 1.7 | Implement `GraphStore` with CRUD | `minna-graph/src/storage.rs` | 6h |
| 1.8 | Implement `edges_from()`, `edges_to()` | `minna-graph/src/storage.rs` | 4h |
| 1.9 | Define `ExtractedEdge`, `NodeRef` | `minna-graph/src/schema.rs` | 2h |
| 1.10 | Unit tests for graph operations | `minna-graph/src/lib.rs` | 6h |

### Schema Addition to minna-ingest

```rust
// In minna-ingest/src/lib.rs, add to init_schema():

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

sqlx::query("CREATE INDEX IF NOT EXISTS idx_edges_from ON graph_edges(from_node)").execute(&self.pool).await?;
sqlx::query("CREATE INDEX IF NOT EXISTS idx_edges_to ON graph_edges(to_node)").execute(&self.pool).await?;

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

### Testing Criteria

```
□ minna-graph crate compiles
□ Graph tables created on fresh DB
□ Can insert 10,000 nodes in < 1s
□ Can insert 50,000 edges in < 5s
□ edges_from() returns correct edges
□ UNIQUE constraint prevents duplicates
```

---

## Sprint 2: Edge Extraction

**Goal:** Legacy providers emit edges, LocalGit extractor works.

### File Changes

```
engine/crates/
├── minna-core/
│   └── src/
│       ├── lib.rs                      # MODIFY: Add edge emission to sync_slack(), etc.
│       ├── edge_extraction.rs          # NEW: Shared extraction logic
│       └── providers/
│           ├── mod.rs                  # MODIFY: Add extract_edges() to SyncProvider trait
│           ├── notion.rs               # MODIFY: Implement extract_edges()
│           └── atlassian.rs            # MODIFY: Implement extract_edges()
├── minna-graph/
│   └── src/
│       └── extractors/                 # NEW: Standalone extractors
│           ├── mod.rs
│           └── local_git.rs            # NEW: LocalGitExtractor
```

### Tasks

| ID | Task | File | Est |
|----|------|------|-----|
| 2.1 | Create `edge_extraction.rs` module | `minna-core/src/edge_extraction.rs` | 4h |
| 2.2 | Add `GraphStore` to `Core` struct | `minna-core/src/lib.rs` | 2h |
| 2.3 | Add edge emission to `sync_slack()` | `minna-core/src/lib.rs` | 6h |
| 2.4 | Add edge emission to `sync_linear()` | `minna-core/src/lib.rs` | 6h |
| 2.5 | Add edge emission to `sync_github()` | `minna-core/src/lib.rs` | 4h |
| 2.6 | Add `extract_edges()` to `SyncProvider` trait | `minna-core/src/providers/mod.rs` | 2h |
| 2.7 | Implement for NotionProvider | `minna-core/src/providers/notion.rs` | 4h |
| 2.8 | Implement for AtlassianProvider | `minna-core/src/providers/atlassian.rs` | 4h |
| 2.9 | Create `LocalGitExtractor` | `minna-graph/src/extractors/local_git.rs` | 8h |
| 2.10 | Add git2 dependency | `minna-graph/Cargo.toml` | 1h |
| 2.11 | Integration tests with mock data | `minna-graph/tests/` | 8h |

### Edge Extraction Hook Pattern (Legacy Providers)

```rust
// In minna-core/src/lib.rs, inside sync_slack():

// After parsing a message:
let message_node = self.graph.upsert_node(NodeRef {
    node_type: NodeType::Message,
    provider: "slack",
    external_id: &message.ts,
    display_name: None,
})?;

let author_node = self.graph.upsert_node(NodeRef {
    node_type: NodeType::User,
    provider: "slack",
    external_id: &message.user,
    display_name: Some(&user_name),
})?;

self.graph.upsert_edge(ExtractedEdge {
    from: author_node,
    to: message_node,
    relation: Relation::AuthorOf,
    observed_at: message_timestamp,
    provider: "slack",
})?;

// Extract @mentions
for mention in extract_slack_mentions(&message.text) {
    let mentioned_node = self.graph.upsert_node(NodeRef {
        node_type: NodeType::User,
        provider: "slack",
        external_id: &mention,
        display_name: None,
    })?;

    self.graph.upsert_edge(ExtractedEdge {
        from: mentioned_node,
        to: message_node,
        relation: Relation::MentionedIn,
        observed_at: message_timestamp,
        provider: "slack",
    })?;
}
```

### LocalGit Integration

```rust
// LocalGitExtractor is standalone, called via CLI or on project add
// minna-graph/src/extractors/local_git.rs

impl LocalGitExtractor {
    pub fn new(repo_path: PathBuf) -> Result<Self> { ... }

    pub fn extract_edges(&self) -> Result<Vec<ExtractedEdge>> {
        // Walk commits, extract author -> file edges
    }

    pub fn get_file_collaborators(&self, file_path: &str) -> Result<Vec<Collaborator>> {
        // For real-time "who else touched this file" queries
    }
}
```

### Testing Criteria

```
□ sync_slack() populates graph_edges table
□ Slack @mentions create MentionedIn edges
□ Linear issues create AssignedTo, BelongsTo edges
□ GitHub PRs create ReviewerOf edges
□ LocalGitExtractor extracts edges from test repo
□ All extractors handle malformed input gracefully
```

---

## Sprint 3: RingEngine + Temporal Decay

**Goal:** Rings computed correctly with decay, cached.

### File Changes

```
engine/crates/minna-graph/src/
├── lib.rs                    # Export RingEngine
├── rings.rs                  # NEW: RingEngine implementation
└── decay.rs                  # NEW: Temporal decay functions
```

### Tasks

| ID | Task | File | Est |
|----|------|------|-----|
| 3.1 | Create `RingEngine` struct | `minna-graph/src/rings.rs` | 4h |
| 3.2 | Implement `edge_weight()` with decay | `minna-graph/src/decay.rs` | 4h |
| 3.3 | Implement ghost edge logic (90 days) | `minna-graph/src/decay.rs` | 2h |
| 3.4 | Implement `compute_rings()` weighted BFS | `minna-graph/src/rings.rs` | 8h |
| 3.5 | Add depth-3 hard cap | `minna-graph/src/rings.rs` | 1h |
| 3.6 | Implement ring cache persistence | `minna-graph/src/rings.rs` | 4h |
| 3.7 | Implement `should_recompute()` | `minna-graph/src/rings.rs` | 3h |
| 3.8 | Add `get_ring()` public API | `minna-graph/src/rings.rs` | 2h |
| 3.9 | Unit tests for decay math | `minna-graph/src/decay.rs` | 4h |
| 3.10 | Integration tests for ring computation | `minna-graph/tests/rings.rs` | 6h |
| 3.11 | Benchmark on 10k/50k/100k graphs | `minna-graph/benches/` | 4h |

### Testing Criteria

```
□ 1-hop fresh edge (1 day old) → Ring 1
□ 1-hop stale edge (1 year old) → Beyond
□ 2-hop fresh edge → Ring 2
□ 3+ hop → Beyond (depth cap)
□ BFS < 100ms for 10k nodes
□ BFS < 500ms for 50k nodes
□ Ring cache persists across restarts
```

---

## Sprint 4: CLI + Search Integration

**Goal:** `minna gravity` commands work, search is ring-boosted.

### File Changes

```
engine/crates/
├── minna-cli/src/
│   ├── commands/
│   │   ├── mod.rs            # MODIFY: Add gravity module
│   │   └── gravity.rs        # NEW: gravity subcommands
│   └── main.rs               # MODIFY: Register gravity command
├── minna-mcp/src/
│   └── lib.rs                # MODIFY: Add ring boost to search
├── minna-core/src/
│   └── lib.rs                # MODIFY: Add ring-aware search method
```

### Tasks

| ID | Task | File | Est |
|----|------|------|-----|
| 4.1 | Create `gravity.rs` command module | `minna-cli/src/commands/gravity.rs` | 2h |
| 4.2 | Implement `gravity status` | `minna-cli/src/commands/gravity.rs` | 4h |
| 4.3 | Implement `gravity show` | `minna-cli/src/commands/gravity.rs` | 4h |
| 4.4 | Implement `gravity explain <entity>` | `minna-cli/src/commands/gravity.rs` | 6h |
| 4.5 | Implement `gravity refresh` | `minna-cli/src/commands/gravity.rs` | 2h |
| 4.6 | Implement `gravity pin/unpin` | `minna-cli/src/commands/gravity.rs` | 4h |
| 4.7 | Implement `identity link suggest` | `minna-cli/src/commands/gravity.rs` | 6h |
| 4.8 | Add ring boost to MCP search | `minna-mcp/src/lib.rs` | 4h |
| 4.9 | Add ring filter to search | `minna-mcp/src/lib.rs` | 3h |
| 4.10 | CLI integration tests | `minna-cli/tests/` | 6h |

### CLI Registration

```rust
// In minna-cli/src/main.rs
#[derive(Subcommand)]
enum Commands {
    Add { ... },
    Sync { ... },
    Status,
    // ... existing commands

    /// Manage your gravity well (collaboration graph)
    Gravity {
        #[command(subcommand)]
        command: GravityCommands,
    },
}

#[derive(Subcommand)]
enum GravityCommands {
    /// Show gravity well status
    Status,
    /// Show Ring 1 and Ring 2 members
    Show,
    /// Explain why an entity is in its ring
    Explain { entity: String },
    /// Force ring recomputation
    Refresh,
    /// Pin entity to Ring 1
    Pin { entity: String },
    /// Remove pin from entity
    Unpin { entity: String },
}
```

### Testing Criteria

```
□ `minna gravity status` shows node/edge counts
□ `minna gravity explain` shows path to user
□ `minna gravity refresh` triggers recomputation
□ Ring 1 results boosted 2x in search
□ Ring filter works
```

---

## Sprint 5: Sync Scheduler

**Goal:** Ring-aware sync scheduling operational.

### File Changes

```
engine/crates/
├── minna-core/src/
│   ├── scheduler.rs          # NEW: GravityScheduler
│   └── lib.rs                # MODIFY: Integrate scheduler
├── minna-server/src/
│   └── main.rs               # MODIFY: Use scheduler for sync dispatch
```

### Tasks

| ID | Task | File | Est |
|----|------|------|-----|
| 5.1 | Create `GravityScheduler` | `minna-core/src/scheduler.rs` | 4h |
| 5.2 | Implement Ring 1 hourly sync | `minna-core/src/scheduler.rs` | 4h |
| 5.3 | Implement Ring 2 head-only sync | `minna-core/src/scheduler.rs` | 6h |
| 5.4 | Implement Beyond on-demand | `minna-core/src/scheduler.rs` | 4h |
| 5.5 | Add `SyncDepth` enum | `minna-core/src/providers/mod.rs` | 2h |
| 5.6 | Integrate scheduler in daemon | `minna-server/src/main.rs` | 6h |
| 5.7 | Add sync budget tracking | `minna-core/src/scheduler.rs` | 4h |
| 5.8 | Store manual pins in config | `minna-core/src/scheduler.rs` | 3h |
| 5.9 | Integration tests | `minna-core/tests/scheduler.rs` | 6h |
| 5.10 | Measure sync volume reduction | Manual testing | 4h |

### Testing Criteria

```
□ Ring 1 objects sync hourly
□ Ring 2 objects sync 6-hourly (head-only)
□ Beyond objects only on query
□ Pinned entities treated as Ring 1
□ Sync volume reduced by 40%
```

---

## Hardening: Weeks 11-12

Same as original plan—bug fixes, performance, documentation, dogfooding.

---

## Dependency Graph

```
Sprint 1 (Graph Schema)
    │
    ├── Sprint 2 (Edge Extraction) ──────┐
    │                                     │
    └── Sprint 3 (RingEngine) ───────────┤
                                          │
                                          ▼
                              Sprint 4 (CLI + Search)
                                          │
                                          ▼
                              Sprint 5 (Scheduler)
                                          │
                                          ▼
                                    Hardening
```

Sprint 2 and Sprint 3 can partially overlap—extraction doesn't require rings, rings don't require extraction (can test with synthetic data).

---

## Risk Mitigations (Updated)

| Risk | Original Assumption | Reality | Mitigation |
|------|---------------------|---------|------------|
| Legacy provider migration | Would migrate to trait | Keep legacy, add hooks | Add extraction directly to existing methods |
| Separate graph DB | New database | Same SQLite via minna-ingest | Add tables to existing schema |
| git2 complexity | Unknown | Medium | Spike early, fallback to shell git |
| Schema migration | New DB | Existing DB with data | Write migration for existing users |

---

## Pre-Sprint Checklist

Before starting Sprint 1:

- [ ] Confirm: Is there existing user data in `minna.db` that needs migration?
- [ ] Confirm: What's the current embedding dimension? (for graph node metadata)
- [ ] Confirm: Are there any existing graph-like structures we should reuse?
- [ ] Spike: Test git2 crate with a real repo (1 day)

---

**Ready for review.** This plan accounts for actual codebase structure.
