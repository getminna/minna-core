# V1 Authentication Strategy

**Status**: Decision
**Date**: 2026-01-20
**Goal**: Ship CLI tomorrow

---

## Decision

**All sources use user-provided credentials.** No shared Minna OAuth apps.

Two patterns:

1. **Token-paste**: User gets token from settings page, pastes into CLI
2. **OAuth with user's app**: User provides client_id + secret, CLI handles browser flow

---

## V1 Auth Methods

### Token-Paste (simple)

| Source | What User Provides | How They Get It |
|--------|-------------------|-----------------|
| Slack | `xoxp-...` token | Create Slack app → Install → Copy from settings |
| Linear | API key | Settings → API → Create Personal Key |
| GitHub | PAT | Developer Settings → Fine-grained PAT |
| Notion | Integration token | Create internal integration → Copy secret |
| Atlassian | API token + email | id.atlassian.com → API tokens |

### OAuth with User's App (requires browser flow)

| Source | What User Provides | CLI Does |
|--------|-------------------|----------|
| Google | client_id + client_secret | Opens browser, captures redirect, exchanges code |

For Google, the user creates their own Google Cloud project, enables APIs, and provides credentials. Minna handles the OAuth dance but with the **user's app**, not a shared Minna app.

Reference: [google_workspace_mcp](https://github.com/taylorwilsdon/google_workspace_mcp) uses the same pattern.

**Key detail**: Use "Desktop Application" as the OAuth app type. This uses loopback redirect (`http://localhost:PORT/callback`) and doesn't require configuring redirect URIs in Google Cloud Console.

The auth-bridge already has this: `authorize_url()`, `exchange_code()`, `refresh_token()`.

---

## CLI Flow

```
$ minna add slack

  To connect Slack, you'll need a User OAuth Token.

  1. Go to: https://api.slack.com/apps
  2. Create an app (or select existing)
  3. Install to your workspace
  4. Copy the User OAuth Token (starts with xoxp-)

? Paste your Slack token: ████████████████████████████

✔ Token verified. Found workspace: Acme Corp

⚡ Sprint Sync...  ████████████████████  142 messages

💤 Deep sync running in background.
```

### Google Flow (OAuth with user's app)

```
$ minna add google

  To connect Google, you'll need OAuth credentials from a Google Cloud project.

  1. Go to: https://console.cloud.google.com
  2. Create a project (or select existing)
  3. APIs & Services → Credentials → Create OAuth Client ID
  4. Application type: Desktop Application
  5. Copy the Client ID and Client Secret

  Enable the APIs you want:
    • Calendar: https://console.cloud.google.com/apis/library/calendar-json.googleapis.com
    • Drive:    https://console.cloud.google.com/apis/library/drive.googleapis.com
    • Gmail:    https://console.cloud.google.com/apis/library/gmail.googleapis.com

? Paste your Client ID: ████████████████████████████
? Paste your Client Secret: ████████████████████████████

Opening browser for authorization...

✔ Authorized. Connected to Google (user@example.com)

⚡ Sprint Sync...  ████████████████████  89 items

💤 Deep sync running in background.
```

### Key Points

- Token-paste sources: masked input, immediate verification, no browser
- Google: prompts for client_id + secret, then opens browser for consent

---

## Implementation

### CLI Prompts

```rust
use dialoguer::{theme::ColorfulTheme, Password};

let token = Password::with_theme(&ColorfulTheme::default())
    .with_prompt("Paste your Slack token")
    .interact()?;

// Verify before storing
let workspace = verify_slack_token(&token).await?;
println!("✔ Token verified. Found workspace: {}", workspace);

// Store in Keychain
keyring::Entry::new("minna", "slack_user_token")?
    .set_password(&token)?;
```

### Token Verification

Each source needs a quick verification call:

| Source | Verification Endpoint |
|--------|----------------------|
| Slack | `auth.test` |
| Linear | `viewer` query |
| GitHub | `GET /user` |
| Notion | `GET /v1/users/me` |
| Atlassian | `GET /rest/api/3/myself` (email + token as Basic Auth) |
| Google | Token exchange success = verified |

### Existing Rust Code

The `minna-auth-bridge` crate already stores tokens in Keychain:

```rust
// From engine/crates/minna-auth-bridge/src/lib.rs
pub struct TokenStore { ... }
impl TokenStore {
    pub fn set_token(&self, provider: Provider, token: &str) -> Result<()>;
    pub fn get_token(&self, provider: Provider) -> Result<Option<String>>;
}
```

The CLI just needs to:
1. Prompt for token
2. Verify it works
3. Call `TokenStore::set_token()`
4. Trigger sync via admin socket

---

## Updated README Section

Replace the OAuth-heavy examples with simpler token flow:

```markdown
### What Happens

Each source uses a personal token you create:

- **Slack**: User OAuth Token from your Slack app
- **Linear**: API key from Settings → API
- **GitHub**: Fine-grained PAT from Developer Settings
- **Notion**: Internal integration token

Minna walks you through getting each token.
```

---

## Migration Path

V1 → V2: Add OAuth flows later for sources where it's painful to get tokens manually. The architecture supports both—`minna add slack` can offer:

```
? How would you like to connect Slack?
  → Open browser (recommended)      # V2: OAuth
    Paste a token                   # V1: What we're building now
```

For v1, only the token option exists.

---

## What This Means

| Component | Status |
|-----------|--------|
| Shared Minna OAuth apps | Not needed (users bring their own) |
| Local HTTP server (port 8847) | Needed for Google only |
| Browser launch + redirect capture | Needed for Google only |
| Token verification | Needed for all sources |

---

## Summary

V1 uses user-provided credentials (no shared Minna OAuth apps):

**Token-paste sources** (Slack, Linear, GitHub, Notion, Atlassian):
1. User gets token from service's settings page
2. `minna add {source}` prompts for token
3. Token verified, stored in Keychain
4. Sync starts

**OAuth sources** (Google):
1. User creates Google Cloud project, provides client_id + secret
2. `minna add google` opens browser for consent
3. User approves, redirects to localhost
4. CLI exchanges code for tokens, stores in Keychain
5. Sync starts

The auth-bridge already has OAuth machinery. CLI needs local HTTP server for Google only.
