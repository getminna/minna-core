# Contributing to Minna Core

Thanks for your interest in contributing to Minna! This guide will help you get started.

## Table of Contents

- [Development Setup](#development-setup)
- [Project Structure](#project-structure)
- [Adding a New Connector (Rust)](#adding-a-new-connector-rust) ← **Start here**
- [Adding MCP Tools](#adding-mcp-tools)
- [Adding CLI Commands](#adding-cli-commands)
- [Code Style](#code-style)
- [Pull Requests](#pull-requests)
- [Security](#security)

---

## Development Setup

### Prerequisites

- **Rust 1.75+** (`rustup update stable`) - for the engine
- macOS 14+ (for Keychain integration)

### Getting Started (Rust Engine)

```bash
# Clone the repo
git clone https://github.com/minna-ai/minna-core.git
cd minna-core/engine

# Build everything
cargo build

# Run tests
cargo test

# Run the daemon locally
cargo run --bin minna-server

# Run CLI commands
cargo run --bin minna -- add slack
cargo run --bin minna -- sync
cargo run --bin minna -- status
```

---

## Project Structure

```
engine/                           # Rust engine (primary)
├── crates/
│   ├── minna-core/               # Core sync engine
│   │   └── src/
│   │       ├── lib.rs            # Core struct, sync methods
│   │       ├── providers/        # Extensible provider system ★
│   │       │   ├── mod.rs        # SyncProvider trait, registry
│   │       │   ├── config.rs     # TOML config schema
│   │       │   ├── notion.rs     # Notion connector
│   │       │   └── atlassian.rs  # Jira + Confluence connector
│   │       └── progress.rs       # UI progress events
│   ├── minna-server/             # Daemon (Unix socket server)
│   ├── minna-cli/                # CLI commands
│   ├── minna-mcp/                # MCP protocol handlers
│   ├── minna-ingest/             # Document storage (SQLite)
│   ├── minna-vector/             # Embeddings + search
│   └── minna-auth-bridge/        # Keychain integration
```

---

## Adding a New Connector (Rust)

This is the **recommended way** to add new connectors. We've built an extensible provider system that makes this straightforward.

### Connectors We'd Love

| Priority | Service | Notes |
|----------|---------|-------|
| ⭐ High | Airtable | REST API, PAT auth |
| ⭐ High | Asana | REST API, PAT auth |
| ⭐ High | Monday.com | GraphQL API |
| Medium | Trello | REST API |
| Medium | Basecamp | REST API |
| Medium | Figma | REST API (comments, files) |
| Medium | Dropbox | OAuth2 |
| Medium | Intercom | REST API |
| Lower | Discord | Bot token |
| Lower | Microsoft 365 | OAuth2 (complex) |

### Step 1: Add Configuration

Edit `engine/providers.example.toml` (or create `~/.minna/providers.toml`):

```toml
[providers.airtable]
enabled = true
display_name = "Airtable"
api_base_url = "https://api.airtable.com/v0"
[providers.airtable.auth]
type = "keychain"
account = "airtable_token"
token_prefix = "pat"
```

### Step 2: Create the Provider

Create `engine/crates/minna-core/src/providers/airtable.rs`:

```rust
use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use serde::Deserialize;
use tracing::info;

use crate::Document;
use crate::progress::emit_progress;
use super::{SyncContext, SyncProvider, SyncSummary, call_with_backoff, calculate_since};

pub struct AirtableProvider;

#[async_trait]
impl SyncProvider for AirtableProvider {
    fn name(&self) -> &'static str { "airtable" }
    fn display_name(&self) -> &'static str { "Airtable" }

    async fn sync(
        &self,
        ctx: &SyncContext<'_>,
        since_days: Option<i64>,
        mode: Option<&str>,
    ) -> Result<SyncSummary> {
        // 1. Load token from Keychain
        let token = ctx.registry.load_token("airtable")?;

        // 2. Calculate time window
        let cursor = ctx.get_sync_cursor("airtable").await?;
        let since = calculate_since(since_days, mode, cursor.as_deref());

        // 3. Fetch data from API (with pagination)
        let mut documents_processed = 0;

        // Example: List bases, then records
        let response = call_with_backoff("airtable", || {
            ctx.http_client
                .get("https://api.airtable.com/v0/meta/bases")
                .bearer_auth(&token)
        }).await?;

        // ... process response, create Documents ...

        // 4. For each item, create and index a Document
        let doc = Document {
            id: None,
            uri: format!("https://airtable.com/{}", record_id),
            source: "airtable".to_string(),
            title: Some(record_name.clone()),
            body: format!("# {}\n\n{}", record_name, fields_as_text),
            updated_at: Utc::now(),
        };
        ctx.index_document(doc).await?;
        documents_processed += 1;

        // 5. Emit progress for UI
        emit_progress("airtable", "syncing", "Processing...", Some(documents_processed));

        // 6. Update sync cursor
        ctx.set_sync_cursor("airtable", &Utc::now().to_rfc3339()).await?;

        // 7. Return summary
        Ok(SyncSummary {
            provider: "airtable".to_string(),
            items_scanned: 1,
            documents_processed,
            updated_at: Utc::now().to_rfc3339(),
        })
    }
}
```

### Step 3: Register the Provider

In `engine/crates/minna-core/src/providers/mod.rs`:

```rust
mod airtable;
pub use airtable::AirtableProvider;

// In register_builtin_providers():
if config.is_enabled("airtable") {
    map.insert("airtable".to_string(), Arc::new(AirtableProvider));
}
```

### Step 4: Add CLI Support

In `engine/crates/minna-cli/src/sources/mod.rs`, add to the `Source` enum and implement instructions:

```rust
Source::Airtable => SourceInstructions {
    title: "To connect Airtable, you'll need a Personal Access Token.",
    recommended_url: None,
    steps: vec![
        "Go to: https://airtable.com/create/tokens",
        "Create a token with read access",
        "Copy the token",
    ],
    auth_type: AuthType::Token {
        prompt: "Paste your Airtable PAT",
        prefix: Some("pat"),
    },
},
```

### Step 5: Build and Test

```bash
cd engine
cargo build
cargo run --bin minna -- add airtable
cargo run --bin minna -- sync airtable
```

### Connector Guidelines

| Guideline | Why |
|-----------|-----|
| Use `call_with_backoff()` for HTTP | Handles rate limits (429) automatically |
| Store cursors via `ctx.set_sync_cursor()` | Enables incremental delta syncs |
| Include metadata header in body | Helps semantic search (title, URL, dates) |
| Use stable IDs in URIs | Prevents duplicate documents |
| Continue on 403 errors | Users may lack access to some items |
| Call `emit_progress()` every ~10 items | Keeps UI responsive |

---

## Adding MCP Tools

Extend the query interface that AI agents use.

**Location:** `engine/crates/minna-mcp/src/lib.rs`

**Ideas for new tools:**
- `list_sources` - Show connected sources and sync status
- `search_by_date` - Find items from a specific time range
- `get_recent_activity` - Last N items across all sources
- `get_context_for_file` - Find related work items for a code file

### How to Add a Tool

1. Add the tool definition to `list_tools` response
2. Implement the handler in `handle_tool_call`
3. Define parameter schema using JSON Schema

---

## Adding CLI Commands

**Location:** `engine/crates/minna-cli/src/commands/`

**Ideas:**
- `minna export` - Export data to JSON/CSV
- `minna stats` - Show sync statistics
- `minna search` - CLI search interface
- `minna reset <source>` - Clear and re-sync

Create a new file in `commands/` and wire it up in `main.rs`.

---

## Code Style

### Rust

- Follow idiomatic Rust patterns
- Use `cargo fmt` to format code
- Use `cargo clippy` for linting
- Write tests for new providers

```bash
# Format code
cargo fmt

# Lint code
cargo clippy
```

---

## Pull Requests

1. Fork the repo
2. Create a feature branch (`git checkout -b feature/linear-connector`)
3. Make your changes
4. Run tests (`cargo test`)
5. Format code (`cargo fmt`)
6. Commit with a clear message
7. Push and open a PR

### PR Checklist

- [ ] Tests pass
- [ ] Code is formatted
- [ ] New connector has `sync()` implemented
- [ ] CLI integration is complete
- [ ] README updated if adding a new connector

---

## Security

**Never commit credentials or tokens.**

- Use environment variables or Keychain for local testing
- Sanitize any tokens in logs before sharing
- If you find a security vulnerability, please email security@minna.ai

---

## Questions?

- Open an issue for bugs or feature requests
- See [README.md](README.md) for architecture overview

Thanks for contributing! 🎉
