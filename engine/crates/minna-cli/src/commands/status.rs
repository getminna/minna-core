use anyhow::Result;
use console::style;
use serde::Serialize;

#[derive(Serialize)]
struct Status {
    daemon: DaemonStatus,
    sources: Vec<SourceStatus>,
    storage: StorageStatus,
}

#[derive(Serialize)]
struct DaemonStatus {
    status: String,
    pid: Option<u32>,
    uptime_secs: Option<u64>,
}

#[derive(Serialize)]
struct SourceStatus {
    name: String,
    status: String,
    documents: u64,
    last_sync: Option<String>,
}

#[derive(Serialize)]
struct StorageStatus {
    documents: u64,
    vectors: u64,
    db_bytes: u64,
}

pub async fn run(json: bool) -> Result<()> {
    // TODO: Connect to admin socket to get real status
    // For now, show expected format with mock data

    let status = Status {
        daemon: DaemonStatus {
            status: "running".to_string(),
            pid: Some(12847),
            uptime_secs: Some(310920),
        },
        sources: vec![
            SourceStatus {
                name: "slack".to_string(),
                status: "synced".to_string(),
                documents: 1247,
                last_sync: Some("2m ago".to_string()),
            },
            SourceStatus {
                name: "linear".to_string(),
                status: "synced".to_string(),
                documents: 342,
                last_sync: Some("5m ago".to_string()),
            },
        ],
        storage: StorageStatus {
            documents: 1589,
            vectors: 1589,
            db_bytes: 50545459,
        },
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&status)?);
        return Ok(());
    }

    // Human-readable output
    println!();

    // Daemon status
    let daemon_status = if status.daemon.status == "running" {
        format!(
            "{}  (pid {})",
            style("running").green(),
            status.daemon.pid.unwrap_or(0)
        )
    } else {
        format!("{}", style("stopped").red())
    };

    println!("  {:<12} {}", style("daemon").bold(), daemon_status);

    if let Some(uptime) = status.daemon.uptime_secs {
        let days = uptime / 86400;
        let hours = (uptime % 86400) / 3600;
        let mins = (uptime % 3600) / 60;
        println!("  {:<12} {}d {}h {}m", "uptime", days, hours, mins);
    }

    println!();
    println!("  {}", style("SOURCES").bold());
    println!("  {}", "─".repeat(45));

    for source in &status.sources {
        let status_str = match source.status.as_str() {
            "synced" => format!("{}", style("✔ synced").green()),
            "syncing" => format!("{}", style("⚡ syncing").yellow()),
            "error" => format!("{}", style("✖ error").red()),
            _ => source.status.clone(),
        };

        let last_sync = source.last_sync.as_deref().unwrap_or("-");

        println!(
            "  {:<12} {:<15} {:>6} docs    {}",
            source.name, status_str, source.documents, style(last_sync).dim()
        );
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

    println!();

    Ok(())
}
