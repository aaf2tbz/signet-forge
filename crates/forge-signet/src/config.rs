use forge_core::ForgeError;
use serde::Deserialize;
use std::path::PathBuf;
use tracing::{debug, info};

/// Signet agent.yaml configuration (subset relevant to Forge)
#[derive(Debug, Deserialize, Default)]
pub struct AgentConfig {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub memory: Option<MemoryConfig>,
    #[serde(default)]
    pub embedding: Option<EmbeddingConfig>,
}

#[derive(Debug, Deserialize, Default)]
pub struct MemoryConfig {
    #[serde(default)]
    pub database: Option<String>,
    #[serde(default)]
    pub session_budget: Option<usize>,
    #[serde(default)]
    pub decay_rate: Option<f64>,
}

#[derive(Debug, Deserialize, Default)]
pub struct EmbeddingConfig {
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
}

/// Load agent.yaml from the standard Signet location
pub fn load_agent_config() -> Result<AgentConfig, ForgeError> {
    let path = agent_yaml_path();
    debug!("Loading agent config from {}", path.display());

    if !path.exists() {
        info!("No agent.yaml found at {} — using defaults", path.display());
        return Ok(AgentConfig::default());
    }

    let content = std::fs::read_to_string(&path)
        .map_err(|e| ForgeError::config(format!("Failed to read agent.yaml: {e}")))?;

    let config: AgentConfig = serde_yml::from_str(&content)
        .map_err(|e| ForgeError::config(format!("Failed to parse agent.yaml: {e}")))?;

    debug!("Loaded agent config: name={:?}", config.name);
    Ok(config)
}

/// Load an identity file (SOUL.md, IDENTITY.md, USER.md, AGENTS.md)
pub fn load_identity_file(name: &str) -> Option<String> {
    let path = agents_dir().join(name);
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            debug!("Loaded identity file: {name} ({} bytes)", content.len());
            Some(content)
        }
        Err(_) => {
            debug!("Identity file not found: {name}");
            None
        }
    }
}

/// Build the system prompt from Signet identity files
pub fn build_identity_prompt() -> String {
    let mut parts = Vec::new();

    if let Some(agents) = load_identity_file("AGENTS.md") {
        parts.push(format!("## Agent Instructions\n\n{agents}"));
    }

    if let Some(soul) = load_identity_file("SOUL.md") {
        parts.push(format!("## Soul\n\n{soul}"));
    }

    if let Some(identity) = load_identity_file("IDENTITY.md") {
        parts.push(format!("## Identity\n\n{identity}"));
    }

    if let Some(user) = load_identity_file("USER.md") {
        parts.push(format!("## About Your User\n\n{user}"));
    }

    parts.join("\n\n")
}

/// Path to ~/.agents/agent.yaml
pub fn agent_yaml_path() -> PathBuf {
    agents_dir().join("agent.yaml")
}

/// Path to ~/.agents/
pub fn agents_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".agents")
}
