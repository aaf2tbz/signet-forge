use anyhow::Result;
use clap::Parser;
use forge_provider::create_provider;
use forge_signet::config::{build_identity_prompt, load_agent_config};
use forge_signet::secrets::resolve_api_key;
use forge_signet::SignetClient;
use forge_tui::App;
use std::sync::Arc;
use tracing::{info, warn};

#[derive(Parser)]
#[command(name = "forge", version, about = "Signet's native AI terminal")]
struct Cli {
    /// Model to use (e.g., claude-sonnet-4-6, claude-opus-4-6)
    #[arg(short, long)]
    model: Option<String>,

    /// Provider (anthropic, openai, gemini, groq, ollama)
    #[arg(short, long)]
    provider: Option<String>,

    /// Signet daemon URL
    #[arg(long, default_value = "http://localhost:3850")]
    daemon_url: String,

    /// Run without connecting to Signet daemon
    #[arg(long)]
    no_daemon: bool,

    /// Resume the last session
    #[arg(long)]
    resume: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "forge=info".into()),
        )
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();

    // Load Signet agent config
    let _agent_config = load_agent_config().unwrap_or_default();
    let provider_name = cli
        .provider
        .unwrap_or_else(|| "anthropic".to_string());
    let model = cli
        .model
        .unwrap_or_else(|| "claude-sonnet-4-6".to_string());

    info!("Forge starting — provider: {provider_name}, model: {model}");

    // Connect to Signet daemon
    let signet_client = if cli.no_daemon {
        warn!("Running without Signet daemon — memory and identity disabled");
        None
    } else {
        let client = SignetClient::new(&cli.daemon_url);
        if client.is_available().await {
            info!("Connected to Signet daemon at {}", cli.daemon_url);
            Some(client)
        } else {
            warn!(
                "Signet daemon not available at {} — running in standalone mode",
                cli.daemon_url
            );
            None
        }
    };

    // Resolve API key
    let api_key = if let Some(client) = &signet_client {
        match resolve_api_key(client, &provider_name).await {
            Ok(key) => key,
            Err(e) => {
                // Fall back to environment variable
                warn!("Signet secret resolution failed: {e}");
                std::env::var(format!(
                    "{}_API_KEY",
                    provider_name.to_uppercase()
                ))
                .map_err(|_| anyhow::anyhow!(
                    "No API key found for {provider_name}. Set {}_API_KEY or add to Signet secrets.",
                    provider_name.to_uppercase()
                ))?
            }
        }
    } else {
        std::env::var(format!(
            "{}_API_KEY",
            provider_name.to_uppercase()
        ))
        .map_err(|_| anyhow::anyhow!(
            "No API key found for {provider_name}. Set {}_API_KEY env var.",
            provider_name.to_uppercase()
        ))?
    };

    // Create provider
    let provider: Arc<dyn forge_provider::Provider> =
        Arc::from(create_provider(&provider_name, &model, &api_key)?);

    // Build system prompt from Signet identity files
    let identity_prompt = build_identity_prompt();
    let system_prompt = if identity_prompt.is_empty() {
        "You are Forge, a helpful AI coding assistant running in a terminal. Help the user with software engineering tasks.".to_string()
    } else {
        identity_prompt
    };

    // Initialize TUI
    let mut terminal = ratatui::init();
    let mut app = App::new(provider, signet_client, system_prompt).await;

    // Run the app
    let result = app.run(&mut terminal).await;

    // Restore terminal
    ratatui::restore();

    result
}
