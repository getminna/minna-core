use anyhow::Result;
use console::style;
use minna_graph::{GraphStore, IdentityService};
use serde::Serialize;
use sqlx::sqlite::SqlitePoolOptions;
use std::path::PathBuf;

use crate::admin_client::AdminClient;
use crate::ui;

#[derive(Serialize)]
struct Status {
    daemon: DaemonStatusJson,
    sources: Vec<SourceStatus>,
    storage: StorageStatus,
    identity: IdentityStatus,
}

#[derive(Serialize)]
struct IdentityStatus {
    pending_links: usize,
}

#[derive(Serialize)]
struct DaemonStatusJson {
    status: String,
    pid: Option<u32>,
    uptime_secs: Option<u64>,
    version: Option<String>,
    ready: bool,
}

#[derive(Serialize)]
struct SourceStatus {
    name: String,
    status: String,
    configured: bool,
    documents: Option<u64>,
    last_sync: Option<String>,
}

#[derive(Serialize)]
struct StorageStatus {
    documents: u64,
    vectors: u64,
    db_bytes: u64,
}

pub async fn run(json: bool) -> Result<()> {
    let client = AdminClient::new();

    // Check if daemon is running
    if !client.is_daemon_running() {
        if json {
            let status = Status {
                daemon: DaemonStatusJson {
                    status: "stopped".to_string(),
                    pid: None,
                    uptime_secs: None,
                    version: None,
                    ready: false,
                },
                sources: vec![],
                storage: StorageStatus {
                    documents: 0,
                    vectors: 0,
                    db_bytes: 0,
                },
                identity: IdentityStatus { pending_links: 0 },
            };
            println!("{}", serde_json::to_string_pretty(&status)?);
            return Ok(());
        }

        println!();
        ui::error("Daemon not running.");
        println!();
        ui::info("Start it with:");
        println!("    minna daemon start");
        println!();
        return Ok(());
    }

    // Get daemon status
    let daemon_status = match client.get_status().await {
        Ok(s) => s,
        Err(e) => {
            if json {
                let status = Status {
                    daemon: DaemonStatusJson {
                        status: "error".to_string(),
                        pid: None,
                        uptime_secs: None,
                        version: None,
                        ready: false,
                    },
                    sources: vec![],
                    storage: StorageStatus {
                        documents: 0,
                        vectors: 0,
                        db_bytes: 0,
                    },
                    identity: IdentityStatus { pending_links: 0 },
                };
                println!("{}", serde_json::to_string_pretty(&status)?);
                return Ok(());
            }
            ui::error(&format!("Cannot connect to daemon: {}", e));
            return Ok(());
        }
    };

    // Get credentials status
    let creds_status = client.verify_credentials().await.ok();

    // Get database stats
    let db_stats = get_db_stats();

    // Get pending identity links
    let pending_links = get_pending_identity_links().await.unwrap_or(0);

    // Build sources list from credentials
    let sources: Vec<SourceStatus> = if let Some(creds) = &creds_status {
        creds
            .providers
            .iter()
            .filter(|p| {
                // Only show real data sources, not local ones
                !matches!(p.name.as_str(), "cursor" | "claude_code")
            })
            .map(|p| SourceStatus {
                name: p.name.clone(),
                status: p.status.clone(),
                configured: p.configured,
                documents: None, // TODO: Get per-source counts
                last_sync: None, // TODO: Get last sync time
            })
            .collect()
    } else {
        vec![]
    };

    let status = Status {
        daemon: DaemonStatusJson {
            status: if daemon_status.running {
                "running".to_string()
            } else {
                "stopped".to_string()
            },
            pid: None, // TODO: Get actual PID
            uptime_secs: None, // TODO: Track uptime
            version: Some(daemon_status.version),
            ready: daemon_status.ready,
        },
        sources,
        storage: db_stats,
        identity: IdentityStatus { pending_links },
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&status)?);
        return Ok(());
    }

    // Human-readable output
    println!();

    // Daemon status
    let daemon_display = if status.daemon.status == "running" {
        if status.daemon.ready {
            format!("{}", style("running").green())
        } else {
            format!("{}", style("starting...").yellow())
        }
    } else {
        format!("{}", style("stopped").red())
    };

    println!("  {:<12} {}", style("daemon").bold(), daemon_display);
    if let Some(version) = &status.daemon.version {
        println!("  {:<12} v{}", "version", version);
    }

    println!();
    println!("  {}", style("SOURCES").bold());
    println!("  {}", "─".repeat(45));

    if status.sources.is_empty() {
        println!("  {}", style("No sources connected").dim());
        println!();
        println!("  Add a source with:");
        println!("    minna add slack");
    } else {
        for source in &status.sources {
            let status_str = if !source.configured {
                format!("{}", style("○ not configured").dim())
            } else {
                match source.status.as_str() {
                    "ready" => format!("{}", style("✔ ready").green()),
                    "expired" => format!("{}", style("⚠ expired").yellow()),
                    "syncing" => format!("{}", style("⚡ syncing").yellow()),
                    "error" => format!("{}", style("✖ error").red()),
                    _ => format!("{}", style(&source.status).dim()),
                }
            };

            let docs = source
                .documents
                .map(|d| format!("{:>6} docs", d))
                .unwrap_or_else(|| "         ".to_string());

            let last_sync = source.last_sync.as_deref().unwrap_or("");

            println!(
                "  {:<12} {:<18} {}    {}",
                source.name,
                status_str,
                docs,
                style(last_sync).dim()
            );
        }
    }

    println!();
    println!("  {}", style("STORAGE").bold());
    println!("  {}", "─".repeat(45));
    println!("  {:<12} {}", "documents", status.storage.documents);
    println!("  {:<12} {}", "vectors", status.storage.vectors);
    println!(
        "  {:<12} {:.1} MB",
        "db size",
        status.storage.db_bytes as f64 / 1_000_000.0
    );

    // Show identity linking suggestions if any
    if status.identity.pending_links > 0 {
        println!();
        println!(
            "  {} {} new accounts found across sources.",
            style("Identity:").cyan().bold(),
            status.identity.pending_links
        );
        println!(
            "  {} to review and confirm.",
            style("$ minna link").dim()
        );
    }

    println!();

    Ok(())
}

fn get_db_stats() -> StorageStatus {
    // Try to get database size
    let db_path = get_db_path();
    let db_bytes = std::fs::metadata(&db_path)
        .map(|m| m.len())
        .unwrap_or(0);

    // TODO: Get actual counts from database
    StorageStatus {
        documents: 0,
        vectors: 0,
        db_bytes,
    }
}

fn get_db_path() -> PathBuf {
    if let Some(dir) = std::env::var_os("MINNA_DATA_DIR") {
        return PathBuf::from(dir).join("minna.db");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("Minna")
            .join("minna.db");
    }
    PathBuf::from(".minna").join("minna.db")
}

/// Get the count of pending identity link suggestions.
async fn get_pending_identity_links() -> Result<usize> {
    let db_path = get_db_path();
    if !db_path.exists() {
        return Ok(0);
    }

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&format!("sqlite:{}", db_path.display()))
        .await?;

    let graph = GraphStore::new(pool);
    IdentityService::pending_suggestions_count(&graph).await
}
