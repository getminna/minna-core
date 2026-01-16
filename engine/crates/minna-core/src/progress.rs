//! Structured progress emission for Swift IPC.
//!
//! This module provides functions to emit progress updates and results
//! via stdout, following the MINNA_PROGRESS/MINNA_RESULT protocol.
//! Swift parses these prefixed JSON lines to update UI state.

use std::io::Write;

/// Emit a progress update to stdout for Swift to parse.
///
/// # Arguments
/// * `provider` - The provider name (e.g., "slack", "github", "init")
/// * `status` - Status type: "syncing", "indexing", "error", "cancelled", "init"
/// * `message` - Human-readable progress message
/// * `docs` - Optional count of documents processed so far
///
/// # Protocol
/// Output format: `MINNA_PROGRESS:{"provider":"slack","status":"syncing",...}\n`
pub fn emit_progress(provider: &str, status: &str, message: &str, docs: Option<usize>) {
    let payload = serde_json::json!({
        "provider": provider,
        "status": status,
        "message": message,
        "documents_processed": docs
    });
    println!("MINNA_PROGRESS:{}", serde_json::to_string(&payload).unwrap());
    let _ = std::io::stdout().flush();
}

/// Emit a final result to stdout for Swift to parse.
///
/// # Arguments
/// * `result_type` - The type of result (e.g., "sync", "init", "auth")
/// * `status` - Final status (e.g., "complete", "ready", "error", "cancelled")
/// * `data` - Additional JSON data for the result
///
/// # Protocol
/// Output format: `MINNA_RESULT:{"type":"sync","status":"complete",...}\n`
pub fn emit_result(result_type: &str, status: &str, data: serde_json::Value) {
    let payload = serde_json::json!({
        "type": result_type,
        "status": status,
        "data": data
    });
    println!("MINNA_RESULT:{}", serde_json::to_string(&payload).unwrap());
    let _ = std::io::stdout().flush();
}

/// Emit an error progress update.
///
/// Convenience wrapper for emit_progress with status="error".
pub fn emit_error(provider: &str, message: &str) {
    emit_progress(provider, "error", message, None);
}

/// Emit engine warmup progress (loading embedding model into memory).
///
/// This is called during the ~0.5-2s window when the embedding model
/// is being loaded from disk into RAM. Not a download - just memory loading.
pub fn emit_warmup_progress(message: &str) {
    emit_progress("engine", "warming_up", message, None);
}

/// Emit that the engine is ready.
pub fn emit_ready() {
    emit_result("init", "ready", serde_json::json!({}));
}
