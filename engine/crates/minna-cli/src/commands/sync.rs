use anyhow::{anyhow, Result};

use crate::admin_client::AdminClient;
use crate::commands::daemon;
use crate::sources::Source;
use crate::ui;

pub async fn run(sources: Vec<String>, all: bool) -> Result<()> {
    // Ensure daemon is running
    let is_ready = daemon::ensure_running().await?;

    if !is_ready {
        ui::info("Daemon is starting. Sync will begin when ready.");
        return Ok(());
    }

    let client = AdminClient::new();

    let sources_to_sync: Vec<Source> = if all || sources.is_empty() {
        // Get configured sources from daemon
        let creds = client.verify_credentials().await?;
        creds
            .providers
            .iter()
            .filter(|p| p.configured && !matches!(p.name.as_str(), "cursor" | "claude_code"))
            .filter_map(|p| Source::from_str(&p.name))
            .collect()
    } else {
        sources
            .iter()
            .map(|s| {
                Source::from_str(s)
                    .ok_or_else(|| anyhow!("Unknown source: {}", s))
            })
            .collect::<Result<Vec<_>>>()?
    };

    if sources_to_sync.is_empty() {
        ui::info("No sources configured. Add one with:");
        println!("    minna add slack");
        return Ok(());
    }

    for source in sources_to_sync {
        sync_source(&client, source).await?;
    }

    Ok(())
}

async fn sync_source(client: &AdminClient, source: Source) -> Result<()> {
    let provider_name = match source {
        Source::Slack => "slack",
        Source::Linear => "linear",
        Source::Github => "github",
        Source::Notion => "notion",
        Source::Atlassian => "atlassian",
        Source::Google => "google",
    };

    let spinner = ui::spinner(&format!("Syncing {}...", source.display_name()));

    match client.sync_provider(provider_name, None, Some(90)).await {
        Ok(result) => {
            spinner.finish_and_clear();

            if result.items_synced > 0 {
                ui::success(&format!(
                    "{}: {} items synced",
                    source.display_name(),
                    result.items_synced
                ));
            } else {
                ui::success(&format!("{}: up to date", source.display_name()));
            }
        }
        Err(e) => {
            spinner.finish_and_clear();
            ui::error(&format!("{}: {}", source.display_name(), e));
        }
    }

    Ok(())
}
