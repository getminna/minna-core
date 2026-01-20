# V1 Authentication Strategy

**Status**: Decision
**Date**: 2026-01-20
**Goal**: Ship CLI tomorrow

---

## Decision

**All sources use token/credential-based auth in v1.** No OAuth browser flows.

---

## V1 Auth Methods

| Source | Auth Method | What User Provides |
|--------|-------------|-------------------|
| Slack | User token | `xoxp-...` from Slack app settings |
| Linear | API key | Personal API key from Linear settings |
| GitHub | PAT | Fine-grained Personal Access Token |
| Notion | Integration token | Internal integration secret |
| Atlassian | API token | Token from id.atlassian.com/manage-profile/security/api-tokens |

### Not in V1

| Source | Why Deferred |
|--------|--------------|
| Google Drive | Requires OAuth bridge (we'd handle the consent redirect) |

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

### Key Points

- Token input is masked (password field)
- Immediate verification before storing
- Token goes straight to Keychain
- No browser, no redirect server, no OAuth dance

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
| Atlassian | `GET /rest/api/3/myself` (requires email + token as Basic Auth) |

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

## What This Removes from Scope

| Component | Status |
|-----------|--------|
| `LocalOAuthManager` equivalent | Not needed |
| Local HTTP server (port 8847) | Not needed |
| Browser launch + redirect capture | Not needed |
| Google Drive connector | Deferred (only source requiring OAuth bridge) |

---

## Summary

V1 is token-paste only:
1. User gets token from service's settings page
2. `minna add {source}` prompts for token
3. Token is verified, stored in Keychain
4. Sync starts

Ship tomorrow. Add OAuth polish later.
