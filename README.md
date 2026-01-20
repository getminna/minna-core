# Minna

Your AI's memory. Local-first. Zero config.

[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)
[![Built with Rust](https://img.shields.io/badge/Built%20with-Rust-orange.svg)](https://www.rust-lang.org/)
[![MCP Native](https://img.shields.io/badge/MCP-Native-purple.svg)](https://modelcontextprotocol.io)
[![Local-first](https://img.shields.io/badge/Local--first-always-blue.svg)]()
[![No telemetry](https://img.shields.io/badge/Telemetry-none-lightgrey.svg)]()

-----

## 30 Seconds to Memory

```bash
brew install getminna/tap/minna-core
minna add slack linear github
```

That's it. Your AI can now remember your work.

-----

## What is Minna?

Minna is the persistent memory layer for your AI agents.

Standard MCP Servers: Live, stateless lookups. The AI has to know exactly what to ask for.

Minna: Proactive, stateful indexing. The AI just asks a question, and Minna finds the relevant needle in the haystack of your Slack history and Linear tickets.

**Before:**
```
┌─────────────────┐                        ┌─────────────────┐
│                 │  "status of Atlas?"    │                 │
│  You            │ ─────────────────────► │  Claude/Cursor  │
│                 │ ◄───────────────────── │                 │
└─────────────────┘  "I don't have info    └─────────────────┘
                      on that. Let me
                      search the web..."

                      [wastes 10k tokens on nothing]
```

**With Minna:**
```
┌─────────────────┐                        ┌─────────────────┐
│                 │  1. "status of Atlas?" │                 │
│  You            │ ─────────────────────► │  Claude/Cursor  │
│                 │ ◄───────────────────── │                 │
└─────────────────┘  5. [progress /        └────────┬───▲────┘
                        blockers /                  │   │
                        action items]      2. query │   │ 4. context
                                                    ▼   │
                                           ┌─────────────────┐
                                           │  Minna          │
                                           │  (local daemon) │
                                           └────────┬────────┘
                                                    │
                                              3. retrieve
                                                    │
                                    ┌───────────────┼───────────────┐
                                    ▲               ▲               ▲
                               ┌────────┐     ┌────────┐     ┌────────┐
                               │ Linear │     │ Slack  │     │ GitHub │
                               └────────┘     └────────┘     └────────┘
```

Your data stays on your machine. Always.

-----

## Why Minna?

**Fast.** Single Rust binary. Sub-millisecond queries over [Unix Domain Socket](https://en.wikipedia.org/wiki/Unix_domain_socket). No Electron. No JVM cold starts. No stdio overhead—your agent connects directly to a persistent daemon.

**Smart retrieval.** Zero-config RAG pipeline powered by [Nomic Embed v1.5](https://huggingface.co/nomic-ai/nomic-embed-text-v1.5) with an 8K context window—built for real work conversations, not empty snippets. Hybrid search combines [SQLite-vec](https://github.com/asg017/sqlite-vec) semantic search with [FTS5](https://www.sqlite.org/fts5.html) full-text search. No reranker. No external API calls. Just results.

**MCP-native.** Built for the [Model Context Protocol](https://modelcontextprotocol.io) from day one. Your agent calls Minna like any other tool—first-class memory for Claude, Cursor, and any MCP-compatible client. This is what Glean should have been.

**Fast-Path Router.** When your agent requests a specific URL (a Linear issue, a Notion page, a Slack thread), Minna fetches it directly from the API instead of searching the index. Real-time data when you need it, cached results when you don't.

**Secure by default.** Credentials stored in macOS Keychain. Tokens never hit disk unencrypted. Your data stays sovereign.

**Unix philosophy.** Daemon + CLI + socket. Pipe it, script it, cron it. No GUI required.

-----

## Installation

```bash
brew install getminna/tap/minna-core
```

-----

## Connect Your Sources

### Interactive

```bash
minna add
```

Select the sources you use. Minna walks you through each one.

### Explicit

```bash
minna add slack linear github
```

Connects all three in sequence.

### What Happens

Each source uses a personal token you create. Minna walks you through getting it.

```
$ minna add linear

  To connect Linear, you'll need an API key.

  1. Go to: https://linear.app/settings/api
  2. Create a new Personal API Key
  3. Copy the key

? Paste your Linear API key: ████████████████

✔ Connected to Linear (Acme Corp)

⚡ Sprint Sync...  ████████████████████  142 issues

💤 Deep sync running in background (90 days of history).
   Run `minna status` to check progress.
```

-----

## Connect Your AI

After your first source syncs, Minna asks which AI tool you use:

```
? Which AI tool do you use?
  → Cursor
    Claude Code
    VS Code + Continue
    Windsurf
    Other / Manual
```

Select your tool. Minna finds the config file and adds itself:

```
✔ Found ~/.cursor/mcp.json
? Add Minna to Cursor? (Y/n) y

✔ Done. Restart Cursor to activate.
```

No JSON to copy. No config to edit. Just restart your editor.

-----

## Test the Signal

Once setup completes:

```
────────────────────────────────────────────────────────
  ✔ Ready.

  Copied to clipboard:

    What's the status of Project Atlas?

  Paste into chat (⌘V) and hit Enter.
────────────────────────────────────────────────────────
```

Paste it. Watch your AI remember.

-----

## Commands

|Command                 |What it does                              |
|------------------------|------------------------------------------|
|`minna add [sources...]`|Connect sources (interactive or explicit) |
|`minna remove <source>` |Disconnect a source                       |
|`minna sync [sources...]`|Fetch latest data from sources           |
|`minna status`          |Show sources, sync progress, daemon health|
|`minna setup [tool]`    |Configure MCP for your AI tool            |
|`minna daemon status`   |Check if daemon is running                |
|`minna daemon restart`  |Restart the background daemon             |
|`minna daemon logs`     |Tail daemon logs                          |

-----

## Supported Sources

|Source       |Auth Method                              |
|-------------|-----------------------------------------|
|Slack        |User OAuth Token (`xoxp-...`)            |
|Linear       |Personal API Key                         |
|GitHub       |Fine-grained PAT                         |
|Notion       |Internal Integration Token               |
|Atlassian    |API Token (id.atlassian.com)             |
|Google Drive |Your OAuth App (client_id + secret)      |

Each source syncs via async [Tokio](https://tokio.rs/) workers. Backfill 90 days in minutes, not hours.

More coming. [Request a source →](https://github.com/getminna/minna-core/issues)

-----

## How It Works

Minna runs as a background daemon on your machine.

When you install, Minna registers with macOS to start automatically on login. No terminal tab required. If something goes wrong:

```bash
minna daemon restart
```

Your AI connects to Minna via [MCP](https://modelcontextprotocol.io) (Model Context Protocol). Queries go over Unix Domain Socket—no stdio overhead, no process spawning per request. When you ask a question, the agent calls Minna's search tool, gets relevant context, and synthesizes the answer.

All data is stored locally:

```
~/.config/minna/config.toml    # Your settings
~/.local/share/minna/db/       # Vector store + raw text
~/.minna/mcp.sock              # Unix socket for MCP
~/.cache/minna/logs/           # Daemon logs
```

No cloud. No telemetry. Your credentials live in macOS Keychain.

-----

## Troubleshooting

**Minna isn't responding to my AI**

```bash
minna status
```

If the daemon isn't running:

```bash
minna daemon restart
```

**My source isn't syncing**

```bash
minna status
```

Check the sync status. If stuck:

```bash
minna daemon logs
```

**I need to re-authenticate a source**

```bash
minna add slack  # Re-run for any source
```

-----

## Uninstall

```bash
brew uninstall minna-core
rm -rf ~/.config/minna ~/.local/share/minna ~/.minna ~/.cache/minna
```

Credentials are removed from Keychain automatically.

-----

## Philosophy

1. **Local-first.** Your data never leaves your machine.
2. **Sovereign credentials.** You own the OAuth apps. You control access.
3. **Zero telemetry.** We don't know what you search. We don't want to.
4. **Unix philosophy.** One tool, one job. Composable. Scriptable.

-----

## License

MIT

-----

<p align="center">
  <i>Minna. Memory for everyone.</i>
</p>
