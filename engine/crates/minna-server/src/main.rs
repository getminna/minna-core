use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::RwLock;
use tokio::time::{sleep, Duration};
use tracing::{error, info};

use minna_core::{Core, MinnaPaths, TokenStore, ProviderRegistry, SyncScheduler, SyncPlanner};
use minna_auth_bridge::Provider;
use minna_graph::Ring;
use minna_mcp::{McpContext, McpHandler, ToolRequest, ToolResponse};

/// Shared state that tracks Core initialization
struct ServerState {
    core: RwLock<Option<Core>>,
    paths: MinnaPaths,
    registry: ProviderRegistry,
    scheduler: RwLock<SyncScheduler>,
}

impl ServerState {
    fn new(paths: MinnaPaths) -> Self {
        // Load provider registry (uses defaults if no config file)
        let config_path = paths.base_dir.join("providers.toml");
        let registry = ProviderRegistry::new(&config_path)
            .unwrap_or_else(|_| ProviderRegistry::with_defaults());

        // Initialize scheduler (disabled by default, enabled after Core is ready)
        let mut scheduler = SyncScheduler::new();
        scheduler.set_config(minna_core::SchedulerConfig {
            enabled: false, // Will be enabled after Core initializes
            ..Default::default()
        });

        Self {
            core: RwLock::new(None),
            paths,
            registry,
            scheduler: RwLock::new(scheduler),
        }
    }

    async fn is_ready(&self) -> bool {
        self.core.read().await.is_some()
    }

    async fn get_core(&self) -> Option<Core> {
        self.core.read().await.clone()
    }

    fn get_registry(&self) -> &ProviderRegistry {
        &self.registry
    }

    async fn get_scheduler(&self) -> tokio::sync::RwLockWriteGuard<'_, SyncScheduler> {
        self.scheduler.write().await
    }

    /// Enable the scheduler after Core is ready
    async fn enable_scheduler(&self) {
        let mut scheduler = self.scheduler.write().await;
        let mut config = scheduler.config().clone();
        config.enabled = true;
        scheduler.set_config(config);
        info!("[SCHEDULER] Sync scheduler enabled");
    }
}

// Admin handler for Swift app control commands
#[derive(Clone)]
struct AdminHandler {
    state: Arc<ServerState>,
}

#[derive(Debug, Deserialize)]
struct AdminRequest {
    id: Option<String>,
    tool: Option<String>,
    method: Option<String>,
    #[serde(default)]
    params: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct AdminResponse {
    id: Option<String>,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    event: Option<minna_core::progress::InternalEvent>,
}

impl AdminHandler {
    fn new(state: Arc<ServerState>) -> Self {
        Self { state }
    }

    async fn handle(&self, request: AdminRequest, tx: tokio::sync::mpsc::UnboundedSender<(String, AdminResponse)>) {
        let tool = request.tool.clone().or(request.method.clone());
        let id = request.id.clone();
        let id_log = id.clone().unwrap_or_else(|| "unknown".to_string());

        match tool.as_deref() {
            Some("ping") => {
                let response = AdminResponse {
                    id,
                    ok: true,
                    result: Some(serde_json::json!({"pong": true})),
                    error: None,
                    event: None,
                };
                let _ = tx.send((id_log, response));
            }
            Some("get_status") => {
                let ready = self.state.is_ready().await;
                let scheduler_stats = {
                    let mut scheduler = self.state.get_scheduler().await;
                    scheduler.stats()
                };
                let response = AdminResponse {
                    id,
                    ok: true,
                    result: Some(serde_json::json!({
                        "running": true,
                        "ready": ready,
                        "version": env!("CARGO_PKG_VERSION"),
                        "scheduler": {
                            "pending_syncs": scheduler_stats.pending,
                            "in_progress": scheduler_stats.in_progress,
                            "budget_used": scheduler_stats.budget_used,
                            "budget_total": scheduler_stats.budget_total,
                        }
                    })),
                    error: None,
                    event: None,
                };
                let _ = tx.send((id_log, response));
            }
            Some("verify_credentials") => {
                self.handle_verify_credentials(id, id_log, tx).await;
            }
            Some("sync_provider") => {
                self.handle_sync_provider(id, id_log, request, tx).await;
            }
            Some("discover") => {
                self.handle_discover(id, id_log, request, tx).await;
            }
            Some("reset") => {
                self.handle_reset(id, id_log, request, tx).await;
            }
            _ => {
                let response = AdminResponse {
                    id,
                    ok: false,
                    result: None,
                    error: Some("unknown admin tool".to_string()),
                    event: None,
                };
                let _ = tx.send((id_log, response));
            }
        }
    }

    async fn handle_verify_credentials(&self, id: Option<String>, id_log: String, tx: tokio::sync::mpsc::UnboundedSender<(String, AdminResponse)>) {
        // Load TokenStore directly - works before Core is ready
        let token_store = match TokenStore::load(&self.state.paths.auth_path) {
            Ok(store) => store,
            Err(err) => {
                let response = AdminResponse {
                    id,
                    ok: false,
                    result: None,
                    error: Some(format!("Failed to load credentials: {}", err)),
                    event: None,
                };
                let _ = tx.send((id_log, response));
                return;
            }
        };

        // Check each provider
        let providers = [
            (Provider::Slack, "slack"),
            (Provider::Github, "github"),
            (Provider::Google, "google"),
            (Provider::Linear, "linear"),
        ];

        let mut results = serde_json::Map::new();
        for (provider, name) in providers.iter() {
            let status = match token_store.get(*provider) {
                Some(token) => {
                    let is_expired = token.expires_at
                        .map(|exp| exp < chrono::Utc::now())
                        .unwrap_or(false);

                    if is_expired {
                        serde_json::json!({ "configured": true, "status": "expired", "message": "Token has expired" })
                    } else {
                        serde_json::json!({ "configured": true, "status": "ready", "message": "Credentials found" })
                    }
                }
                None => {
                    serde_json::json!({ "configured": false, "status": "not_configured", "message": "No credentials found" })
                }
            };
            results.insert(name.to_string(), status);
        }

        // Add local providers
        results.insert("cursor".to_string(), serde_json::json!({ "configured": true, "status": "ready", "message": "Local provider" }));
        results.insert("claude_code".to_string(), serde_json::json!({ "configured": true, "status": "ready", "message": "Local provider" }));

        let response = AdminResponse {
            id,
            ok: true,
            result: Some(serde_json::Value::Object(results)),
            error: None,
            event: None,
        };
        let _ = tx.send((id_log, response));
    }

    async fn handle_sync_provider(&self, id: Option<String>, id_log: String, request: AdminRequest, tx: tokio::sync::mpsc::UnboundedSender<(String, AdminResponse)>) {
        
        let core = match self.state.get_core().await {
            Some(c) => c,
            None => {
                let response = AdminResponse {
                    id,
                    ok: false,
                    result: None,
                    error: Some("Engine still initializing, please wait...".to_string()),
                    event: None,
                };
                let _ = tx.send((id_log, response));
                return;
            },
        };

        let provider = request.params.get("provider").and_then(|v| v.as_str()).unwrap_or("");
        let mode = request.params.get("mode").and_then(|v| v.as_str());
        let since_days = request.params.get("since_days").and_then(|v| v.as_u64()).map(|v| v as i64);

        info!("[SYNC_PROVIDER] Starting sync: provider={}, mode={:?}", provider, mode);

        // Subscribe to progress events
        let mut progress_rx = minna_core::progress::subscribe_progress();
        let tx_clone = tx.clone();
        let id_clone = id.clone();
        let id_log_clone = id_log.clone();
        let provider_name = provider.to_string();

        let progress_task = tokio::spawn(async move {
            while let Ok(event) = progress_rx.recv().await {
                let matches = match &event {
                    minna_core::progress::InternalEvent::Progress(p) => p.provider == provider_name,
                    minna_core::progress::InternalEvent::Result(r) => r.result_type == "sync"
                };

                if matches {
                    let response = AdminResponse {
                        id: id_clone.clone(),
                        ok: true,
                        result: None,
                        error: None,
                        event: Some(event),
                    };
                    if tx_clone.send((id_log_clone.clone(), response)).is_err() {
                        break;
                    }
                }
            }
        });

        // Handle local-only
        if provider == "cursor" || provider == "claude_code" {
            let response = AdminResponse {
                id,
                ok: true,
                result: Some(serde_json::json!({ "provider": provider, "status": "complete", "items_synced": 0 })),
                error: None,
                event: None,
            };
            let _ = tx.send((id_log, response));
            progress_task.abort();
            return;
        }

        let result = match provider {
            "google" | "google_drive" | "google_workspace" |
            "github" | "slack" | "linear" | "notion" | "atlassian" | "jira" | "confluence" => {
                let target = match provider {
                    "jira" | "confluence" => "atlassian",
                    "google_drive" | "google_workspace" => "google",
                    _ => provider,
                };
                let registry = self.state.get_registry();
                core.sync_via_registry(registry, target, since_days, mode).await
            },
            _ => {
                let response = AdminResponse {
                    id,
                    ok: false,
                    result: None,
                    error: Some(format!("unknown provider: {}", provider)),
                    event: None,
                };
                let _ = tx.send((id_log, response));
                progress_task.abort();
                return;
            }
        };

        match result {
            Ok(summary) => {
                let api_calls = (summary.documents_processed as u32 / 10).max(1);
                {
                    let mut scheduler = self.state.get_scheduler().await;
                    scheduler.complete_sync(provider, Ring::One, api_calls);
                }
                let response = AdminResponse {
                    id,
                    ok: true,
                    result: Some(serde_json::to_value(summary).unwrap_or_default()),
                    error: None,
                    event: None,
                };
                let _ = tx.send((id_log, response));
            },
            Err(err) => {
                {
                    let mut scheduler = self.state.get_scheduler().await;
                    scheduler.fail_sync(provider);
                }
                let response = AdminResponse {
                    id,
                    ok: false,
                    result: None,
                    error: Some(err.to_string()),
                    event: None,
                };
                let _ = tx.send((id_log, response));
            }
        }
        progress_task.abort();
    }

    async fn handle_discover(&self, id: Option<String>, id_log: String, request: AdminRequest, tx: tokio::sync::mpsc::UnboundedSender<(String, AdminResponse)>) {
        let core = match self.state.get_core().await {
            Some(c) => c,
            None => {
                let response = AdminResponse { id, ok: false, result: None, error: Some("Engine still initializing...".to_string()), event: None };
                let _ = tx.send((id_log, response));
                return;
            }
        };

        let provider = request.params.get("provider").and_then(|v| v.as_str()).unwrap_or("");
        let result = match provider {
            "slack" => core.discover_slack().await,
            "google" | "google_drive" => core.discover_google_drive().await,
            "github" => core.discover_github().await,
            _ => {
                let response = AdminResponse { id, ok: true, result: Some(serde_json::json!({ "provider": provider, "error": "discovery not implemented" })), error: None, event: None };
                let _ = tx.send((id_log, response));
                return;
            }
        };
        
        let response = match result {
            Ok(val) => AdminResponse { id, ok: true, result: Some(val), error: None, event: None },
            Err(err) => AdminResponse { id, ok: false, result: None, error: Some(err.to_string()), event: None },
        };
        let _ = tx.send((id_log, response));
    }

    async fn handle_reset(&self, id: Option<String>, id_log: String, request: AdminRequest, tx: tokio::sync::mpsc::UnboundedSender<(String, AdminResponse)>) {
        let core = match self.state.get_core().await {
            Some(c) => c,
            None => {
                let response = AdminResponse { id, ok: false, result: None, error: Some("Engine still initializing...".to_string()), event: None };
                let _ = tx.send((id_log, response));
                return;
            }
        };

        let provider = request.params.get("provider").and_then(|v| v.as_str()).unwrap_or("");
        let response = match core.reset_provider(provider).await {
            Ok(_) => AdminResponse { id, ok: true, result: Some(serde_json::json!({ "status": "reset_complete" })), error: None, event: None },
            Err(err) => AdminResponse { id, ok: false, result: None, error: Some(err.to_string()), event: None },
        };
        let _ = tx.send((id_log, response));
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Route tracing to stderr so stdout is reserved for MINNA_PROGRESS/MINNA_RESULT
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let paths = MinnaPaths::from_env();
    paths.ensure_dirs()?;

    // Clean up old sockets
    if Path::new(&paths.socket_path).exists() {
        std::fs::remove_file(&paths.socket_path)?;
    }
    if Path::new(&paths.admin_socket_path).exists() {
        std::fs::remove_file(&paths.admin_socket_path)?;
    }

    // Create shared state (Core not yet initialized)
    let state = Arc::new(ServerState::new(paths.clone()));

    // Bind sockets IMMEDIATELY so Swift can connect right away
    let admin_listener = UnixListener::bind(&paths.admin_socket_path)?;
    info!("Admin server listening on {}", paths.admin_socket_path.display());

    // Admin handler for Swift app (control) - works before Core is ready
    let admin_handler = Arc::new(AdminHandler::new(state.clone()));

    // Spawn admin listener immediately so Swift can connect
    let admin_handler_clone = admin_handler.clone();
    tokio::spawn(async move {
        loop {
            match admin_listener.accept().await {
                Ok((stream, _)) => {
                    let handler = admin_handler_clone.clone();
                    tokio::spawn(async move {
                        if let Err(err) = handle_admin_client(stream, handler).await {
                            error!("Admin client error: {}", err);
                        }
                    });
                }
                Err(err) => {
                    error!("Admin accept error: {}", err);
                }
            }
        }
    });

    // Now initialize Core in background (this is the slow part - loading embedding model)
    info!("Initializing engine (loading embedding model)...");
    let state_clone = state.clone();
    let paths_clone = paths.clone();
    tokio::spawn(async move {
        match Core::init(&paths_clone).await {
            Ok(core) => {
                info!("Engine initialized successfully!");
                // Store the initialized core
                *state_clone.core.write().await = Some(core.clone());
                // Emit ready signal to Swift UI
                minna_core::emit_ready();
                // Enable the sync scheduler now that Core is ready
                state_clone.enable_scheduler().await;
                // Start the scheduler background task
                spawn_scheduler_task(state_clone.clone());
                // Start clustering task if enabled
                spawn_cluster_task(core);
            }
            Err(err) => {
                error!("Failed to initialize engine: {}", err);
                minna_core::emit_error("engine", &format!("Failed to initialize: {}", err));
            }
        }
    });

    // MCP socket - bind after a short delay to give Core time to start
    // (MCP queries need Core, so we wait a bit)
    sleep(Duration::from_millis(100)).await;
    let mcp_listener = UnixListener::bind(&paths.socket_path)?;
    info!("MCP server listening on {}", paths.socket_path.display());

    // MCP listener (main loop) - needs Core to be ready for most operations
    let state_for_mcp = state.clone();
    loop {
        let (stream, _) = mcp_listener.accept().await?;
        let state = state_for_mcp.clone();
        tokio::spawn(async move {
            // Wait for Core to be ready before handling MCP requests
            loop {
                if state.is_ready().await {
                    break;
                }
                sleep(Duration::from_millis(100)).await;
            }
            if let Some(core) = state.get_core().await {
                let ctx = McpContext::with_graph(
                    core.ingest.clone(),
                    core.vector.clone(),
                    core.auth.clone(),
                    core.embedder.clone(),
                    core.graph.clone(),
                );
                let handler = Arc::new(McpHandler::new(ctx));
                if let Err(err) = handle_mcp_client(stream, handler).await {
                    error!("MCP client error: {}", err);
                }
            }
        });
    }
}

fn spawn_cluster_task(core: Core) {
    let enabled = std::env::var("MINNA_ENABLE_CLUSTERING")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if !enabled {
        return;
    }
    let interval = std::env::var("MINNA_CLUSTER_INTERVAL_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60 * 60 * 24);
    let min_similarity = std::env::var("MINNA_CLUSTER_MIN_SIMILARITY")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.82f32);
    let min_points = std::env::var("MINNA_CLUSTER_MIN_POINTS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4usize);

    tokio::spawn(async move {
        if let Err(err) = core.run_clustering(min_similarity, min_points).await {
            error!("cluster run failed: {}", err);
        }
        loop {
            sleep(Duration::from_secs(interval)).await;
            if let Err(err) = core.run_clustering(min_similarity, min_points).await {
                error!("cluster run failed: {}", err);
            }
        }
    });
}

/// Spawn the background scheduler task that handles ring-aware sync scheduling.
fn spawn_scheduler_task(state: Arc<ServerState>) {
    let enabled = std::env::var("MINNA_ENABLE_SCHEDULER")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    if !enabled {
        info!("[SCHEDULER] Background scheduler disabled (set MINNA_ENABLE_SCHEDULER=1 to enable)");
        return;
    }

    info!("[SCHEDULER] Starting background scheduler task");

    tokio::spawn(async move {
        // Check every minute for scheduled syncs
        let check_interval = Duration::from_secs(60);

        loop {
            sleep(check_interval).await;

            let core = match state.get_core().await {
                Some(c) => c,
                None => continue,
            };

            // Schedule syncs based on ring assignments
            let providers: Vec<&str> = state.get_registry().list_available();
            {
                let mut scheduler = state.get_scheduler().await;
                if let Err(err) = scheduler.schedule_from_rings(&core.graph, &providers).await {
                    error!("[SCHEDULER] Failed to schedule syncs: {}", err);
                    continue;
                }
            }

            // Process pending syncs
            loop {
                let sync_task = {
                    let mut scheduler = state.get_scheduler().await;
                    scheduler.next_sync()
                };

                let sync_task = match sync_task {
                    Some(t) => t,
                    None => break, // No more pending syncs
                };

                info!(
                    "[SCHEDULER] Executing scheduled sync: provider={}, ring={:?}, depth={:?}",
                    sync_task.provider, sync_task.ring, sync_task.depth
                );

                // Determine sync parameters based on ring
                let (since_days, mode) = SyncPlanner::plan_for_ring(sync_task.ring);

                // Execute sync
                let registry = state.get_registry();
                let result = core.sync_via_registry(
                    registry,
                    &sync_task.provider,
                    since_days,
                    mode,
                ).await;

                // Update scheduler with result
                let mut scheduler = state.get_scheduler().await;
                match result {
                    Ok(summary) => {
                        // Estimate API calls from items synced (rough heuristic)
                        let api_calls = (summary.documents_processed as u32 / 10).max(1);
                        scheduler.complete_sync(&sync_task.provider, sync_task.ring, api_calls);
                        info!(
                            "[SCHEDULER] Sync complete: provider={}, items={}",
                            sync_task.provider, summary.documents_processed
                        );
                    }
                    Err(err) => {
                        scheduler.fail_sync(&sync_task.provider);
                        error!(
                            "[SCHEDULER] Sync failed: provider={}, error={}",
                            sync_task.provider, err
                        );
                    }
                }

                // Small delay between syncs to avoid overwhelming APIs
                sleep(Duration::from_secs(5)).await;
            }
        }
    });
}

async fn handle_mcp_client(
    stream: tokio::net::UnixStream,
    handler: Arc<McpHandler>,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    while let Some(line) = lines.next_line().await? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<ToolRequest>(trimmed) {
            Ok(request) => handler.handle(request).await,
            Err(err) => ToolResponse {
                id: None,
                ok: false,
                result: None,
                error: Some(format!("invalid request: {}", err)),
            },
        };
        let payload = serde_json::to_string(&response)?;
        writer.write_all(payload.as_bytes()).await?;
        writer.write_all(b"\n").await?;
    }
    Ok(())
}

async fn handle_admin_client(
    stream: tokio::net::UnixStream,
    handler: Arc<AdminHandler>,
) -> Result<()> {
    use tokio::sync::mpsc;
    
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();
    
    // Channel to send responses back in order
    let (tx, mut rx) = mpsc::unbounded_channel::<(String, AdminResponse)>();

    // Spawn a task to write responses back in order
    let write_task = tokio::spawn(async move {
        while let Some((_id, response)) = rx.recv().await {
            let payload = match serde_json::to_string(&response) {
                Ok(p) => p,
                Err(err) => {
                    error!("Failed to serialize admin response: {}", err);
                    continue;
                }
            };
            if let Err(err) = writer.write_all(payload.as_bytes()).await {
                error!("Failed to write admin response: {}", err);
                break;
            }
            if let Err(err) = writer.write_all(b"\n").await {
                error!("Failed to write newline: {}", err);
                break;
            }
        }
    });

    // Process requests concurrently
    let mut request_counter = 0u64;
    while let Some(line) = lines.next_line().await? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        
        // Log when a line is received
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
        info!("[ADMIN] Received request line (length: {})", trimmed.len());
        
        // Capture current counter value, then increment for next request
        let current_counter = request_counter;
        request_counter += 1;
        
        let handler_clone = handler.clone();
        
        match serde_json::from_str::<AdminRequest>(trimmed) {
            Ok(request) => {
                let request_id = request.id.clone().unwrap_or_else(|| format!("req_{}", current_counter));
                let tool = request.tool.clone().or(request.method.clone()).unwrap_or_else(|| "unknown".to_string());
                
                // Log request details
                info!("[ADMIN] Parsed request: id={}, tool={}, counter={}", request_id, tool, current_counter);
                
                // Spawn each request handler in its own task so they can run concurrently
                let id_clone = request_id.clone();
                let tx_inner = tx.clone();
                tokio::spawn(async move {
                    let spawn_timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
                    info!("[ADMIN] Handler task spawned: id={}, delay_ms={}", id_clone, spawn_timestamp - timestamp);
                    handler_clone.handle(request, tx_inner).await;
                });
            }
            Err(err) => {
                info!("[ADMIN] Failed to parse request: {}", err);
                let response = AdminResponse {
                    id: None,
                    ok: false,
                    result: None,
                    error: Some(format!("invalid request: {}", err)),
                    event: None,
                };
                let _ = tx.send((format!("req_{}", current_counter), response));
            }
        }
    }
    
    // Close the channel to signal the write task to finish
    drop(tx);
    
    // Wait for write task to finish
    let _ = write_task.await;
    
    Ok(())
}
