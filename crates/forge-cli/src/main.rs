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

    /// Provider (anthropic, openai, gemini, groq, ollama, openrouter, xai)
    #[arg(long)]
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

    /// Non-interactive mode: send a single prompt and print the response
    #[arg(short = 'p', long = "prompt")]
    prompt: Option<String>,

    /// Color theme (signet-dark, signet-light, midnight, amber)
    #[arg(long, default_value = "signet-dark")]
    theme: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging — stderr so it doesn't interfere with -p output
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "forge=info".into()),
        )
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();

    // Load Signet agent config
    let _agent_config = load_agent_config().unwrap_or_default();
    let provider_name = cli.provider.unwrap_or_else(|| "anthropic".to_string());
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
    let api_key = resolve_key(&signet_client, &provider_name).await?;

    // Create provider
    let provider: Arc<dyn forge_provider::Provider> =
        Arc::from(create_provider(&provider_name, &model, &api_key)?);

    // Build system prompt from Signet identity files
    let identity_prompt = build_identity_prompt();
    let system_prompt = if identity_prompt.is_empty() {
        "You are Forge, a helpful AI coding assistant running in a terminal. \
         Help the user with software engineering tasks."
            .to_string()
    } else {
        identity_prompt
    };

    // Non-interactive mode: send prompt, print response, exit
    if let Some(prompt) = cli.prompt {
        return run_non_interactive(provider, signet_client, system_prompt, &prompt).await;
    }

    // Interactive TUI mode
    let mut terminal = ratatui::init();
    let mut app = App::new(provider, signet_client, system_prompt).await;

    if cli.resume {
        app.resume_last_session().await;
    }

    let result = app.run(&mut terminal).await;

    ratatui::restore();

    result
}

/// Non-interactive mode: single prompt → streamed response → exit
async fn run_non_interactive(
    provider: Arc<dyn forge_provider::Provider>,
    _signet_client: Option<SignetClient>,
    system_prompt: String,
    prompt: &str,
) -> Result<()> {
    use forge_core::Message;
    use forge_provider::{CompletionOpts, StreamEvent};
    use forge_tools;
    use futures::StreamExt;

    let messages = vec![Message::user(prompt)];
    let tools = forge_tools::all_definitions();

    let opts = CompletionOpts {
        system_prompt: Some(system_prompt),
        max_tokens: Some(8192),
        ..Default::default()
    };

    let stream = provider.complete(&messages, &tools, &opts).await?;
    let mut stream = std::pin::pin!(stream);

    while let Some(event) = stream.next().await {
        match event {
            StreamEvent::TextDelta(text) => {
                print!("{text}");
            }
            StreamEvent::Error(e) => {
                eprintln!("\nError: {e}");
                std::process::exit(1);
            }
            StreamEvent::Done => break,
            _ => {}
        }
    }

    println!();
    Ok(())
}

async fn resolve_key(
    signet_client: &Option<SignetClient>,
    provider_name: &str,
) -> Result<String> {
    if provider_name == "ollama" {
        return Ok("ollama".to_string());
    }

    if let Some(client) = signet_client {
        match resolve_api_key(client, provider_name).await {
            Ok(key) => return Ok(key),
            Err(e) => {
                warn!("Signet secret resolution failed: {e}");
            }
        }
    }

    let var_name = format!("{}_API_KEY", provider_name.to_uppercase());
    std::env::var(&var_name).map_err(|_| {
        anyhow::anyhow!(
            "No API key for {provider_name}. Set {var_name} or add to Signet secrets."
        )
    })
}
