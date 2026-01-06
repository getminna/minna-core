# Minna Core

Local context engine for AI. Syncs Slack, Google Workspace, and GitHub to a local vector DB, then exposes it via MCP so your AI tools can search your work history.

Works with Claude, Cursor, ChatGPT, and anything that speaks MCP.

Everything runs on your machine. No cloud, no SaaS, no fees or token burn.

## Quickstart

```bash
git clone https://github.com/minna-ai/minna-core.git
cd minna-core
poetry install

# sync your slack
python -m minna.cli sync --provider slack

# start the MCP server
python -m minna.mcp_server
```

Then point your AI at it. Example for Claude Desktop (`~/Library/Application Support/Claude/claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "minna": {
      "command": "python",
      "args": ["-m", "minna.mcp_server"],
      "cwd": "/path/to/minna-core"
    }
  }
}
```

Now your AI knows about that Slack thread from last week.

---

## Why?

LLMs are goldfish. Brilliant reasoning, zero recall. Your AI can architect distributed systems but has no idea your team deprecated that API last Tuesday.

Minna syncs your actual work context—the decisions made in Slack, the meetings on your calendar, the PRs on GitHub—into a local embeddings database. When you ask a question, your AI can pull from months of context instead of just your current prompt.

We run everything locally because your Slack DMs with your cofounder about fundraising shouldn't live on someone else's server.

---

## Connecting Your Tools

### Slack

You create your own Slack app (we don't use a shared app—that would defeat the point):

1. [api.slack.com/apps](https://api.slack.com/apps) → Create New App → From Manifest
2. Paste the manifest below, install to workspace
3. Grab your `xoxp-` token

<details>
<summary><strong>Slack Manifest</strong></summary>

```yaml
display_information:
  name: Minna Local
  description: Local-first AI context engine
features:
  bot_user:
    display_name: Minna
    always_online: false
oauth_config:
  scopes:
    user:
      - channels:history
      - channels:read
      - groups:history
      - groups:read
      - im:history
      - im:read
      - mpim:history
      - mpim:read
      - users:read
      - search:read
      - team:read
    bot:
      - channels:history
      - channels:read
      - groups:history
      - groups:read
      - im:history
      - im:read
      - mpim:history
      - mpim:read
      - users:read
settings:
  org_deploy_enabled: false
  socket_mode_enabled: false
  token_rotation_enabled: false
```
</details>

### Google Workspace

1. [Google Cloud Console](https://console.cloud.google.com) → Create project
2. Enable Calendar API + Gmail API
3. Create OAuth credentials (Desktop app), redirect URI: `http://127.0.0.1:8847/callback`
4. Enter Client ID/Secret in Minna

OAuth happens entirely on your machine.

### GitHub

[Fine-grained PAT](https://github.com/settings/tokens?type=beta) with read access.

---

## macOS App

Prefer clicking to typing? Grab the app from [Releases](https://github.com/minna-ai/minna-core/releases).

> **Note**: Unsigned—right-click → Open to bypass Gatekeeper.

---

## How It Works

| Component | What | Why |
|-----------|------|-----|
| Connectors | Python workers for Slack/Google/GitHub | Fetch via APIs |
| Embeddings | FastEmbed | Local, no API calls |
| Vector DB | SQLite + sqlite-vec | No server, just a file |
| Credentials | macOS Keychain | System encryption |
| Protocol | MCP | Works with Claude, Cursor, ChatGPT, etc. |

---

## Project Layout

```
src/
├── minna/                  # Python backend
│   ├── cli.py              # CLI entry point
│   ├── mcp_server.py       # MCP server
│   ├── ingestion/          # Connectors
│   └── core/vector_db.py   # Embeddings + storage
└── minna_ep1/              # macOS SwiftUI app
```

---

## Status

**Working**
- [x] Slack, Google Workspace, GitHub sync
- [x] Local OAuth (you provide your own credentials)
- [x] MCP server (Claude, Cursor, ChatGPT, etc.)
- [x] macOS app

**Coming**
- [ ] `pip install minna-core`
- [ ] `brew install minna`
- [ ] Signed macOS builds
- [ ] Linear, Notion connectors

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). We'd love help with:
- New connectors
- Windows/Linux support
- Security review

## License

MIT

---

<p align="center">
  <strong>minna</strong> (皆) — everyone
</p>
