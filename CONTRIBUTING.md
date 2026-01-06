# Contributing to Minna Core

Thanks for your interest in contributing to Minna! This document outlines how to get started.

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

## Project Structure

```
src/
├── minna/                  # Python backend (connectors, MCP server)
│   ├── cli.py              # CLI entry point
│   ├── mcp_server.py       # MCP protocol implementation
│   ├── core/               # Vector DB, embeddings
│   ├── ingestion/          # Connector implementations
│   └── workers/            # Sync orchestration
│
└── minna_ep1/              # macOS SwiftUI app
    ├── Views/              # UI components
    ├── Services/           # OAuth, Keychain helpers
    └── Theme/              # City Pop design system
```

## How to Contribute

### Reporting Bugs

1. Check if the issue already exists
2. Include your OS version, Python version, and steps to reproduce
3. Attach relevant logs (sanitize any tokens/credentials!)

### Suggesting Features

Open an issue with the `enhancement` label. Describe:
- The problem you're trying to solve
- Your proposed solution
- Any alternatives you've considered

### Pull Requests

1. Fork the repo
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Make your changes
4. Run tests (`poetry run pytest`)
5. Commit with a clear message
6. Push and open a PR

### Adding a New Connector

We'd love help adding connectors for:
- Linear
- Notion
- Obsidian
- Discord
- Microsoft 365

To add a connector:

1. Create `src/minna/ingestion/yourservice.py`:

```python
from .base import BaseConnector, Document

class YourServiceConnector(BaseConnector):
    def __init__(self, token: str, progress_callback=None):
        self.token = token
        self.progress_callback = progress_callback or (lambda *a, **k: None)
    
    def sync(self, days_back: int = 14) -> list[Document]:
        # Fetch data from API
        # Convert to Document objects
        # Return list of documents
        pass
```

2. Create `src/minna/workers/yourservice_worker.py`:

```python
import keyring
from minna.core.vector_db import VectorManager
from minna.ingestion.yourservice import YourServiceConnector

class YourServiceWorker:
    KEYCHAIN_SERVICE = "minna_ai"
    KEYCHAIN_ACCOUNT = "yourservice_token"
    
    def __init__(self, progress_callback=None):
        self.progress_callback = progress_callback or (lambda *a, **k: None)
        token = keyring.get_password(self.KEYCHAIN_SERVICE, self.KEYCHAIN_ACCOUNT)
        if not token:
            raise ValueError("No token found in Keychain")
        self.connector = YourServiceConnector(token, self.progress_callback)
        self.db = VectorManager()
    
    def sync(self):
        documents = self.connector.sync()
        self.db.add_documents(documents)
```

3. Add to `src/minna/cli.py`
4. (Optional) Add Swift UI in `src/minna_ep1/Views/`

## Code Style

### Python

- Use type hints
- Format with `black`
- Sort imports with `isort`
- Docstrings for public functions

### Swift

- Follow Apple's Swift API Design Guidelines
- Use SwiftUI idioms
- Keep views small and composable

## Security

**Never commit credentials or tokens.**

If you find a security vulnerability, please email security@minna.ai instead of opening a public issue.

## License

By contributing, you agree that your contributions will be licensed under the MIT License.

---

Questions? Open an issue or reach out on Twitter [@maboroshi](https://twitter.com/maboroshi).

