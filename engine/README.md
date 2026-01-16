# minna-core

**The distributed memory layer for AI agents.**

minna-core is a headless Rust engine that provides:
- **MCP Server**: Model Context Protocol interface for AI agents
- **Vector DB**: sqlite-vec powered semantic search
- **Ingestion Workers**: Connectors for Slack, Google Workspace, GitHub, and more

## Installation

### From Source

```bash
git clone https://github.com/anthropics/minna.git
cd minna/engine
cargo build --release
```

The binary will be at `target/release/minna-core`.

### Homebrew

```bash
brew install minna
```

## Usage

### As a Daemon (Unix Socket)

```bash
# Start the MCP server on a Unix socket
minna-core --socket ~/Library/Application\ Support/Minna/mcp.sock
```

### With stdio (for direct MCP integration)

```bash
# Start in stdio mode for piping
minna-core --stdio
```

## MCP Tools

minna-core exposes the following MCP tools:

| Tool | Description |
|------|-------------|
| `sync_provider` | Sync data from a provider (slack, google, github) |
| `search` | Semantic search across all indexed content |
| `get_context` | Get context for a specific topic/query |
| `discover` | Discover available channels/resources for a provider |

## Configuration

Configuration is stored in `~/.config/minna/config.toml`:

```toml
[database]
path = "~/.local/share/minna/minna.db"

[providers.slack]
token = "xoxb-..."

[providers.google]
client_id = "..."
client_secret = "..."
```

## Architecture

```
minna-core (binary)
├── minna-mcp      # MCP protocol implementation
├── minna-core     # Core business logic
├── minna-vector   # Vector DB (sqlite-vec)
├── minna-ingest   # Data ingestion workers
└── minna-auth-bridge  # OAuth handling
```

## License

MIT License - see [LICENSE](LICENSE)
