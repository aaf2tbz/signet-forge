use forge_core::ForgeError;
use reqwest::Client;
use serde::{Deserialize, Serialize};
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

#[derive(Debug, Serialize)]
pub struct HookPayload {
    pub harness: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcript: Option<String>,
}

impl SignetClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            client: Client::new(),
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

    /// Call a hook endpoint on the daemon
    pub async fn call_hook(
        &self,
        hook: &str,
        payload: &HookPayload,
    ) -> Result<serde_json::Value, ForgeError> {
        let resp = self
            .client
            .post(format!("{}/api/hooks/{}", self.base_url, hook))
            .json(payload)
            .send()
            .await
            .map_err(|e| ForgeError::daemon(format!("Hook {hook} failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ForgeError::daemon(format!(
                "Hook {hook} returned {status}: {body}"
            )));
        }

        resp.json()
            .await
            .map_err(|e| ForgeError::daemon(format!("Invalid hook response: {e}")))
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

    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}
