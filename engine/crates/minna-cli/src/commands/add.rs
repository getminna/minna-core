use anyhow::{anyhow, Result};
use minna_auth_bridge::{AuthToken, Provider, TokenStore};
use std::path::PathBuf;

use crate::admin_client::AdminClient;
use crate::sources::{AuthType, Source};
use crate::ui;

pub async fn run(sources: Vec<String>) -> Result<()> {
    let sources = if sources.is_empty() {
        // Interactive picker
        pick_sources()?
    } else {
        // Parse provided sources
        sources
            .iter()
            .map(|s| {
                Source::from_str(s)
                    .ok_or_else(|| anyhow!("Unknown source: {}. Valid: slack, linear, github, notion, atlassian, google", s))
            })
            .collect::<Result<Vec<_>>>()?
    };

    for source in sources {
        if let Err(e) = connect_source(source).await {
            ui::error(&format!("Failed to connect {}: {}", source.display_name(), e));
        }
    }

    Ok(())
}

fn pick_sources() -> Result<Vec<Source>> {
    let items: Vec<&str> = Source::all().iter().map(|s| s.display_name()).collect();

    let selection = ui::prompt_select("Which sources do you want to connect?", &items)?;

    Ok(vec![Source::all()[selection]])
}

async fn connect_source(source: Source) -> Result<()> {
    let instructions = source.instructions();

    // Show instructions
    ui::header(instructions.title);
    ui::steps(&instructions.steps);

    // Collect credentials based on auth type
    let token = match instructions.auth_type {
        AuthType::Token { prompt, prefix } => {
            let value = ui::prompt_password(prompt)?;

            // Validate prefix if expected
            if let Some(expected) = prefix {
                if !value.starts_with(expected) {
                    ui::error(&format!(
                        "Token should start with '{}'. Got something else.",
                        expected
                    ));
                    return Err(anyhow!("Invalid token format"));
                }
            }

            value
        }
        AuthType::AtlassianToken => {
            let email = ui::prompt_input("Your Atlassian email")?;
            let token = ui::prompt_password("Paste your API token")?;
            // Store as email:token for Basic Auth
            format!("{}:{}", email, token)
        }
        AuthType::GoogleOAuth => {
            return connect_google().await;
        }
    };

    // Verify the token
    let spinner = ui::spinner(&format!("Verifying {}...", source.display_name()));
    let verification = verify_token(source, &token).await;
    spinner.finish_and_clear();

    match verification {
        Ok(display_name) => {
            ui::success(&format!("Connected to {} ({})", source.display_name(), display_name));
        }
        Err(e) => {
            ui::error(&format!("Verification failed: {}", e));
            return Err(e);
        }
    }

    // Store in Keychain
    store_token(source, &token)?;

    // Trigger sync
    trigger_sync(source).await?;

    Ok(())
}

async fn verify_token(source: Source, token: &str) -> Result<String> {
    let client = reqwest::Client::new();

    match source {
        Source::Slack => {
            let resp: serde_json::Value = client
                .post("https://slack.com/api/auth.test")
                .bearer_auth(token)
                .send()
                .await?
                .json()
                .await?;

            if resp["ok"].as_bool() != Some(true) {
                return Err(anyhow!("Slack API error: {}", resp["error"]));
            }

            Ok(resp["team"].as_str().unwrap_or("Unknown").to_string())
        }
        Source::Linear => {
            let resp: serde_json::Value = client
                .post("https://api.linear.app/graphql")
                .header("Authorization", token)
                .json(&serde_json::json!({
                    "query": "{ viewer { id name } organization { name } }"
                }))
                .send()
                .await?
                .json()
                .await?;

            let org = resp["data"]["organization"]["name"]
                .as_str()
                .unwrap_or("Unknown");
            Ok(org.to_string())
        }
        Source::Github => {
            let resp: serde_json::Value = client
                .get("https://api.github.com/user")
                .header("User-Agent", "minna-cli")
                .bearer_auth(token)
                .send()
                .await?
                .json()
                .await?;

            if resp["login"].is_null() {
                return Err(anyhow!("GitHub API error: {}", resp["message"]));
            }

            Ok(resp["login"].as_str().unwrap_or("Unknown").to_string())
        }
        Source::Notion => {
            let resp: serde_json::Value = client
                .get("https://api.notion.com/v1/users/me")
                .header("Notion-Version", "2022-06-28")
                .bearer_auth(token)
                .send()
                .await?
                .json()
                .await?;

            if resp["object"].as_str() == Some("error") {
                return Err(anyhow!("Notion API error: {}", resp["message"]));
            }

            let name = resp["name"].as_str().unwrap_or("Integration");
            Ok(name.to_string())
        }
        Source::Atlassian => {
            // Token format is email:token
            let parts: Vec<&str> = token.splitn(2, ':').collect();
            if parts.len() != 2 {
                return Err(anyhow!("Invalid Atlassian credentials format"));
            }

            // We need the cloud ID first - this is a simplified check
            // In production, we'd need the user to provide their site URL
            let resp = client
                .get("https://api.atlassian.com/oauth/token/accessible-resources")
                .basic_auth(parts[0], Some(parts[1]))
                .send()
                .await?;

            if !resp.status().is_success() {
                return Err(anyhow!("Atlassian authentication failed"));
            }

            let resources: Vec<serde_json::Value> = resp.json().await?;
            let site_name = resources
                .first()
                .and_then(|r| r["name"].as_str())
                .unwrap_or("Atlassian");

            Ok(site_name.to_string())
        }
        Source::Google => {
            // Google verification happens during OAuth flow
            Ok("Google".to_string())
        }
    }
}

fn store_token(source: Source, token: &str) -> Result<()> {
    let data_dir = get_data_dir()?;
    let token_path = data_dir.join("auth.json");

    let mut store = TokenStore::load(&token_path)?;

    let provider = match source {
        Source::Slack => Provider::Slack,
        Source::Linear => Provider::Linear,
        Source::Github => Provider::Github,
        Source::Google => Provider::Google,
        // Notion and Atlassian need to be added to Provider enum
        _ => {
            // For now, store in keychain directly
            use std::process::Command;
            let account = format!("{}_token", source.as_str());
            let _ = Command::new("security")
                .args(["delete-generic-password", "-s", "minna_ai", "-a", &account])
                .output();
            Command::new("security")
                .args(["add-generic-password", "-s", "minna_ai", "-a", &account, "-w", token])
                .output()?;
            return Ok(());
        }
    };

    store.set(AuthToken {
        provider,
        access_token: token.to_string(),
        refresh_token: None,
        expires_at: None,
        scope: None,
        token_type: Some("Bearer".to_string()),
    });

    Ok(())
}

async fn trigger_sync(source: Source) -> Result<()> {
    let client = AdminClient::new();

    // Check if daemon is running
    if !client.is_daemon_running() {
        println!();
        ui::info("Daemon not running. Start it to begin syncing:");
        println!("    minna daemon start");
        return Ok(());
    }

    // Check if daemon is ready
    let status = client.get_status().await;
    if let Ok(s) = &status {
        if !s.ready {
            println!();
            ui::info("Daemon is starting up. Sync will begin shortly.");
            ui::background_notice(
                "Run `minna status` to check when ready.",
                "",
            );
            return Ok(());
        }
    }

    // Map source to provider name
    let provider_name = match source {
        Source::Slack => "slack",
        Source::Linear => "linear",
        Source::Github => "github",
        Source::Notion => "notion", // Note: may not be implemented in daemon yet
        Source::Atlassian => "atlassian", // Note: may not be implemented in daemon yet
        Source::Google => "google",
    };

    // Start sync with sprint mode (recent items first)
    let spinner = ui::spinner(&format!("Starting {} sync...", source.display_name()));

    match client.sync_provider(provider_name, Some("sprint"), Some(7)).await {
        Ok(result) => {
            spinner.finish_and_clear();

            if result.items_synced > 0 {
                let pb = ui::progress_bar(result.items_synced as u64, "Sprint Sync");
                pb.set_position(result.items_synced as u64);
                pb.finish_with_message("done");
            } else {
                ui::success(&format!("{} sync started", source.display_name()));
            }

            println!();
            ui::background_notice(
                "Deep sync running in background (90 days of history).",
                "Run `minna status` to check progress.",
            );
        }
        Err(e) => {
            spinner.finish_and_clear();
            // Don't fail the whole add operation if sync fails
            ui::info(&format!("Sync will start when daemon is ready: {}", e));
        }
    }

    Ok(())
}

async fn connect_google() -> Result<()> {
    use tiny_http::{Response, Server};

    println!();
    println!("  Enable the APIs you want:");
    println!("    • Calendar: https://console.cloud.google.com/apis/library/calendar-json.googleapis.com");
    println!("    • Drive:    https://console.cloud.google.com/apis/library/drive.googleapis.com");
    println!("    • Gmail:    https://console.cloud.google.com/apis/library/gmail.googleapis.com");
    println!();

    let client_id = ui::prompt_password("Paste your Client ID")?;
    let client_secret = ui::prompt_password("Paste your Client Secret")?;

    // Build authorization URL
    let redirect_uri = "http://127.0.0.1:8847/callback";
    let scopes = [
        "https://www.googleapis.com/auth/calendar.readonly",
        "https://www.googleapis.com/auth/drive.readonly",
        "https://www.googleapis.com/auth/gmail.readonly",
    ];

    let auth_url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?\
        client_id={}&\
        redirect_uri={}&\
        response_type=code&\
        scope={}&\
        access_type=offline&\
        prompt=consent",
        urlencoding::encode(&client_id),
        urlencoding::encode(redirect_uri),
        urlencoding::encode(&scopes.join(" ")),
    );

    println!();
    ui::info("Opening browser for authorization...");

    // Start local server for callback
    let server = Server::http("127.0.0.1:8847")
        .map_err(|e| anyhow!("Failed to start callback server: {}", e))?;

    // Open browser
    open::that(&auth_url)?;

    // Wait for callback
    let spinner = ui::spinner("Waiting for authorization...");

    let request = server
        .incoming_requests()
        .next()
        .ok_or_else(|| anyhow!("No callback received"))?;

    spinner.finish_and_clear();

    // Extract code from URL
    let url = request.url().to_string();
    let code = url
        .split("code=")
        .nth(1)
        .and_then(|s| s.split('&').next())
        .ok_or_else(|| anyhow!("No authorization code in callback"))?
        .to_string();

    // Send response to browser
    let response = Response::from_string(
        "<html><body><h1>Success!</h1><p>You can close this window.</p></body></html>",
    )
    .with_header(
        tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html"[..]).unwrap(),
    );
    let _ = request.respond(response);

    // Exchange code for tokens
    let spinner = ui::spinner("Exchanging authorization code...");

    let client = reqwest::Client::new();
    let token_resp: serde_json::Value = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("code", &code),
            ("grant_type", "authorization_code"),
            ("redirect_uri", redirect_uri),
        ])
        .send()
        .await?
        .json()
        .await?;

    spinner.finish_and_clear();

    if let Some(error) = token_resp["error"].as_str() {
        ui::error(&format!("OAuth error: {}", error));
        return Err(anyhow!("OAuth failed: {}", error));
    }

    let access_token = token_resp["access_token"]
        .as_str()
        .ok_or_else(|| anyhow!("No access token in response"))?;
    let refresh_token = token_resp["refresh_token"].as_str();

    // Get user info for display
    let user_info: serde_json::Value = client
        .get("https://www.googleapis.com/oauth2/v2/userinfo")
        .bearer_auth(access_token)
        .send()
        .await?
        .json()
        .await?;

    let email = user_info["email"].as_str().unwrap_or("Unknown");

    ui::success(&format!("Authorized. Connected to Google ({})", email));

    // Store tokens
    let data_dir = get_data_dir()?;
    let token_path = data_dir.join("auth.json");
    let mut store = TokenStore::load(&token_path)?;

    store.set(AuthToken {
        provider: Provider::Google,
        access_token: access_token.to_string(),
        refresh_token: refresh_token.map(|s| s.to_string()),
        expires_at: None,
        scope: Some(scopes.join(" ")),
        token_type: Some("Bearer".to_string()),
    });

    // Also store client credentials for refresh
    use std::process::Command;
    let _ = Command::new("security")
        .args(["delete-generic-password", "-s", "minna_ai", "-a", "google_client_id"])
        .output();
    Command::new("security")
        .args(["add-generic-password", "-s", "minna_ai", "-a", "google_client_id", "-w", &client_id])
        .output()?;

    let _ = Command::new("security")
        .args(["delete-generic-password", "-s", "minna_ai", "-a", "google_client_secret"])
        .output();
    Command::new("security")
        .args(["add-generic-password", "-s", "minna_ai", "-a", "google_client_secret", "-w", &client_secret])
        .output()?;

    // Trigger sync
    trigger_sync(Source::Google).await?;

    Ok(())
}

fn get_data_dir() -> Result<PathBuf> {
    let dir = directories::ProjectDirs::from("ai", "minna", "minna")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".local/share/minna")
        });

    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

// URL encoding helper
mod urlencoding {
    pub fn encode(s: &str) -> String {
        let mut result = String::new();
        for c in s.chars() {
            match c {
                'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
                _ => {
                    for byte in c.to_string().as_bytes() {
                        result.push_str(&format!("%{:02X}", byte));
                    }
                }
            }
        }
        result
    }
}
