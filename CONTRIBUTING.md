# Contributing to Minna Core

Thanks for your interest in contributing to Minna! This guide will help you get started.

## Table of Contents

- [Development Setup](#development-setup)
- [Project Structure](#project-structure)
- [Adding a New Connector](#adding-a-new-connector)
- [Code Style](#code-style)
- [Pull Requests](#pull-requests)
- [Security](#security)

---

## Development Setup

### Prerequisites

- Python 3.11+
- Poetry (`pip install poetry`)
- macOS 14+ (for SwiftUI app development)
- Xcode 15+ (for macOS app)
- XcodeGen (`brew install xcodegen`)

### Getting Started

```bash
# Clone the repo
git clone https://github.com/minna-ai/minna-core.git
cd minna-core

# Install Python dependencies
poetry install

# Run tests
poetry run pytest

# Start the MCP server locally
poetry run python -m minna.mcp_server
```

### Building the macOS App

```bash
cd src/minna_ep1
xcodegen generate
open MinnaEP1.xcodeproj
# Build with Cmd+B in Xcode
```

---

## Project Structure

```
src/
├── minna/                      # Python backend
│   ├── cli.py                  # CLI entry point
│   ├── scaffold.py             # Connector generator
│   ├── mcp_server.py           # MCP protocol
│   ├── core/
│   │   ├── schema.sql          # Database schema
│   │   └── vector_db.py        # VectorManager
│   ├── ingestion/
│   │   ├── base.py             # BaseConnector, Document
│   │   ├── _template.py        # Connector template
│   │   ├── slack.py            # SlackConnector
│   │   ├── google.py           # GoogleWorkspaceConnector
│   │   └── github.py           # GitHubConnector
│   └── workers/
│       ├── slack_worker.py     # SlackWorker
│       ├── google_worker.py    # GoogleWorker
│       └── github_worker.py    # GitHubWorker
│
└── minna_ep1/                  # macOS SwiftUI app
    ├── MinnaEP1App.swift       # App entry
    ├── MinnaEngineManager.swift # Process orchestration
    ├── KeychainHelper.swift    # Credential storage
    ├── Views/                  # UI components
    ├── Services/               # OAuth, helpers
    └── Theme/                  # City Pop design system
```

---

## Adding a New Connector

We'd love help adding connectors for:
- ⭐ Linear (high priority)
- Notion
- Obsidian (local files)
- Discord
- Microsoft 365
- Jira
- Confluence

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
