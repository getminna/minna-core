use anyhow::Result;
use clap::{Parser, Subcommand};

mod admin_client;
mod commands;
mod paths;
mod sources;
mod tui;
mod ui;

#[derive(Parser)]
#[command(name = "minna")]
#[command(about = "Your AI's memory. Local-first. Zero config.")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Connect data sources to Minna
    Add {
        /// Sources to connect (slack, linear, github, notion, atlassian, google)
        /// If omitted, shows interactive picker.
        #[arg(value_name = "SOURCES")]
        sources: Vec<String>,

        /// Use mock data for UI testing (no real API calls)
        #[arg(long, hide = true)]
        ui_test: bool,
    },

    /// Show sources, sync progress, and daemon health
    Status {
        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Use mock data for UI testing (no real API calls)
        #[arg(long, hide = true)]
        ui_test: bool,
    },

    /// Connect Minna to your AI agent (auto-detects current IDE)
    Mcp {
        /// AI tool to configure (claude-code, cursor, zed, antigravity, manual)
        /// If omitted, auto-detects current IDE or installed tools.
        #[arg(value_name = "TOOL")]
        tool: Option<String>,

        /// Use mock data for UI testing (no real API calls)
        #[arg(long, hide = true)]
        ui_test: bool,
    },

    /// Manage the background daemon
    Daemon {
        #[command(subcommand)]
        command: DaemonCommand,
    },

    /// Remove a connected source
    Remove {
        /// Source to disconnect
        #[arg(value_name = "SOURCE")]
        source: String,
    },

    /// Sync sources (fetch latest data)
    Sync {
        /// Sources to sync. If omitted, syncs all configured sources.
        #[arg(value_name = "SOURCES")]
        sources: Vec<String>,

        /// Sync all configured sources
        #[arg(long, short)]
        all: bool,
    },

    /// Review and link user identities across sources
    Link,

    /// Save checkpoint and prepare for context reset (used by hooks)
    #[command(name = "checkpoint-and-clear")]
    CheckpointAndClear {
        /// Trigger type (auto-compact, auto-close, manual)
        #[arg(long, short)]
        trigger: Option<String>,
    },
}

#[derive(Subcommand)]
enum DaemonCommand {
    /// Check if daemon is running
    Status,
    /// Start the daemon
    Start,
    /// Restart the daemon
    Restart,
    /// Tail daemon logs
    Logs {
        /// Number of lines to show
        #[arg(short = 'n', default_value = "50")]
        lines: usize,
        /// Follow log output
        #[arg(short = 'f', long)]
        follow: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing - send to stderr only
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::WARN.into()),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        None => tui::welcome::run().await,
        Some(Commands::Add { sources, ui_test }) => {
            if ui_test {
                tui::add::run_test(sources).await
            } else {
                commands::add::run(sources).await
            }
        }
        Some(Commands::Status { json, ui_test }) => {
            if ui_test {
                tui::status::run_test().await
            } else {
                commands::status::run(json).await
            }
        }
        Some(Commands::Mcp { tool, ui_test }) => {
            if ui_test {
                tui::mcp::run_test(tool).await
            } else {
                commands::mcp::run(tool).await
            }
        }
        Some(Commands::Daemon { command }) => match command {
            DaemonCommand::Status => commands::daemon::status().await,
            DaemonCommand::Start => commands::daemon::start().await,
            DaemonCommand::Restart => commands::daemon::restart().await,
            DaemonCommand::Logs { lines, follow } => commands::daemon::logs(lines, follow).await,
        },
        Some(Commands::Remove { source }) => commands::remove::run(&source).await,
        Some(Commands::Sync { sources, all }) => commands::sync::run(sources, all).await,
        Some(Commands::Link) => commands::link::run().await,
        Some(Commands::CheckpointAndClear { trigger }) => {
            commands::checkpoint::run(trigger).await
        }
    }
}
