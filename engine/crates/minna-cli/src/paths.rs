//! Shared path utilities for minna-cli
//!
//! These paths must match minna-core's MinnaPaths::from_env() to ensure
//! the CLI and daemon use the same locations.

use std::path::PathBuf;

/// Get the base Minna data directory.
/// Matches minna-core's MinnaPaths::from_env() logic.
pub fn get_data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("MINNA_DATA_DIR") {
        return PathBuf::from(dir);
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("Minna");
    }
    PathBuf::from(".minna")
}

/// Get the MCP socket path (used by AI clients)
pub fn get_socket_path() -> PathBuf {
    get_data_dir().join("mcp.sock")
}

/// Get the admin socket path (used by CLI to control daemon)
#[allow(dead_code)]
pub fn get_admin_socket_path() -> PathBuf {
    get_data_dir().join("admin.sock")
}

/// Get the daemon PID file path
pub fn get_pid_file() -> PathBuf {
    get_data_dir().join("daemon.pid")
}

/// Get the database path
#[allow(dead_code)]
pub fn get_db_path() -> PathBuf {
    get_data_dir().join("minna.db")
}

/// Get the auth file path
pub fn get_auth_path() -> PathBuf {
    get_data_dir().join("auth.json")
}
