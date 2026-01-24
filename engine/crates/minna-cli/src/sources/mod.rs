use serde::{Deserialize, Serialize};

/// Supported data sources
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Slack,
    Linear,
    Github,
    Notion,
    Atlassian,
    Google,
}

impl Source {
    pub fn all() -> &'static [Source] {
        &[
            Source::Slack,
            Source::Linear,
            Source::Github,
            Source::Notion,
            Source::Atlassian,
            Source::Google,
        ]
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Source::Slack => "slack",
            Source::Linear => "linear",
            Source::Github => "github",
            Source::Notion => "notion",
            Source::Atlassian => "atlassian",
            Source::Google => "google",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Source::Slack => "Slack",
            Source::Linear => "Linear",
            Source::Github => "GitHub",
            Source::Notion => "Notion",
            Source::Atlassian => "Atlassian (Jira/Confluence)",
            Source::Google => "Google (Drive/Calendar/Gmail)",
        }
    }

    pub fn from_str(s: &str) -> Option<Source> {
        match s.to_lowercase().as_str() {
            "slack" => Some(Source::Slack),
            "linear" => Some(Source::Linear),
            "github" | "gh" => Some(Source::Github),
            "notion" => Some(Source::Notion),
            "atlassian" | "jira" | "confluence" => Some(Source::Atlassian),
            "google" | "gdrive" | "gmail" | "gcal" => Some(Source::Google),
            _ => None,
        }
    }

    /// Instructions for getting credentials
    pub fn instructions(&self) -> SourceInstructions {
        match self {
            Source::Slack => SourceInstructions {
                title: "Recommended: Connect Slack via Minna Auth Bridge (1-click).",
                recommended_url: Some("https://auth.minna.cloud/api/connect/slack"),
                steps: vec![
                    "Or manually: Go to https://api.slack.com/apps",
                    "Create a 'Classic' app (or select existing)",
                    "Install to your workspace",
                    "Copy the User OAuth Token (starts with xoxp-)",
                ],
                auth_type: AuthType::Token {
                    prompt: "Paste your Slack token",
                    prefix: Some("xoxp-"),
                },
            },
            Source::Linear => SourceInstructions {
                title: "Recommended: Connect Linear via Minna Auth Bridge (1-click).",
                recommended_url: Some("https://auth.minna.cloud/api/connect/linear"),
                steps: vec![
                    "Or manually: Go to https://linear.app/settings/api",
                    "Create a new Personal API Key",
                    "Copy the key",
                ],
                auth_type: AuthType::Token {
                    prompt: "Paste your Linear API key",
                    prefix: None,
                },
            },
            Source::Github => SourceInstructions {
                title: "Recommended: Connect GitHub via Minna Auth Bridge (1-click).",
                recommended_url: Some("https://auth.minna.cloud/api/connect/github"),
                steps: vec![
                    "Or manually: Go to https://github.com/settings/personal-access-tokens/new",
                    "Generate new Fine-Grained token",
                    "Select repositories and 'Metadata: Read', 'Issues: Read', 'Discussions: Read'",
                    "Copy the token",
                ],
                auth_type: AuthType::Token {
                    prompt: "Paste your GitHub PAT",
                    prefix: Some("github_pat_"),
                },
            },
            Source::Notion => SourceInstructions {
                title: "To connect Notion, you'll need an Internal Integration Token.",
                recommended_url: None, // Bridge punted for Tier 2 in 2026
                steps: vec![
                    "Go to: https://www.notion.so/my-integrations",
                    "Create new integration (Internal)",
                    "Copy the Internal Integration Secret",
                    "Share relevant pages with your integration in Notion",
                ],
                auth_type: AuthType::Token {
                    prompt: "Paste your Notion integration token",
                    prefix: Some("secret_"),
                },
            },
            Source::Atlassian => SourceInstructions {
                title: "To connect Atlassian, you'll need an API token and your email.",
                recommended_url: None, // Bridge punted for Tier 2 in 2026
                steps: vec![
                    "Go to: https://id.atlassian.com/manage-profile/security/api-tokens",
                    "Create API token",
                    "Copy the token",
                ],
                auth_type: AuthType::AtlassianToken,
            },
            Source::Google => SourceInstructions {
                title: "To connect Google, you'll need OAuth credentials (client_id/secret).",
                recommended_url: None, // ⏳ Pending CASA Tier 2
                steps: vec![
                    "Go to: https://console.cloud.google.com",
                    "Enable Calendar/Drive/Gmail APIs",
                    "Create OAuth Client ID (Desktop Application)",
                    "Copy Client ID and Secret",
                ],
                auth_type: AuthType::GoogleOAuth,
            },
        }
    }
}

pub struct SourceInstructions {
    pub title: &'static str,
    pub recommended_url: Option<&'static str>,
    pub steps: Vec<&'static str>,
    pub auth_type: AuthType,
}

pub enum AuthType {
    /// Simple token paste
    Token {
        prompt: &'static str,
        prefix: Option<&'static str>,
    },
    /// Atlassian needs email + token
    AtlassianToken,
    /// Google needs client_id + secret, then browser OAuth
    GoogleOAuth,
}
