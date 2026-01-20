use anyhow::Result;
use clap::{Parser, Subcommand};

mod admin_client;
mod commands;
mod sources;
mod ui;

#[derive(Parser)]
#[command(name = "minna")]
#[command(about = "Your AI's memory. Local-first. Zero config.")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Connect data sources to Minna
    Add {
        /// Sources to connect (slack, linear, github, notion, atlassian, google)
        /// If omitted, shows interactive picker.
        #[arg(value_name = "SOURCES")]
        sources: Vec<String>,
    },

    /// Show sources, sync progress, and daemon health
    Status {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Configure MCP for your AI tool
    Setup {
        /// AI tool to configure (cursor, claude-code, vscode, windsurf)
        /// If omitted, auto-detects installed tools.
        #[arg(value_name = "TOOL")]
        tool: Option<String>,
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
    // Initialize tracing for debug logs (hidden by default)
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::WARN.into()),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Add { sources } => commands::add::run(sources).await,
        Commands::Status { json } => commands::status::run(json).await,
        Commands::Setup { tool } => commands::setup::run(tool).await,
        Commands::Daemon { command } => match command {
            DaemonCommand::Status => commands::daemon::status().await,
            DaemonCommand::Start => commands::daemon::start().await,
            DaemonCommand::Restart => commands::daemon::restart().await,
            DaemonCommand::Logs { lines, follow } => commands::daemon::logs(lines, follow).await,
        },
        Commands::Remove { source } => commands::remove::run(&source).await,
        Commands::Sync { sources, all } => commands::sync::run(sources, all).await,
    }
}
