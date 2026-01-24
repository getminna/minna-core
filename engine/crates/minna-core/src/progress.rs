use std::io::Write;
use serde::{Deserialize, Serialize};
use once_cell::sync::Lazy;
use tokio::sync::broadcast;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressEvent {
    pub provider: String,
    pub status: String,
    pub message: String,
    pub documents_processed: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultEvent {
    pub result_type: String,
    pub status: String,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum InternalEvent {
    Progress(ProgressEvent),
    Result(ResultEvent),
}

static PROGRESS_TX: Lazy<broadcast::Sender<InternalEvent>> = Lazy::new(|| {
    let (tx, _) = broadcast::channel(100);
    tx
});

/// Subscribe to progress events
pub fn subscribe_progress() -> broadcast::Receiver<InternalEvent> {
    PROGRESS_TX.subscribe()
}

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
    let payload = ProgressEvent {
        provider: provider.to_string(),
        status: status.to_string(),
        message: message.to_string(),
        documents_processed: docs,
    };
    
    // 1. Emit to stdout for Swift app
    println!("MINNA_PROGRESS:{}", serde_json::to_string(&payload).unwrap());
    let _ = std::io::stdout().flush();

    // 2. Broadcast to internal channel for Admin Socket
    let _ = PROGRESS_TX.send(InternalEvent::Progress(payload));
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
    let payload = ResultEvent {
        result_type: result_type.to_string(),
        status: status.to_string(),
        data,
    };

    // 1. Emit to stdout for Swift app
    println!("MINNA_RESULT:{}", serde_json::to_string(&payload).unwrap());
    let _ = std::io::stdout().flush();

    // 2. Broadcast to internal channel for Admin Socket
    let _ = PROGRESS_TX.send(InternalEvent::Result(payload));
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
