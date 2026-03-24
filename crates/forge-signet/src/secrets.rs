use crate::client::SignetClient;
use forge_core::ForgeError;
use tracing::{debug, warn};

/// Resolve an API key from Signet's secret store
///
/// Uses the daemon's secret_exec endpoint to safely retrieve secrets
/// without exposing them in process memory or logs.
pub async fn resolve_api_key(
    client: &SignetClient,
    provider: &str,
) -> Result<String, ForgeError> {
    let secret_name = provider_to_secret_name(provider);
    debug!("Resolving API key for provider '{provider}' (secret: {secret_name})");

    // Try environment variable first (for direct usage without daemon)
    if let Ok(key) = std::env::var(&secret_name) {
        if !key.is_empty() {
            debug!("Found API key in environment variable {secret_name}");
            return Ok(key);
        }
    }

    // Fall back to Signet's secret store
    let body = serde_json::json!({
        "command": format!("echo ${secret_name}"),
        "secrets": {
            &secret_name: &secret_name,
        }
    });

    let result = client
        .post("/api/secrets/exec", &body)
        .await
        .map_err(|e| {
            ForgeError::ApiKeyMissing(format!(
                "Failed to resolve {secret_name} from Signet: {e}"
            ))
        })?;

    let stdout = result
        .get("stdout")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();

    if stdout.is_empty() {
        warn!("API key {secret_name} is empty — check Signet secrets");
        return Err(ForgeError::ApiKeyMissing(provider.to_string()));
    }

    debug!("Successfully resolved API key for {provider}");
    Ok(stdout)
}

/// Map provider name to the expected environment variable / secret name
fn provider_to_secret_name(provider: &str) -> String {
    match provider {
        "anthropic" => "ANTHROPIC_API_KEY".to_string(),
        "openai" => "OPENAI_API_KEY".to_string(),
        "gemini" | "google" => "GEMINI_API_KEY".to_string(),
        "groq" => "GROQ_API_KEY".to_string(),
        "openrouter" => "OPENROUTER_API_KEY".to_string(),
        other => format!("{}_API_KEY", other.to_uppercase()),
    }
}
