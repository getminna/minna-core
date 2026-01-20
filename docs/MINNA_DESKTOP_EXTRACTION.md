# Minna Desktop Extraction Plan

**Status**: Planned
**Date**: 2026-01-20
**Context**: Separating Mac app into standalone project

---

## Overview

The Swift macOS app (~10,068 lines) will be extracted to a separate repository called **Minna Desktop**. This keeps the core CLI-first `minna` project focused while preserving the City Pop UI investment.

---

## What Gets Extracted

### Files to Move

```
src/minna_ep1/                          → minna-desktop/Sources/MinnaDesktop/
├── MinnaEP1App.swift                   → MinnaDesktopApp.swift (rename)
├── MinnaEngineManager.swift            → Keep (refactor to call CLI)
├── KeychainHelper.swift                → Keep
├── Services/
│   ├── SignalProvider.swift            → Keep
│   ├── SignalProviderWrapper.swift     → Keep
│   ├── MockSignalEngine.swift          → Keep (for development)
│   ├── RealSignalEngine.swift          → Refactor (use CLI instead of sockets)
│   ├── LocalOAuthManager.swift         → Remove (CLI handles OAuth)
│   └── MCPClient.swift                 → Keep (for status display)
├── Views/
│   ├── ControlCenterView.swift         → Keep
│   ├── TEProviderRow.swift             → Keep
│   ├── ConnectorSettingsSheet.swift    → Keep
│   ├── ProviderConfigSheet.swift       → Simplify (launches CLI)
│   ├── SlackConfigSheet.swift          → Remove (CLI handles)
│   ├── GitHubConfigSheet.swift         → Remove (CLI handles)
│   ├── MCPSetupWizard.swift            → Remove (CLI handles)
│   ├── MCPSourcesView.swift            → Keep
│   ├── FirstSyncSheet.swift            → Simplify
│   └── TEModifiers.swift               → Keep
├── Theme/
│   └── CityPopTheme.swift              → Keep (the good stuff)
├── Resources/                          → Keep
├── project.yml                         → Keep (update paths)
├── MinnaEP1.entitlements               → Keep
└── Info.plist                          → Keep
```

### Files That Stay in minna-core

```
engine/                                 # Rust daemon + CLI (stays)
├── crates/
│   ├── minna-server/                   # Daemon binary
│   ├── minna-cli/                      # NEW: CLI binary
│   ├── minna-core/                     # Core logic
│   ├── minna-mcp/                      # MCP handler
│   ├── minna-vector/                   # Vector store
│   ├── minna-ingest/                   # Document storage
│   └── minna-auth-bridge/              # Keychain integration
└── README.md
```

---

## Architectural Changes for Desktop

### Before (Current)

```
┌────────────────────────────────────────────────────┐
│                  Minna Desktop                     │
│                                                    │
│  MinnaEngineManager spawns daemon process          │
│  LocalOAuthManager handles all OAuth flows         │
│  RealSignalEngine talks to admin.sock directly     │
└────────────────────────────────────────────────────┘
```

### After (Extracted)

```
┌────────────────────────────────────────────────────┐
│                  Minna Desktop                     │
│                                                    │
│  Assumes `minna` CLI is installed (via Homebrew)   │
│  Calls `minna add slack` to trigger OAuth          │
│  Calls `minna status --json` to get state          │
│  Optionally reads mcp.sock for live status         │
└────────────────────────────────────────────────────┘
           │
           │ subprocess calls
           ▼
┌────────────────────────────────────────────────────┐
│                    minna CLI                       │
│                                                    │
│  Manages daemon lifecycle                          │
│  Handles OAuth (browser flows)                     │
│  Manages credentials                               │
└────────────────────────────────────────────────────┘
```

---

## Refactoring Required

### 1. MinnaEngineManager

**Current**: Spawns `minna-core` binary, monitors stderr, owns process lifecycle

**After**:
- Check if `minna` CLI is installed (`which minna`)
- If not installed, show "Install Minna first" screen with `brew install` instructions
- Call `minna daemon status` to check if running
- Call `minna daemon start` if needed
- No longer owns the process directly

### 2. RealSignalEngine

**Current**: Connects to admin.sock, sends JSON-RPC commands

**After**:
- Calls `minna status --json` subprocess
- Parses JSON output
- Optionally: Still connect to admin.sock for real-time updates (streaming)

### 3. OAuth Sheets (Remove)

**Current**: SlackConfigSheet, GitHubConfigSheet handle OAuth in-app

**After**:
- Single "Connect Source" button
- Calls `minna add slack` in subprocess
- CLI handles browser OAuth
- Desktop shows spinner until complete

### 4. Provider Config Sheets

**Current**: Complex forms for each provider

**After**:
- Simplified status display
- "Connect" button → `minna add {provider}`
- "Disconnect" button → `minna remove {provider}`
- "Re-sync" button → `minna sync {provider}`

---

## New Repository Structure

```
minna-desktop/
├── Sources/
│   └── MinnaDesktop/
│       ├── MinnaDesktopApp.swift
│       ├── MinnaCliWrapper.swift        # NEW: subprocess helper
│       ├── Services/
│       ├── Views/
│       ├── Theme/
│       └── Resources/
├── project.yml
├── Package.swift                        # SPM for dependencies
├── README.md
├── LICENSE
└── .github/
    └── workflows/
        └── build.yml
```

---

## MinnaCliWrapper (New)

```swift
import Foundation

actor MinnaCliWrapper {
    private let minnaPath: String

    init() throws {
        guard let path = Self.findMinna() else {
            throw MinnaError.cliNotInstalled
        }
        self.minnaPath = path
    }

    static func findMinna() -> String? {
        // Check common locations
        let paths = [
            "/opt/homebrew/bin/minna",
            "/usr/local/bin/minna",
            NSHomeDirectory() + "/.cargo/bin/minna"
        ]
        return paths.first { FileManager.default.fileExists(atPath: $0) }
    }

    func status() async throws -> MinnaStatus {
        let output = try await run(["status", "--json"])
        return try JSONDecoder().decode(MinnaStatus.self, from: output)
    }

    func addSource(_ source: String) async throws {
        // This opens a browser - we just wait for completion
        _ = try await run(["add", source])
    }

    func daemonStatus() async throws -> DaemonStatus {
        let output = try await run(["daemon", "status", "--json"])
        return try JSONDecoder().decode(DaemonStatus.self, from: output)
    }

    private func run(_ args: [String]) async throws -> Data {
        // Subprocess execution
    }
}
```

---

## Migration Checklist

### Phase 1: Prepare minna-core
- [ ] Build CLI binary with `--json` output flags
- [ ] Ensure daemon can run without Desktop app
- [ ] Test `brew install` flow works

### Phase 2: Create minna-desktop repo
- [ ] Create new GitHub repo `minna-ai/minna-desktop`
- [ ] Copy Swift files with git history (`git filter-branch` or `git subtree`)
- [ ] Update imports and paths
- [ ] Add MinnaCliWrapper

### Phase 3: Refactor Desktop
- [ ] Remove MinnaEngineManager process spawning
- [ ] Remove OAuth sheets
- [ ] Simplify to CLI wrapper
- [ ] Test "not installed" state

### Phase 4: Clean up minna-core
- [ ] Remove `src/minna_ep1/` directory
- [ ] Update main README (no more Mac app mention)
- [ ] Archive old Swift code in git history

---

## Open Questions

1. **Should Desktop require Homebrew install?** Or bundle the binary?
   - Recommendation: Require Homebrew. Keeps Desktop thin.

2. **Real-time sync progress?**
   - Option A: Poll `minna status --json` every second
   - Option B: Connect to admin.sock for streaming
   - Recommendation: Start with polling, add streaming later

3. **City Pop theme reuse?**
   - The theme could be a separate Swift package
   - Other Minna tools (hypothetical iOS app?) could use it

4. **Version compatibility?**
   - Desktop should check CLI version
   - Warn if CLI is outdated

---

## Timeline

This extraction is **not blocking** CLI-first development. The Swift code can stay in `src/minna_ep1/` until:

1. CLI is functional (`minna add`, `minna status` work)
2. Homebrew formula is published
3. Someone wants to work on Desktop

Until then, it's just dormant code that doesn't interfere with CLI development.
