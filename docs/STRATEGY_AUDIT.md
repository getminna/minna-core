# Minna Strategy Audit: Mac App vs CLI-First

**Date**: 2026-01-20
**Context**: Comparing current implementation to new CLI-first README vision

---

## Executive Summary

The current codebase is heavily invested in a **Mac app-first approach** (~10,068 lines Swift), while the new README vision describes a **CLI-first, Unix-philosophy approach**. This audit identifies the gaps and what needs to change.

**Key Finding**: The Rust engine is solid and mostly aligned with the new vision. The Swift app represents significant effort that would become secondary or removed entirely under the new strategy.

---

## Architecture Comparison

### Current Implementation

```
┌──────────────────────────────────────────────────────┐
│                    macOS App (Swift)                 │
│         ~10,068 lines • City Pop UI • OAuth         │
│                  "MinnaEngineManager"                │
└─────────────────────────┬────────────────────────────┘
                          │ spawns & controls
                          ▼
┌──────────────────────────────────────────────────────┐
│                  Rust Daemon                         │
│           Two sockets: admin.sock + mcp.sock         │
└──────────────────────────────────────────────────────┘
                          │
        ┌─────────────────┼─────────────────┐
        ▼                 ▼                 ▼
   ┌────────┐       ┌────────┐       ┌────────┐
   │ Slack  │       │ GitHub │       │ Google │
   └────────┘       └────────┘       └────────┘
```

### New Vision

```
┌─────────────┐     ┌─────────────┐
│  minna CLI  │     │  AI Tool    │
│  (control)  │     │  (Claude)   │
└──────┬──────┘     └──────┬──────┘
       │                   │
       │              MCP over UDS
       ▼                   ▼
┌──────────────────────────────────────┐
│         Rust Daemon (single socket)  │
│         ~/.minna/mcp.sock            │
│         launchd-managed              │
└──────────────────────────────────────┘
```

---

## Detailed Comparison

### 1. Distribution & Installation

| Aspect | Current | New Vision | Gap |
|--------|---------|------------|-----|
| Install method | Clone repo, build Xcode | `brew install minna-ai/tap/minna` | **Major**: No Homebrew formula, no single binary |
| Binary | Rust lib embedded in .app bundle | Standalone Rust binary | Need to build standalone CLI |
| First-run | Launch .app, GUI wizard | `minna add linear` | CLI onboarding doesn't exist |

### 2. CLI Commands

| New Vision Command | Current State | What's Missing |
|--------------------|---------------|----------------|
| `minna add [sources...]` | Only GUI config sheets | **Entire CLI flow**: interactive prompts, OAuth browser flow, credential capture |
| `minna status` | Admin socket returns JSON | CLI binary to call admin socket and format output |
| `minna setup [tool]` | MCPSetupWizard in Swift | Auto-detect config files, write JSON, handle multiple AI tools |
| `minna daemon status` | Process managed by Swift app | launchd integration, CLI status check |
| `minna daemon restart` | App restarts process | launchd plist, `launchctl` wrapper |
| `minna daemon logs` | Logs go to Swift app | Log to `~/.cache/minna/logs/`, CLI to tail |

### 3. Source Configuration

| Source | New Vision Auth | Current Auth | Gap |
|--------|-----------------|--------------|-----|
| Slack | Browser OAuth | Browser OAuth (via Swift) | Need CLI to spawn browser, capture redirect |
| Linear | Browser OAuth | API key input in GUI | **New**: Need OAuth flow, not just API key |
| GitHub | Browser OAuth or PAT | PAT input in GUI | Need browser OAuth option |
| Notion | Browser OAuth | **Not implemented** | **New source entirely** |
| Atlassian | Your OAuth App creds | **Not implemented** | **New source entirely** |
| Google Drive | Your OAuth App creds | Calendar + Gmail (not Drive) | Different Google API scope |

### 4. Socket Architecture

| Aspect | Current | New Vision | Impact |
|--------|---------|------------|--------|
| Number of sockets | 2 (mcp.sock + admin.sock) | 1 (mcp.sock only?) | Either keep 2 or merge control into MCP |
| MCP socket path | `~/Library/Application Support/Minna/mcp.sock` | `~/.minna/mcp.sock` | Path change needed |
| Protocol | Admin uses custom JSON-RPC | MCP only | Decide: keep admin socket or add control tools to MCP? |

### 5. Data Paths

| Data Type | Current Path | New Vision Path | Notes |
|-----------|--------------|-----------------|-------|
| Config | `~/Library/.../Minna/` | `~/.config/minna/config.toml` | XDG-style for cross-platform |
| Database | `~/Library/.../Minna/minna.db` | `~/.local/share/minna/db/` | Linux-friendly |
| Socket | `~/Library/.../Minna/mcp.sock` | `~/.minna/mcp.sock` | Shorter, memorable |
| Logs | Swift app console | `~/.cache/minna/logs/` | Persistent, tailable |

### 6. Daemon Management

| Aspect | Current | New Vision | Gap |
|--------|---------|------------|-----|
| Startup | Swift app spawns `minna-core` binary | launchd runs on login | Need launchd plist, install logic |
| Lifecycle | App owns process | Self-managed daemon | Daemon must survive terminal close |
| Health check | App monitors stderr | `minna daemon status` | CLI health check command |
| Logs | Captured by app | File-based, rotated | Log rotation, `minna daemon logs` |

### 7. AI Tool Integration (minna setup)

| AI Tool | New Vision | Current | Gap |
|---------|------------|---------|-----|
| Cursor | Auto-detect `~/.cursor/mcp.json` | MCPSetupWizard shows manual steps | Auto-detect + auto-write |
| Claude Code | Auto-detect config | Same | Auto-detect + auto-write |
| VS Code + Continue | Auto-detect | Not mentioned | New target |
| Windsurf | Auto-detect | Not mentioned | New target |

New flow:
```
? Which AI tool do you use?
  → Cursor

✔ Found ~/.cursor/mcp.json
? Add Minna to Cursor? (Y/n) y

✔ Done. Restart Cursor to activate.
```

### 8. User Experience Flow

**Current Flow (Mac App)**:
1. Download .app
2. Right-click → Open (unsigned)
3. City Pop UI appears
4. Click provider cards → OAuth sheets
5. Watch VFD-style sync animation
6. Open MCP Setup Wizard
7. Copy JSON manually

**New Vision Flow (CLI)**:
1. `brew install minna-ai/tap/minna`
2. `minna add linear` → browser opens → approve
3. `minna add slack github` → sequential auth
4. "Which AI tool?" → auto-configures
5. "Paste this: What's the status of Project Atlas?"
6. Done

---

## What Can Be Kept

### Rust Engine (✅ Mostly Aligned)

The Rust crates are solid and align with the vision:

| Crate | Status | Notes |
|-------|--------|-------|
| `minna-core` | ✅ Keep | Core sync logic is good |
| `minna-mcp` | ✅ Keep | MCP handler works |
| `minna-vector` | ✅ Keep | sqlite-vec + FTS5 is exactly the vision |
| `minna-ingest` | ✅ Keep | Document storage works |
| `minna-auth-bridge` | ✅ Keep | Keychain integration is good |
| `minna-server` | ⚠️ Modify | Currently expects Swift app control |

**Changes needed to Rust**:
- Add CLI binary (`minna`) with subcommands
- Remove dependency on admin socket for basic operation
- Add launchd/systemd registration
- Support XDG paths (cross-platform)
- Self-daemonize or work with system init

### Swift App (⚠️ Secondary or Deprecated)

Under CLI-first strategy, the Swift app becomes:
- **Option A**: Deprecated entirely (Unix philosophy)
- **Option B**: Nice-to-have GUI wrapper around CLI
- **Option C**: Separate "Minna Desktop" project

The ~10,068 lines of Swift code represent significant investment in:
- City Pop UI design system
- OAuth flow management
- Process orchestration

**If keeping the app**, it should call the CLI, not manage the daemon directly.

---

## What Needs to Be Built

### New Rust CLI Binary

```
minna
├── add [sources...]      # Interactive or explicit source setup
├── status                # Show sources, sync progress, daemon
├── setup [tool]          # Configure AI tool MCP
├── daemon
│   ├── status            # Is daemon running?
│   ├── start             # Start if not running
│   ├── restart           # Kill and restart
│   └── logs              # Tail log file
└── remove <source>       # Disconnect a source
```

**Estimated scope**: ~2,000-3,000 lines of Rust

### New Sources

| Source | Effort | Notes |
|--------|--------|-------|
| Notion | Medium | OAuth flow + blocks API |
| Atlassian (Jira/Confluence) | Medium | Self-hosted OAuth setup |
| Google Drive | Low | Already have Google OAuth, add Drive scope |

### Distribution

| Item | Effort | Notes |
|------|--------|-------|
| Homebrew formula | Low | Rust binary → tap |
| launchd plist | Low | Standard XML |
| Path migration | Medium | Support both old and new paths |

---

## Recommended Migration Path

### Phase 1: CLI Binary
1. Create `minna` CLI binary in Rust
2. Implement `minna add` with browser OAuth
3. Implement `minna status`
4. Implement `minna daemon` subcommands
5. Add launchd registration

### Phase 2: Distribution
1. Create Homebrew formula
2. Test `brew install` flow
3. Update XDG paths
4. Cross-platform path detection

### Phase 3: AI Tool Setup
1. Implement `minna setup` with auto-detection
2. Support Cursor, Claude Code, VS Code, Windsurf
3. Add "test the signal" clipboard flow

### Phase 4: New Sources
1. Add Notion connector
2. Add Atlassian connector
3. Add Linear OAuth (currently API key only)

### Phase 5: Decide on Mac App
- **Option A**: Archive Swift code, CLI-only
- **Option B**: Refactor app to call CLI
- **Option C**: Separate repo for GUI

---

## Quantified Impact

| Metric | Current | After Migration |
|--------|---------|-----------------|
| Install steps | 5+ (clone, Xcode, build, open, auth) | 2 (`brew install`, `minna add`) |
| Lines of Swift | ~10,068 | 0 (or separate project) |
| Lines of Rust | ~4,531 | ~7,000 (add CLI) |
| Platforms | macOS only | macOS, Linux, (Windows later) |
| Time to first query | ~10 minutes | ~30 seconds |

---

## Questions to Resolve

1. **Keep admin socket?** Or merge control into MCP tools?
2. **Mac app fate?** Archive, refactor, or separate project?
3. **Existing user migration?** Auto-migrate `~/Library/...` → `~/.config/...`?
4. **Notion priority?** New README lists it as supported
5. **Linear OAuth vs API key?** New README says "Browser (OAuth)"

---

## Conclusion

The strategic pivot from Mac app-first to CLI-first requires:

1. **New CLI binary** (~2,500 lines Rust)
2. **Homebrew formula**
3. **launchd integration**
4. **Path standardization** (XDG-style)
5. **Decision on Swift app** (~10K lines at stake)

The Rust engine is well-architected and mostly ready. The main work is building the CLI frontend and distribution mechanism.

The new vision is simpler, faster to onboard, more Unix-y, and cross-platform ready. The tradeoff is abandoning (or deprioritizing) the significant Swift investment.

---

*"Unix philosophy. Daemon + CLI + socket. Pipe it, script it, cron it. No GUI required."*
