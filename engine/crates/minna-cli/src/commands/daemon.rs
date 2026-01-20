use anyhow::{anyhow, Result};
use std::path::PathBuf;
use std::process::Command;

use crate::admin_client::AdminClient;
use crate::ui;

/// Ensure daemon is running and ready. Starts it if needed.
/// Returns Ok(true) if ready, Ok(false) if started but not ready yet.
pub async fn ensure_running() -> Result<bool> {
    let client = AdminClient::new();

    // If already running and ready, we're done
    if client.is_daemon_running() {
        if let Ok(status) = client.get_status().await {
            if status.ready {
                return Ok(true);
            }
            // Running but not ready - wait for it
            return wait_for_ready(&client).await;
        }
    }

    // Not running - start it
    start_internal(false).await?;

    // Wait for ready
    wait_for_ready(&client).await
}

async fn wait_for_ready(client: &AdminClient) -> Result<bool> {
    let spinner = ui::spinner("Waiting for daemon to be ready...");

    for _ in 0..60 {
        // 30 second timeout
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        if let Ok(status) = client.get_status().await {
            if status.ready {
                spinner.finish_and_clear();
                return Ok(true);
            }
        }
    }

    spinner.finish_and_clear();
    ui::info("Daemon is starting but embedding model is still loading.");
    ui::info("This can take 30-60 seconds on first run.");
    Ok(false)
}

async fn start_internal(show_success: bool) -> Result<()> {
    let pid_file = get_pid_file();

    // Check if already running
    if pid_file.exists() {
        let pid_str = std::fs::read_to_string(&pid_file)?;
        if let Ok(pid) = pid_str.trim().parse::<u32>() {
            let is_running = Command::new("kill")
                .args(["-0", &pid.to_string()])
                .status()
                .map(|s| s.success())
                .unwrap_or(false);

            if is_running {
                if show_success {
                    ui::info(&format!("Daemon is already running (pid {})", pid));
                }
                return Ok(());
            }
        }
        // Stale PID file
        let _ = std::fs::remove_file(&pid_file);
    }

    // Find the daemon binary
    let daemon_path = find_daemon_binary()?;

    // Start daemon in background
    let spinner = ui::spinner("Starting daemon...");

    let child = Command::new(&daemon_path).spawn()?;

    // Write PID file
    if let Some(parent) = pid_file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&pid_file, child.id().to_string())?;

    // Wait a moment for startup
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    spinner.finish_and_clear();

    if show_success {
        ui::success(&format!("Daemon started (pid {})", child.id()));
    }

    Ok(())
}

pub async fn status() -> Result<()> {
    let pid_file = get_pid_file();

    if !pid_file.exists() {
        ui::error("Daemon is not running.");
        println!();
        ui::info("Start with: minna daemon start");
        return Ok(());
    }

    let pid_str = std::fs::read_to_string(&pid_file)?;
    let pid: u32 = pid_str.trim().parse()?;

    // Check if process is actually running
    let is_running = Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if is_running {
        ui::success(&format!("Daemon is running (pid {})", pid));

        // Check socket
        let socket_path = get_socket_path();
        if socket_path.exists() {
            ui::info(&format!("Socket: {}", socket_path.display()));
        }
    } else {
        ui::error("Daemon PID file exists but process is not running.");
        ui::info("Restart with: minna daemon restart");

        // Clean up stale PID file
        let _ = std::fs::remove_file(&pid_file);
    }

    Ok(())
}

pub async fn start() -> Result<()> {
    start_internal(true).await
}

pub async fn restart() -> Result<()> {
    let pid_file = get_pid_file();

    // Stop if running
    if pid_file.exists() {
        if let Ok(pid_str) = std::fs::read_to_string(&pid_file) {
            if let Ok(pid) = pid_str.trim().parse::<u32>() {
                let spinner = ui::spinner("Stopping daemon...");

                let _ = Command::new("kill")
                    .args([&pid.to_string()])
                    .status();

                // Wait for process to exit
                for _ in 0..20 {
                    let is_running = Command::new("kill")
                        .args(["-0", &pid.to_string()])
                        .status()
                        .map(|s| s.success())
                        .unwrap_or(false);

                    if !is_running {
                        break;
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }

                spinner.finish_and_clear();
            }
        }

        let _ = std::fs::remove_file(&pid_file);
    }

    // Clean up socket
    let socket_path = get_socket_path();
    let _ = std::fs::remove_file(&socket_path);

    // Start fresh
    start().await
}

pub async fn logs(lines: usize, follow: bool) -> Result<()> {
    let log_file = get_log_file();

    if !log_file.exists() {
        ui::error("No log file found.");
        ui::info(&format!("Expected at: {}", log_file.display()));
        return Ok(());
    }

    let mut args = vec!["-n".to_string(), lines.to_string()];
    if follow {
        args.push("-f".to_string());
    }
    args.push(log_file.to_string_lossy().to_string());

    let status = Command::new("tail")
        .args(&args)
        .status()?;

    if !status.success() {
        return Err(anyhow!("Failed to read logs"));
    }

    Ok(())
}

fn get_pid_file() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".minna/daemon.pid")
}

fn get_socket_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".minna/mcp.sock")
}

fn get_log_file() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".cache")
        })
        .join("minna/logs/daemon.log")
}

fn find_daemon_binary() -> Result<PathBuf> {
    // Check common locations
    let locations = [
        // Same directory as CLI
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.join("minna-core"))),
        // Homebrew
        Some(PathBuf::from("/opt/homebrew/bin/minna-core")),
        Some(PathBuf::from("/usr/local/bin/minna-core")),
        // Cargo
        dirs::home_dir().map(|h| h.join(".cargo/bin/minna-core")),
    ];

    for location in locations.into_iter().flatten() {
        if location.exists() {
            return Ok(location);
        }
    }

    Err(anyhow!(
        "Could not find minna-core daemon binary. \
        Make sure it's installed and in your PATH."
    ))
}
