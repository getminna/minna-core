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
                title: "To connect Slack, you'll need a User OAuth Token.",
                steps: vec![
                    "Go to: https://api.slack.com/apps",
                    "Create an app (or select existing)",
                    "Install to your workspace",
                    "Copy the User OAuth Token (starts with xoxp-)",
                ],
                auth_type: AuthType::Token {
                    prompt: "Paste your Slack token",
                    prefix: Some("xoxp-"),
                },
            },
            Source::Linear => SourceInstructions {
                title: "To connect Linear, you'll need an API key.",
                steps: vec![
                    "Go to: https://linear.app/settings/api",
                    "Create a new Personal API Key",
                    "Copy the key",
                ],
                auth_type: AuthType::Token {
                    prompt: "Paste your Linear API key",
                    prefix: None,
                },
            },
            Source::Github => SourceInstructions {
                title: "To connect GitHub, you'll need a Personal Access Token.",
                steps: vec![
                    "Go to: https://github.com/settings/tokens?type=beta",
                    "Generate new token (fine-grained)",
                    "Select repositories and permissions (Issues, PRs read access)",
                    "Copy the token",
                ],
                auth_type: AuthType::Token {
                    prompt: "Paste your GitHub PAT",
                    prefix: Some("github_pat_"),
                },
            },
            Source::Notion => SourceInstructions {
                title: "To connect Notion, you'll need an Internal Integration Token.",
                steps: vec![
                    "Go to: https://www.notion.so/my-integrations",
                    "Create new integration",
                    "Copy the Internal Integration Secret",
                    "Share pages/databases with your integration in Notion",
                ],
                auth_type: AuthType::Token {
                    prompt: "Paste your Notion integration token",
                    prefix: Some("secret_"),
                },
            },
            Source::Atlassian => SourceInstructions {
                title: "To connect Atlassian, you'll need an API token and your email.",
                steps: vec![
                    "Go to: https://id.atlassian.com/manage-profile/security/api-tokens",
                    "Create API token",
                    "Copy the token",
                ],
                auth_type: AuthType::AtlassianToken,
            },
            Source::Google => SourceInstructions {
                title: "To connect Google, you'll need OAuth credentials from a Google Cloud project.",
                steps: vec![
                    "Go to: https://console.cloud.google.com",
                    "Create a project (or select existing)",
                    "APIs & Services → Credentials → Create OAuth Client ID",
                    "Application type: Desktop Application",
                    "Copy the Client ID and Client Secret",
                ],
                auth_type: AuthType::GoogleOAuth,
            },
        }
    }
}

pub struct SourceInstructions {
    pub title: &'static str,
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
