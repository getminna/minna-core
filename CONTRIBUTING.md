# Contributing to Minna Core

Thanks for your interest in contributing to Minna! This guide will help you get started.

## Table of Contents

- [Development Setup](#development-setup)
- [Project Structure](#project-structure)
- [Adding a New Connector (Rust)](#adding-a-new-connector-rust) ← **Start here**
- [Adding MCP Tools](#adding-mcp-tools)
- [Adding CLI Commands](#adding-cli-commands)
- [Legacy: Python Connectors](#legacy-python-connectors)
- [Code Style](#code-style)
- [Pull Requests](#pull-requests)
- [Security](#security)

---

## Development Setup

### Prerequisites

- **Rust 1.75+** (`rustup update stable`) - for the engine
- macOS 14+ (for Keychain integration)
- Python 3.11+ and Poetry (optional, for legacy connectors)
- Xcode 15+ (optional, for macOS app)

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

### Building the macOS App (Optional)

```bash
cd src/minna_ep1
xcodegen generate
open MinnaEP1.xcodeproj
# Build with Cmd+B in Xcode
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
│
minna-python-legacy/              # Python (deprecated)
│   └── minna/
│       ├── ingestion/            # Legacy connectors
│       └── workers/              # Legacy workers
│
src/minna_ep1/                    # macOS SwiftUI app
    ├── MinnaEP1App.swift
    ├── MinnaEngineManager.swift
    └── Views/
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

## Legacy: Python Connectors

> **Note:** The Python codebase is deprecated. New connectors should use the Rust engine above.

If you need to modify existing Python connectors:

### Quick Start: Use the Scaffold

The fastest way to create a new connector:

```bash
# Generate boilerplate
poetry run python -m minna.scaffold linear

# With Swift UI instructions
poetry run python -m minna.scaffold linear --with-ui
```

This creates:
- `src/minna/ingestion/linear.py` - Connector class
- `src/minna/workers/linear_worker.py` - Worker class
- Instructions for CLI and Swift UI integration

### Manual Guide

If you prefer to create files manually:

#### Step 1: Create the Connector

Create `src/minna/ingestion/yourservice.py`:

```python
from .base import BaseConnector, Document, DiscoveryResult, ProgressCallback

class YourServiceConnector(BaseConnector):
    """Syncs YourService data to Minna."""
    
    CONNECTOR_NAME = "yourservice"
    BASE_URL = "https://api.yourservice.com"
    
    def __init__(self, api_key: str, progress_callback: ProgressCallback = None):
        super().__init__(progress_callback)
        self.api_key = api_key
        self._docs_processed = 0
    
    def sync(self, since_timestamp: float = 0) -> list[Document]:
        """Fetch data and return Documents."""
        documents = []
        
        # Fetch from API
        # Convert to Documents using self._make_document()
        # Report progress using self._emit_progress()
        
        return documents
    
    def discover(self) -> DiscoveryResult:
        """Quick scan for FirstSyncSheet UX."""
        return DiscoveryResult(
            total_items=100,
            estimated_quick_sync_minutes=1,
            estimated_full_sync_minutes=5,
        )
```

#### Step 2: Create the Worker

Create `src/minna/workers/yourservice_worker.py`:

```python
from minna.ingestion.base import BaseWorker, SyncResult
from minna.ingestion.yourservice import YourServiceConnector

class YourServiceWorker(BaseWorker):
    """Orchestrates YourService sync."""
    
    KEYCHAIN_API_KEY = "yourservice_api_key"
    
    def __init__(self, progress_callback=None):
        super().__init__(progress_callback)
        
        api_key = self._get_credential(self.KEYCHAIN_API_KEY)
        if not api_key:
            raise ValueError("No API key found in Keychain")
        
        self.connector = YourServiceConnector(api_key, self.progress_callback)
    
    def sync(self, since_timestamp: float = 0) -> SyncResult:
        documents = self.connector.sync(since_timestamp)
        if documents:
            self.db.add_documents(documents)
        return {"documents": len(documents), "success": True, "errors": []}
```

#### Step 3: Add to CLI

In `src/minna/cli.py`, add your provider to:

1. **discover_provider()** function:
```python
elif provider == "yourservice":
    from minna.workers.yourservice_worker import YourServiceWorker
    worker = YourServiceWorker(progress_callback=emit_progress)
    result = worker.discover()
    emit_result(result)
    emit_progress("discovered", "Discovery complete")
```

2. **sync_provider()** function:
```python
elif provider == "yourservice":
    from minna.workers.yourservice_worker import YourServiceWorker
    worker = YourServiceWorker(progress_callback=emit_progress)
    result = worker.sync(since_timestamp=since_timestamp)
    emit_progress("complete", "YourService sync complete",
                 documents_processed=result.get("documents", 0))
```

3. **argparse choices** (both discover and sync):
```python
choices=["slack", "google", "github", "yourservice"]
```

#### Step 4: Test Your Connector

```bash
# Run discovery
poetry run python -m minna.cli discover --provider yourservice

# Run sync
poetry run python -m minna.cli sync --provider yourservice

# Run with date filter
poetry run python -m minna.cli sync --provider yourservice --since-days 7
```

---

## Swift UI Integration (Optional)

To add a native macOS config sheet for your connector:

### Step 1: Add to Provider Enum

In `MinnaEngineManager.swift`:

```swift
enum Provider: String, CaseIterable, Identifiable {
    case slack = "Slack"
    case googleWorkspace = "Google Workspace"
    case github = "GitHub"
    case yourService = "YourService"  // Add this
    
    var id: String { rawValue }
    
    var cliName: String {
        switch self {
        // ... existing cases ...
        case .yourService: return "yourservice"
        }
    }
}
```

### Step 2: Add Provider Color

In `Theme/CityPopTheme.swift`:

```swift
static func providerColor(for name: String) -> Color {
    switch name {
    // ... existing cases ...
    case "YourService": return Color(red: 0.5, green: 0.5, blue: 0.8)
    default: return accent
    }
}
```

### Step 3: Create Config Sheet

Create `Views/YourServiceConfigSheet.swift` (copy from `GitHubConfigSheet.swift`):

```swift
struct YourServiceConfigSheet: View {
    let onComplete: () -> Void
    let onCancel: () -> Void
    
    @State private var apiKey: String = ""
    @State private var isValidating = false
    
    var body: some View {
        VStack(spacing: 20) {
            Text("Connect YourService")
                .font(.system(size: 18, weight: .semibold))
            
            // API key input
            SecureField("API Key", text: $apiKey)
                .textFieldStyle(.roundedBorder)
            
            // Instructions
            Text("Get your API key from YourService settings")
                .font(.caption)
                .foregroundColor(.secondary)
            
            // Buttons
            HStack {
                Button("Cancel") { onCancel() }
                Button("Connect") { saveAndConnect() }
                    .disabled(apiKey.isEmpty)
            }
        }
        .padding()
        .frame(width: 400)
    }
    
    private func saveAndConnect() {
        // Save to Keychain
        KeychainHelper.save(
            service: "minna_ai",
            account: "yourservice_api_key",
            data: apiKey.data(using: .utf8)!
        )
        onComplete()
    }
}
```

### Step 4: Wire Up Config Sheet

In `ControlCenterView.swift`, add to `providerConfigSheet()`:

```swift
case .yourService:
    YourServiceConfigSheet(
        onComplete: {
            showingConfigSheet = nil
            engine.triggerSync(for: provider)
        },
        onCancel: {
            showingConfigSheet = nil
        }
    )
```

---

## Code Style

### Python

- Use type hints
- Format with `black`
- Sort imports with `isort`
- Docstrings for public functions

```bash
# Format code
poetry run black src/
poetry run isort src/
```

### Swift

- Follow Apple's Swift API Design Guidelines
- Use SwiftUI idioms
- Keep views small and composable
- Use `CityPopTheme` for all styling

---

## Pull Requests

1. Fork the repo
2. Create a feature branch (`git checkout -b feature/linear-connector`)
3. Make your changes
4. Run tests (`poetry run pytest`)
5. Format code (`poetry run black src/`)
6. Commit with a clear message
7. Push and open a PR

### PR Checklist

- [ ] Tests pass
- [ ] Code is formatted
- [ ] New connector has both `sync()` and `discover()` implemented
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
- Tag `@maboroshi` for connector reviews
- See [README.md](README.md) for architecture overview

Thanks for contributing! 🎉
