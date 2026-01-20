use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

#[derive(Debug, Serialize)]
pub struct AdminRequest {
    pub id: Option<String>,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct AdminResponse {
    pub id: Option<String>,
    pub ok: bool,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

pub struct AdminClient {
    socket_path: PathBuf,
}

impl AdminClient {
    pub fn new() -> Self {
        Self {
            socket_path: get_admin_socket_path(),
        }
    }

    pub fn is_daemon_running(&self) -> bool {
        self.socket_path.exists()
    }

    async fn send(&self, request: AdminRequest) -> Result<AdminResponse> {
        let mut stream = UnixStream::connect(&self.socket_path).await.map_err(|e| {
            anyhow!(
                "Cannot connect to daemon at {}: {}",
                self.socket_path.display(),
                e
            )
        })?;

        let payload = serde_json::to_string(&request)?;
        stream.write_all(payload.as_bytes()).await?;
        stream.write_all(b"\n").await?;

        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).await?;

        let response: AdminResponse = serde_json::from_str(&line)?;
        Ok(response)
    }

    pub async fn ping(&self) -> Result<bool> {
        let response = self
            .send(AdminRequest {
                id: Some("ping".to_string()),
                method: "ping".to_string(),
                params: None,
            })
            .await?;
        Ok(response.ok)
    }

    pub async fn get_status(&self) -> Result<DaemonStatus> {
        let response = self
            .send(AdminRequest {
                id: Some("status".to_string()),
                method: "get_status".to_string(),
                params: None,
            })
            .await?;

        if !response.ok {
            return Err(anyhow!(
                response.error.unwrap_or_else(|| "Unknown error".to_string())
            ));
        }

        let result = response.result.ok_or_else(|| anyhow!("No result"))?;
        Ok(DaemonStatus {
            running: result["running"].as_bool().unwrap_or(false),
            ready: result["ready"].as_bool().unwrap_or(false),
            version: result["version"].as_str().unwrap_or("unknown").to_string(),
        })
    }

    pub async fn verify_credentials(&self) -> Result<CredentialsStatus> {
        let response = self
            .send(AdminRequest {
                id: Some("verify".to_string()),
                method: "verify_credentials".to_string(),
                params: None,
            })
            .await?;

        if !response.ok {
            return Err(anyhow!(
                response.error.unwrap_or_else(|| "Unknown error".to_string())
            ));
        }

        let result = response.result.ok_or_else(|| anyhow!("No result"))?;
        let mut providers = Vec::new();

        for (name, status) in result.as_object().into_iter().flatten() {
            providers.push(ProviderStatus {
                name: name.clone(),
                configured: status["configured"].as_bool().unwrap_or(false),
                status: status["status"].as_str().unwrap_or("unknown").to_string(),
            });
        }

        Ok(CredentialsStatus { providers })
    }

    pub async fn sync_provider(
        &self,
        provider: &str,
        mode: Option<&str>,
        since_days: Option<i64>,
    ) -> Result<SyncResult> {
        let mut params = serde_json::json!({
            "provider": provider,
        });

        if let Some(m) = mode {
            params["mode"] = serde_json::Value::String(m.to_string());
        }
        if let Some(days) = since_days {
            params["since_days"] = serde_json::Value::Number(days.into());
        }

        let response = self
            .send(AdminRequest {
                id: Some(format!("sync_{}", provider)),
                method: "sync_provider".to_string(),
                params: Some(params),
            })
            .await?;

        if !response.ok {
            return Err(anyhow!(
                response.error.unwrap_or_else(|| "Sync failed".to_string())
            ));
        }

        let result = response.result.unwrap_or_default();
        Ok(SyncResult {
            provider: provider.to_string(),
            status: result["status"]
                .as_str()
                .unwrap_or("complete")
                .to_string(),
            items_synced: result["items_synced"].as_u64().unwrap_or(0) as usize,
            message: result["message"].as_str().map(|s| s.to_string()),
        })
    }
}

#[derive(Debug)]
pub struct DaemonStatus {
    pub running: bool,
    pub ready: bool,
    pub version: String,
}

#[derive(Debug)]
pub struct CredentialsStatus {
    pub providers: Vec<ProviderStatus>,
}

#[derive(Debug)]
pub struct ProviderStatus {
    pub name: String,
    pub configured: bool,
    pub status: String,
}

#[derive(Debug)]
pub struct SyncResult {
    pub provider: String,
    pub status: String,
    pub items_synced: usize,
    pub message: Option<String>,
}

fn get_admin_socket_path() -> PathBuf {
    if let Some(dir) = std::env::var_os("MINNA_DATA_DIR") {
        return PathBuf::from(dir).join("admin.sock");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("Minna")
            .join("admin.sock");
    }
    PathBuf::from(".minna").join("admin.sock")
}
