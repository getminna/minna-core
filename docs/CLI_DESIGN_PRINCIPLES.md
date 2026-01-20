# Minna CLI Design Principles

**Status**: Spec
**Date**: 2026-01-20
**Inspiration**: Claude Code CLI conventions

---

## Core Philosophy

The README **is** the TUI spec. Every interaction example in the README defines exactly how the CLI should behave. This document extracts the design principles.

---

## Visual Language

### Prompt Markers

| Symbol | Meaning | Usage |
|--------|---------|-------|
| `?` | Question/prompt | User input needed |
| `→` | Selected option | Current selection in list |
| `✔` | Success | Operation completed |
| `✖` | Error | Operation failed |
| `⚡` | Active/fast | Sprint sync, quick operations |
| `💤` | Background | Long-running background tasks |
| `────` | Divider | Section separation |

### Example Flow

```
$ minna add linear

? How would you like to connect Linear?
  → Open browser (recommended)
    Paste a Personal Access Token

Opening browser...

✔ Linear connected.

⚡ Sprint Sync...  ████████████████████  142 artifacts

💤 Deep sync running in background (90 days of history).
   Run `minna status` to check progress.
```

---

## Interaction Patterns

### 1. Questions with Selection

```
? Which AI tool do you use?
  → Cursor
    Claude Code
    VS Code + Continue
    Windsurf
    Other / Manual
```

**Rules**:
- Question starts with `?` and ends with `?`
- Options indented 2 spaces
- Current selection marked with `→`
- Arrow keys navigate, Enter selects
- First option is default/recommended
- "Other / Manual" always last if applicable

### 2. Confirmation Prompts

```
✔ Found ~/.cursor/mcp.json
? Add Minna to Cursor? (Y/n) y
```

**Rules**:
- Show what was found/detected first
- Default option capitalized: `(Y/n)` = yes is default
- Single character input, no Enter required
- Immediate action after input

### 3. Progress Display

```
⚡ Sprint Sync...  ████████████████████  142 artifacts
```

**Rules**:
- Emoji prefix indicates operation type
- Operation name followed by `...`
- Progress bar (20 chars wide)
- Count/metric at end
- Updates in place (carriage return)

### 4. Background Task Notice

```
💤 Deep sync running in background (90 days of history).
   Run `minna status` to check progress.
```

**Rules**:
- Sleepy emoji for background tasks
- Explain what's happening in parentheses
- Always provide next action on following line
- Indented continuation (3 spaces)

### 5. Completion Box

```
────────────────────────────────────────────────────────
  ✔ Ready.

  Copied to clipboard:

    What's the status of Project Atlas?

  Paste into chat (⌘V) and hit Enter.
────────────────────────────────────────────────────────
```

**Rules**:
- Box width: 56 chars (fits 80-char terminal with margin)
- Top/bottom borders with `─` (box drawing)
- Content indented 2 spaces
- Blank lines for breathing room
- Actionable instruction at end

---

## Command Output Conventions

### Status Command

```
$ minna status

  daemon     running (pid 12847)
  uptime     3d 14h 22m

  SOURCES
  ─────────────────────────────────────────
  slack      ✔ synced    1,247 messages    2m ago
  linear     ✔ synced      342 issues      5m ago
  github     ⚡ syncing     12%            (PRs)

  STORAGE
  ─────────────────────────────────────────
  documents  1,589
  vectors    1,589
  db size    48.2 MB
```

**Rules**:
- Key-value pairs aligned
- Section headers in CAPS
- Divider line under headers
- Status emoji: ✔ synced, ⚡ syncing, ✖ error, ⏸ paused
- Human-readable times ("2m ago", "3d 14h")
- Right-align numbers for scannability

### JSON Output (`--json`)

Every command should support `--json` for scripting:

```
$ minna status --json
{
  "daemon": {"status": "running", "pid": 12847, "uptime_secs": 310920},
  "sources": [
    {"name": "slack", "status": "synced", "documents": 1247, "last_sync": "2026-01-20T15:30:00Z"},
    {"name": "linear", "status": "synced", "documents": 342, "last_sync": "2026-01-20T15:27:00Z"},
    {"name": "github", "status": "syncing", "progress": 0.12, "current_task": "PRs"}
  ],
  "storage": {"documents": 1589, "vectors": 1589, "db_bytes": 50545459}
}
```

---

## Error Handling

### Recoverable Errors

```
✖ Linear authentication failed.

  Your token may have expired. Re-authenticate:

    minna add linear
```

**Rules**:
- Error symbol `✖` with brief description
- Blank line
- Explanation in plain language
- Concrete next step in code block

### Fatal Errors

```
✖ Cannot start daemon: port 8847 in use.

  Another process is using the OAuth callback port.
  Check: lsof -i :8847

  If it's a stuck Minna process:
    minna daemon restart --force
```

**Rules**:
- Technical detail in first line
- Debugging help
- Recovery command if applicable

### Missing Dependencies

```
✖ Minna daemon not running.

  Start it:
    minna daemon start

  Or check logs:
    minna daemon logs
```

---

## Color Usage

Colors enhance but never carry meaning alone (accessibility).

| Color | Usage |
|-------|-------|
| **Green** | Success (✔), synced status |
| **Yellow** | Warning, in-progress |
| **Red** | Error (✖) |
| **Cyan** | Highlights, links, commands |
| **Dim** | Secondary info, hints |
| **Bold** | Emphasis, section headers |

### Respecting NO_COLOR

```rust
// Always check
if std::env::var("NO_COLOR").is_ok() || !atty::is(atty::Stream::Stdout) {
    // Plain output, no ANSI codes
}
```

---

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `↑`/`↓` | Navigate options |
| `Enter` | Select/confirm |
| `Esc` | Cancel/back |
| `Ctrl+C` | Abort |
| `q` | Quit (in status views) |

---

## Terminal Compatibility

### Width Handling

- Minimum: 40 columns
- Optimal: 80 columns
- Wide: 120+ columns (no extra benefit)

Detect and adapt:
```rust
let width = terminal_size::terminal_size()
    .map(|(w, _)| w.0 as usize)
    .unwrap_or(80);
```

### Unicode Support

Assume UTF-8. The symbols we use (✔, ✖, →, ─, ⚡, 💤) are widely supported.

Fallback for ancient terminals:
```
✔ → [ok]
✖ → [error]
→ → >
─ → -
⚡ → *
💤 → ~
```

---

## Claude Code Conventions to Follow

### 1. Immediate Feedback

Never leave the user waiting without indication. If something takes >100ms, show activity.

### 2. Progressive Disclosure

Start with the essential. Details on request (`--verbose`, `--debug`).

### 3. Sensible Defaults

`minna add` should just work. Interactive prompts guide, not interrogate.

### 4. Predictable Structure

Every command follows: action → feedback → next step.

### 5. Respect the Terminal

- Don't clear the screen unnecessarily
- Don't trap Ctrl+C
- Support piping (`minna status | grep slack`)
- Exit codes: 0 = success, 1 = error, 2 = usage error

### 6. Help is Helpful

```
$ minna add --help

Connect data sources to Minna

Usage: minna add [SOURCES...]

Arguments:
  [SOURCES...]  Sources to connect (slack, linear, github, notion, atlassian, google)
                If omitted, shows interactive picker.

Options:
  -h, --help  Print help

Examples:
  minna add                    # Interactive source picker
  minna add slack              # Connect Slack
  minna add slack linear       # Connect multiple sources
```

---

## Implementation Notes

### Recommended Crates

| Crate | Purpose |
|-------|---------|
| `clap` | Argument parsing (with derive) |
| `dialoguer` | Interactive prompts, selections |
| `indicatif` | Progress bars, spinners |
| `console` | Colors, terminal control |
| `crossterm` | Low-level terminal (if needed) |

### Example: Selection Prompt

```rust
use dialoguer::{theme::ColorfulTheme, Select};

let options = &["Cursor", "Claude Code", "VS Code + Continue", "Windsurf", "Other / Manual"];
let selection = Select::with_theme(&ColorfulTheme::default())
    .with_prompt("Which AI tool do you use?")
    .items(options)
    .default(0)
    .interact()?;
```

### Example: Progress Bar

```rust
use indicatif::{ProgressBar, ProgressStyle};

let pb = ProgressBar::new(total);
pb.set_style(ProgressStyle::default_bar()
    .template("⚡ {msg}...  {bar:20} {pos}/{len}")
    .progress_chars("█▓░"));
pb.set_message("Sprint Sync");

for item in items {
    process(item);
    pb.inc(1);
}
pb.finish_with_message("done");
```

---

## Summary

The Minna CLI should feel like a natural extension of Claude Code's terminal experience:

1. **Clean** — No clutter, every character earns its place
2. **Responsive** — Immediate feedback, streaming progress
3. **Helpful** — Errors explain, success suggests next steps
4. **Scriptable** — `--json` everywhere, sensible exit codes
5. **Beautiful** — Unicode symbols, thoughtful color, good typography

The README is the contract. Build exactly what it shows.
