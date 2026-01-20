# Architecture Decision: Admin Socket vs MCP-Only

**Status**: Recommendation
**Date**: 2026-01-20
**Context**: CLI-first refactor

---

## The Question

Should the daemon expose:
- **Option A**: Two sockets (admin.sock + mcp.sock) — current approach
- **Option B**: Single MCP socket with admin tools
- **Option C**: No admin socket; CLI manages state directly

---

## Current Architecture (Two Sockets)

```
┌──────────────┐                    ┌──────────────┐
│  minna CLI   │                    │  Claude/     │
│  (control)   │                    │  Cursor      │
└──────┬───────┘                    └──────┬───────┘
       │                                   │
       │ admin.sock                        │ mcp.sock
       │ (JSON-RPC)                        │ (MCP protocol)
       ▼                                   ▼
┌─────────────────────────────────────────────────────┐
│                    Minna Daemon                     │
│                                                     │
│  Admin Handler          │         MCP Handler       │
│  - sync_provider        │         - get_context     │
│  - verify_credentials   │         - read_resource   │
│  - get_status           │                           │
│  - reset                │                           │
└─────────────────────────────────────────────────────┘
```

**Pros**:
- Clear separation: AI tools can only read, CLI can control
- Security: An LLM can't accidentally trigger `reset` or credential changes
- Protocol clarity: MCP stays pure for tool calls

**Cons**:
- Two protocols to maintain
- Two sockets to manage
- Complexity you experienced when implementing

---

## Option B: MCP-Only with Admin Tools

```
┌──────────────┐          ┌──────────────┐
│  minna CLI   │          │  Claude      │
│  (MCP client)│          │  (MCP client)│
└──────┬───────┘          └──────┬───────┘
       │                         │
       └───────────┬─────────────┘
                   │ mcp.sock
                   ▼
┌─────────────────────────────────────────┐
│              Minna Daemon               │
│                                         │
│  Tools:                                 │
│  - get_context (all clients)            │
│  - read_resource (all clients)          │
│  - admin_sync (CLI only?)               │
│  - admin_status (CLI only?)             │
│  - admin_reset (CLI only?)              │
└─────────────────────────────────────────┘
```

**Pros**:
- Single protocol
- CLI is just another MCP client
- Simpler mental model

**Cons**:
- How do you restrict admin tools to CLI only? MCP doesn't have auth
- LLMs might try to call admin tools (prompt injection risk)
- MCP wasn't designed for control plane operations
- You'd be inventing "privileged MCP tools" — non-standard

---

## Option C: CLI Manages State Directly

```
┌──────────────────────────────────────────────────────┐
│                      minna CLI                       │
│                                                      │
│  - Writes credentials to Keychain directly           │
│  - Writes config to ~/.config/minna/config.toml     │
│  - Sends SIGHUP to daemon to reload                  │
└──────────────────────────────────────────────────────┘
                          │
                       SIGHUP
                          ▼
┌──────────────────────────────────────────────────────┐
│                    Minna Daemon                      │
│                                                      │
│  - Watches config file for changes                   │
│  - On SIGHUP: re-reads config, restarts syncs        │
│  - Serves MCP queries on mcp.sock                    │
└──────────────────────────────────────────────────────┘
```

**Pros**:
- Daemon is simpler ("do syncs, serve queries")
- No admin protocol needed
- Unix-y (signals, config files)

**Cons**:
- CLI must understand daemon internals (credential format, sync state)
- Race conditions: CLI writes config while daemon is reading
- Limited expressiveness: can't easily get real-time sync progress
- No request/response for admin operations

---

## Recommendation: Keep Two Sockets, Hide the Complexity

The two-socket architecture is correct. The confusion came from the **Swift app exposing the complexity**, not from the architecture itself.

With a CLI-first approach, the admin socket becomes an **implementation detail**:

```
User types:        minna add slack
                        │
CLI does:          1. Spawns browser for OAuth
                   2. Captures redirect, gets token
                   3. Stores token in Keychain
                   4. Connects to admin.sock
                   5. Sends: {"method": "sync", "params": {"provider": "slack"}}
                   6. Streams progress to terminal
                        │
User sees:         ⚡ Syncing Slack...  ████████████████  142 messages
```

The user never thinks about sockets. They just run `minna add slack`.

### Why This Works

1. **Security stays intact**: AI tools only access mcp.sock (read-only)
2. **Protocol stays clean**: MCP for queries, JSON-RPC for control
3. **Complexity is hidden**: CLI abstracts the admin socket entirely
4. **Debugging stays possible**: Power users can `echo '{"method":"ping"}' | nc -U ~/.minna/admin.sock`

### Simplification: Merge the Sockets into One File

You could even put both listeners on the same path with different protocols:

```
~/.minna/minna.sock   # Single socket, daemon detects protocol from first bytes
```

But this adds detection complexity for marginal benefit. Two sockets is fine.

---

## What Confused You Before

The Swift app made the admin socket **visible** to users:
- Users saw "Admin Channel" in logs
- UI state depended on admin socket responses
- Process management was coupled to socket health

With CLI-first, none of this is exposed. The CLI is a thin wrapper that:
1. Parses commands
2. Does local work (OAuth, Keychain)
3. Sends RPC to admin socket when needed
4. Formats output prettily

The admin socket is just plumbing, like how `git` uses pack protocols internally.

---

## Implementation Notes

### Admin Socket Commands (Minimal Set)

```
ping                    → {"ok": true}
status                  → {"sources": [...], "daemon": {...}}
sync {provider}         → streams progress events
verify {provider}       → {"valid": true/false}
reset {provider}        → {"ok": true}
```

### CLI Owns These Responsibilities

- OAuth flows (browser, redirect capture)
- Keychain reads/writes
- Config file management
- launchd registration
- MCP config file updates (for AI tools)

### Daemon Owns These Responsibilities

- Sync workers (Tokio tasks)
- Vector store
- MCP query handling
- Background sync scheduling

---

## Decision

**Keep the two-socket architecture.** The admin socket is an implementation detail that the CLI hides from users.

The README's promise holds:
```bash
minna add slack
```

That's it. User doesn't know or care about sockets.
