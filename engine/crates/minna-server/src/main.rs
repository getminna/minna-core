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

use minna_core::{Core, MinnaPaths, TokenStore, ProviderRegistry};
use minna_auth_bridge::Provider;
use minna_mcp::{McpContext, McpHandler, ToolRequest, ToolResponse};

/// Shared state that tracks Core initialization
struct ServerState {
    core: RwLock<Option<Core>>,
    paths: MinnaPaths,
    registry: ProviderRegistry,
}

impl ServerState {
    fn new(paths: MinnaPaths) -> Self {
        // Load provider registry (uses defaults if no config file)
        let config_path = paths.base_dir.join("providers.toml");
        let registry = ProviderRegistry::new(&config_path)
            .unwrap_or_else(|_| ProviderRegistry::with_defaults());

        Self {
            core: RwLock::new(None),
            paths,
            registry,
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
}

impl AdminHandler {
    fn new(state: Arc<ServerState>) -> Self {
        Self { state }
    }

    async fn handle(&self, request: AdminRequest) -> AdminResponse {
        let tool = request.tool.clone().or(request.method.clone());
        let id = request.id.clone();

        match tool.as_deref() {
            // These tools work even before Core is ready
            Some("ping") => {
                AdminResponse {
                    id,
                    ok: true,
                    result: Some(serde_json::json!({"pong": true})),
                    error: None,
                }
            }
            Some("get_status") => {
                let ready = self.state.is_ready().await;
                AdminResponse {
                    id,
                    ok: true,
                    result: Some(serde_json::json!({
                        "running": true,
                        "ready": ready,
                        "version": env!("CARGO_PKG_VERSION"),
                    })),
                    error: None,
                }
            }
            Some("verify_credentials") => {
                // Load TokenStore directly - works before Core is ready
                let token_store = match TokenStore::load(&self.state.paths.auth_path) {
                    Ok(store) => store,
                    Err(err) => return AdminResponse {
                        id,
                        ok: false,
                        result: None,
                        error: Some(format!("Failed to load credentials: {}", err)),
                    },
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
                            // Check if token is expired
                            let is_expired = token.expires_at
                                .map(|exp| exp < chrono::Utc::now())
                                .unwrap_or(false);

                            if is_expired {
                                serde_json::json!({
                                    "configured": true,
                                    "status": "expired",
                                    "message": "Token has expired, needs refresh"
                                })
                            } else {
                                serde_json::json!({
                                    "configured": true,
                                    "status": "ready",
                                    "message": "Credentials found"
                                })
                            }
                        }
                        None => {
                            serde_json::json!({
                                "configured": false,
                                "status": "not_configured",
                                "message": "No credentials found"
                            })
                        }
                    };
                    results.insert(name.to_string(), status);
                }

                // Add local providers (always ready)
                results.insert("cursor".to_string(), serde_json::json!({
                    "configured": true,
                    "status": "ready",
                    "message": "Local provider"
                }));
                results.insert("claude_code".to_string(), serde_json::json!({
                    "configured": true,
                    "status": "ready",
                    "message": "Local provider"
                }));

                AdminResponse {
                    id,
                    ok: true,
                    result: Some(serde_json::Value::Object(results)),
                    error: None,
                }
            }

            // These tools require Core to be ready
            Some("sync_provider") => {
                let handler_start = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
                info!("[SYNC_PROVIDER] Handler started: id={:?}", id);
                
                let core = match self.state.get_core().await {
                    Some(c) => c,
                    None => {
                        info!("[SYNC_PROVIDER] Core not ready: id={:?}", id);
                        return AdminResponse {
                            id,
                            ok: false,
                            result: None,
                            error: Some("Engine still initializing, please wait...".to_string()),
                        };
                    },
                };

                let provider = request.params.get("provider")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let mode = request.params.get("mode")
                    .and_then(|v| v.as_str());
                let since_days = request.params.get("since_days")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as i64);

                info!("[SYNC_PROVIDER] Starting sync: id={:?}, provider={}, mode={:?}, since_days={:?}", id, provider, mode, since_days);

                // Handle local-only providers that don't need network sync
                if provider == "cursor" || provider == "claude_code" {
                    info!("[SYNC_PROVIDER] Local provider, returning immediately: provider={}", provider);
                    return AdminResponse {
                        id,
                        ok: true,
                        result: Some(serde_json::json!({
                            "provider": provider,
                            "status": "complete",
                            "message": "Local indexing not yet implemented",
                            "items_synced": 0
                        })),
                        error: None,
                    };
                }

                let sync_start = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
                info!("[SYNC_PROVIDER] Calling sync function: provider={}, delay_from_handler_start_ms={}", provider, sync_start - handler_start);
                
                let result = match provider {
                    // All providers now use the registry with Gravity Well edge extraction
                    "google" | "google_drive" | "google_workspace" |
                    "github" | "slack" | "linear" | "notion" | "atlassian" | "jira" | "confluence" => {
                        let provider_name = match provider {
                            "jira" | "confluence" => "atlassian",
                            "google_drive" | "google_workspace" => "google",
                            _ => provider,
                        };
                        info!("[SYNC_PROVIDER] Calling sync_via_registry for {}", provider_name);
                        let registry = self.state.get_registry();
                        core.sync_via_registry(registry, provider_name, since_days, mode).await
                    },
                    _ => {
                        info!("[SYNC_PROVIDER] Unknown provider: {}", provider);
                        return AdminResponse {
                            id,
                            ok: false,
                            result: None,
                            error: Some(format!("unknown provider: {}", provider)),
                        };
                    },
                };
                
                let sync_end = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
                info!("[SYNC_PROVIDER] Sync function completed: provider={}, duration_ms={}", provider, sync_end - sync_start);

                match result {
                    Ok(summary) => AdminResponse {
                        id,
                        ok: true,
                        result: Some(serde_json::to_value(summary).unwrap_or_default()),
                        error: None,
                    },
                    Err(err) => AdminResponse {
                        id,
                        ok: false,
                        result: None,
                        error: Some(err.to_string()),
                    },
                }
            }
            Some("discover") => {
                let handler_start = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
                info!("[DISCOVER] Handler started: id={:?}", id);
                
                let core = match self.state.get_core().await {
                    Some(c) => c,
                    None => {
                        info!("[DISCOVER] Core not ready: id={:?}", id);
                        return AdminResponse {
                            id,
                            ok: false,
                            result: None,
                            error: Some("Engine still initializing, please wait...".to_string()),
                        };
                    },
                };

                let provider = request.params.get("provider")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                info!("[DISCOVER] Starting discovery: id={:?}, provider={}", id, provider);

                let discover_start = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
                info!("[DISCOVER] Calling discover function: provider={}, delay_from_handler_start_ms={}", provider, discover_start - handler_start);
                
                let result = match provider {
                    "slack" => {
                        info!("[DISCOVER] Calling discover_slack");
                        core.discover_slack().await
                    }
                    "google" | "google_drive" => {
                        info!("[DISCOVER] Calling discover_google_drive");
                        core.discover_google_drive().await
                    }
                    "github" => {
                        info!("[DISCOVER] Calling discover_github");
                        core.discover_github().await
                    }
                    _ => {
                        info!("[DISCOVER] Unknown provider: {}", provider);
                        return AdminResponse {
                            id,
                            ok: true,
                            result: Some(serde_json::json!({
                                "provider": provider,
                                "error": "discovery not implemented for this provider"
                            })),
                            error: None,
                        };
                    }
                };
                
                let discover_end = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
                info!("[DISCOVER] Discover function completed: provider={}, duration_ms={}", provider, discover_end - discover_start);
                
                match result {
                    Ok(val) => {
                        info!("[DISCOVER] Discovery succeeded: provider={}, id={:?}", provider, id);
                        AdminResponse {
                            id,
                            ok: true,
                            result: Some(val),
                            error: None,
                        }
                    }
                    Err(err) => {
                        let err_str = err.to_string();
                        info!("[DISCOVER] Discovery failed: provider={}, id={:?}, error={}", provider, id, err_str);
                        AdminResponse {
                            id,
                            ok: false,
                            result: None,
                            error: Some(err_str),
                        }
                    }
                }
            }
            Some("reset") => {
                let core = match self.state.get_core().await {
                    Some(c) => c,
                    None => return AdminResponse {
                        id,
                        ok: false,
                        result: None,
                        error: Some("Engine still initializing, please wait...".to_string()),
                    },
                };

                let provider = request.params.get("provider")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                match core.reset_provider(provider).await {
                    Ok(_) => AdminResponse {
                        id,
                        ok: true,
                        result: Some(serde_json::json!({ "status": "reset_complete" })),
                        error: None,
                    },
                    Err(err) => AdminResponse {
                        id,
                        ok: false,
                        result: None,
                        error: Some(err.to_string()),
                    },
                }
            }
            _ => AdminResponse {
                id,
                ok: false,
                result: None,
                error: Some("unknown admin tool".to_string()),
            },
        }
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
                let ctx = McpContext::new(
                    core.ingest.clone(),
                    core.vector.clone(),
                    core.auth.clone(),
                    core.embedder.clone(),
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
        let tx_clone = tx.clone();
        
        match serde_json::from_str::<AdminRequest>(trimmed) {
            Ok(request) => {
                let request_id = request.id.clone().unwrap_or_else(|| format!("req_{}", current_counter));
                let tool = request.tool.clone().or(request.method.clone()).unwrap_or_else(|| "unknown".to_string());
                
                // Log request details
                info!("[ADMIN] Parsed request: id={}, tool={}, counter={}", request_id, tool, current_counter);
                
                // Spawn each request handler in its own task so they can run concurrently
                let request_id_log = request_id.clone();
                let tool_log = tool.clone();
                tokio::spawn(async move {
                    let spawn_timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
                    info!("[ADMIN] Handler task spawned: id={}, tool={}, delay_ms={}", request_id_log, tool_log, spawn_timestamp - timestamp);
                    let handler_start = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
                    let response = handler_clone.handle(request).await;
                    let handler_end = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
                    info!("[ADMIN] Handler completed: id={}, tool={}, duration_ms={}", request_id_log, tool_log, handler_end - handler_start);
                    let _ = tx_clone.send((request_id_log, response));
                });
            }
            Err(err) => {
                info!("[ADMIN] Failed to parse request: {}", err);
                let response = AdminResponse {
                    id: None,
                    ok: false,
                    result: None,
                    error: Some(format!("invalid request: {}", err)),
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
