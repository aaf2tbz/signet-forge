use crate::client::SignetClient;
use forge_core::ForgeError;
use tracing::debug;

/// Known provider → secret name mappings
const PROVIDER_SECRETS: &[(&str, &str)] = &[
    ("anthropic", "ANTHROPIC_API_KEY"),
    ("openai", "OPENAI_API_KEY"),
    ("gemini", "GEMINI_API_KEY"),
    ("groq", "GROQ_API_KEY"),
    ("openrouter", "OPENROUTER_API_KEY"),
    ("xai", "XAI_API_KEY"),
];

/// Where an API key / provider was found
#[derive(Debug, Clone, PartialEq)]
pub enum KeySource {
    /// Signet daemon encrypted secret store
    Daemon,
    /// Process environment variable
    Environment,
    /// Installed CLI tool (no API key needed — CLI handles auth)
    Cli { path: String },
}

impl std::fmt::Display for KeySource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeySource::Daemon => write!(f, "daemon"),
            KeySource::Environment => write!(f, "env"),
            KeySource::Cli { .. } => write!(f, "cli"),
        }
    }
}

/// A provider that has an available API key or CLI tool
#[derive(Debug, Clone)]
pub struct DiscoveredProvider {
    pub provider: String,
    pub secret_name: String,
    pub source: KeySource,
}

/// List all secret names stored in the Signet daemon
pub async fn list_daemon_secrets(client: &SignetClient) -> Result<Vec<String>, ForgeError> {
    let resp = client.get("/api/secrets").await?;
    let secrets = resp
        .get("secrets")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    Ok(secrets)
}

/// Discover all providers available — daemon secrets, env vars, and installed CLI tools.
pub async fn discover_available_providers(
    client: Option<&SignetClient>,
) -> Vec<DiscoveredProvider> {
    let mut found = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // 1. Check daemon secret store (preferred — encrypted, managed)
    if let Some(client) = client {
        match list_daemon_secrets(client).await {
            Ok(daemon_secrets) => {
                for (provider, secret_name) in PROVIDER_SECRETS {
                    if daemon_secrets.iter().any(|s| s == secret_name) {
                        debug!("Found {secret_name} in daemon for {provider}");
                        found.push(DiscoveredProvider {
                            provider: provider.to_string(),
                            secret_name: secret_name.to_string(),
                            source: KeySource::Daemon,
                        });
                        seen.insert(provider.to_string());
                    }
                }
            }
            Err(e) => {
                debug!("Could not list daemon secrets: {e}");
            }
        }
    }

    // 2. Check environment variables for providers not found in daemon
    for (provider, secret_name) in PROVIDER_SECRETS {
        if seen.contains(*provider) {
            continue;
        }
        if let Ok(val) = std::env::var(secret_name) {
            if !val.is_empty() {
                debug!("Found {secret_name} in environment for {provider}");
                found.push(DiscoveredProvider {
                    provider: provider.to_string(),
                    secret_name: secret_name.to_string(),
                    source: KeySource::Environment,
                });
                seen.insert(provider.to_string());
            }
        }
    }

    // 3. Detect installed CLI tools
    let cli_checks: &[(&str, &str)] = &[
        ("claude-cli", "claude"),
        ("codex-cli", "codex"),
        ("gemini-cli", "gemini"),
    ];

    for (provider_name, binary) in cli_checks {
        if let Ok(output) = tokio::process::Command::new("which")
            .arg(binary)
            .output()
            .await
        {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path.is_empty() {
                    debug!("Found CLI tool: {binary} at {path}");
                    found.push(DiscoveredProvider {
                        provider: provider_name.to_string(),
                        secret_name: format!("{binary} (installed)"),
                        source: KeySource::Cli { path },
                    });
                }
            }
        }
    }

    // 4. Ollama is always available (local, no key)
    if !seen.contains("ollama") {
        found.push(DiscoveredProvider {
            provider: "ollama".to_string(),
            secret_name: String::new(),
            source: KeySource::Environment,
        });
    }

    found
}

/// Resolve the actual API key value for a provider.
/// Checks environment first (fastest), then daemon secret store.
pub async fn resolve_api_key(
    client: Option<&SignetClient>,
    provider: &str,
) -> Result<String, ForgeError> {
    if provider == "ollama" {
        return Ok(String::new());
    }

    // CLI providers don't need API keys
    if provider.ends_with("-cli") {
        return Ok(String::new());
    }

    let secret_name = provider_to_secret_name(provider);

    // Environment variable — fastest, no network round-trip
    if let Ok(key) = std::env::var(&secret_name) {
        if !key.is_empty() {
            debug!("Resolved {secret_name} from environment");
            return Ok(key);
        }
    }

    // Daemon secret store — exec printenv to read the injected value
    if let Some(client) = client {
        let body = serde_json::json!({
            "command": format!("printenv {secret_name}"),
            "secrets": {
                &secret_name: &secret_name,
            }
        });

        match client.post("/api/secrets/exec", &body).await {
            Ok(result) => {
                let code = result.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
                let stdout = result
                    .get("stdout")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();

                if code == 0 && !stdout.is_empty() {
                    debug!("Resolved {secret_name} from daemon secret store");
                    return Ok(stdout);
                }
                debug!(
                    "Daemon exec returned code={code}, stdout empty={} for {secret_name}",
                    stdout.is_empty()
                );
            }
            Err(e) => {
                debug!("Daemon secret exec failed for {secret_name}: {e}");
            }
        }
    }

    Err(ForgeError::ApiKeyMissing(format!(
        "No API key found for {provider} ({secret_name})"
    )))
}

/// Map provider name to its expected secret/env var name
pub fn provider_to_secret_name(provider: &str) -> String {
    for (p, s) in PROVIDER_SECRETS {
        if *p == provider {
            return s.to_string();
        }
    }
    format!("{}_API_KEY", provider.to_uppercase())
}

/// Default model for each provider
pub fn default_model_for_provider(provider: &str) -> &'static str {
    match provider {
        "anthropic" | "claude-cli" => "claude-sonnet-4-6",
        "openai" | "codex-cli" => "gpt-4o",
        "gemini" | "gemini-cli" => "gemini-2.5-flash",
        "groq" => "llama-3.3-70b-versatile",
        "ollama" => "qwen3:4b",
        "openrouter" => "anthropic/claude-sonnet-4-6",
        "xai" => "grok-3",
        _ => "claude-sonnet-4-6",
    }
}
