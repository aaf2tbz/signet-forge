use forge_core::ForgeError;
use reqwest::Client;
use serde::Deserialize;
use tracing::{debug, warn};

/// HTTP client for the Signet daemon API
#[derive(Clone)]
pub struct SignetClient {
    base_url: String,
    client: Client,
}

#[derive(Debug, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DaemonStatus {
    #[serde(default)]
    pub health_score: Option<f64>,
    #[serde(default)]
    pub sessions: Vec<serde_json::Value>,
    #[serde(default)]
    pub pipeline: Option<serde_json::Value>,
}

impl SignetClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        let client = Client::builder()
            .pool_idle_timeout(std::time::Duration::from_secs(300))
            .pool_max_idle_per_host(4)
            .connect_timeout(std::time::Duration::from_secs(5))
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| Client::new());
        Self {
            base_url: base_url.into(),
            client,
        }
    }

    /// Check if the Signet daemon is running and healthy
    pub async fn health(&self) -> Result<HealthResponse, ForgeError> {
        let resp = self
            .client
            .get(format!("{}/health", self.base_url))
            .send()
            .await
            .map_err(|e| ForgeError::daemon(format!("Daemon not reachable: {e}")))?;

        resp.json::<HealthResponse>()
            .await
            .map_err(|e| ForgeError::daemon(format!("Invalid health response: {e}")))
    }

    /// Check if daemon is available (non-error)
    pub async fn is_available(&self) -> bool {
        match self.health().await {
            Ok(h) => {
                debug!("Signet daemon healthy: {:?}", h);
                true
            }
            Err(e) => {
                warn!("Signet daemon unavailable: {e}");
                false
            }
        }
    }

    /// Get daemon status (sessions, pipeline, health score)
    pub async fn status(&self) -> Result<DaemonStatus, ForgeError> {
        let resp = self
            .client
            .get(format!("{}/api/status", self.base_url))
            .send()
            .await?;

        resp.json::<DaemonStatus>()
            .await
            .map_err(|e| ForgeError::daemon(format!("Invalid status response: {e}")))
    }

    /// Generic GET request to daemon API
    pub async fn get(&self, path: &str) -> Result<serde_json::Value, ForgeError> {
        let resp = self
            .client
            .get(format!("{}{}", self.base_url, path))
            .send()
            .await?;

        resp.json()
            .await
            .map_err(|e| ForgeError::daemon(format!("GET {path} parse error: {e}")))
    }

    /// Generic POST request to daemon API
    pub async fn post(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, ForgeError> {
        let resp = self
            .client
            .post(format!("{}{}", self.base_url, path))
            .json(body)
            .send()
            .await?;

        resp.json()
            .await
            .map_err(|e| ForgeError::daemon(format!("POST {path} parse error: {e}")))
    }

    /// Get total memory count from the daemon
    pub async fn memory_count(&self) -> usize {
        // GET /api/memories returns { memories: [...], stats: { total: N, ... } }
        match self.get("/api/memories").await {
            Ok(resp) => resp
                .get("stats")
                .and_then(|s| s.get("total"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize,
            Err(_) => 0,
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}
