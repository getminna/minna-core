use anyhow::{anyhow, Result};
use std::process::Command;

use crate::sources::Source;
use crate::ui;

pub async fn run(source_name: &str) -> Result<()> {
    let source = Source::from_str(source_name)
        .ok_or_else(|| anyhow!("Unknown source: {}", source_name))?;

    // Confirm removal
    let yes_option = format!("Yes, disconnect {}", source.display_name());
    let items = &[yes_option.as_str(), "No, cancel"];
    let selection = ui::prompt_select(
        &format!("Remove {} from Minna?", source.display_name()),
        items,
    )?;

    if selection == 1 {
        ui::info("Cancelled.");
        return Ok(());
    }

    // Remove from Keychain
    let account = match source {
        Source::Slack => "slack_user_token",
        Source::Linear => "linear_token",
        Source::Github => "github_pat",
        Source::Notion => "notion_token",
        Source::Atlassian => "atlassian_token",
        Source::Google => "googleWorkspace_token",
    };

    let spinner = ui::spinner(&format!("Removing {}...", source.display_name()));

    let _ = Command::new("security")
        .args(["delete-generic-password", "-s", "minna_ai", "-a", account])
        .output();

    // For Google, also remove client credentials and refresh token
    if source == Source::Google {
        let _ = Command::new("security")
            .args(["delete-generic-password", "-s", "minna_ai", "-a", "google_client_id"])
            .output();
        let _ = Command::new("security")
            .args(["delete-generic-password", "-s", "minna_ai", "-a", "google_client_secret"])
            .output();
        let _ = Command::new("security")
            .args(["delete-generic-password", "-s", "minna_ai", "-a", "googleWorkspace_refresh_token"])
            .output();
    }

    spinner.finish_and_clear();

    // TODO: Notify daemon to stop syncing and optionally delete indexed data

    ui::success(&format!("{} disconnected.", source.display_name()));
    ui::info("Indexed data remains until next full re-sync.");

    Ok(())
}
