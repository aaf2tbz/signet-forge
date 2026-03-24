# Signet Forge

**Signet's native AI terminal.** A Rust TUI that calls provider APIs directly, executes tools natively, and integrates deeply with the Signet daemon for memory, extraction, identity, and session management.

## Why Forge Exists

Signet currently operates as a **passenger** inside other AI tools. It hooks into Claude Code, OpenCode, and Codex via connector adapters and shell hooks — injecting memories through subprocess calls, scraping transcripts after sessions end, and hoping the host harness cooperates. Every harness has its own config format, session model, and tool system. Connectors must be written and maintained for each one.

**Forge inverts this.** Signet becomes the harness.

### What changes

| Today (Connector Model) | Forge (Native Terminal) |
|---|---|
| Signet hooks into Claude Code via shell subprocesses | Forge calls daemon HTTP API directly |
| ~200ms latency per hook call | ~5ms for localhost HTTP |
| Each harness needs a maintained connector package | Zero connector maintenance |
| Model locked to whatever the harness supports | Any provider, hot-swappable mid-session |
| Config scattered across settings.json + agent.yaml + .opencode.json | One config: agent.yaml |
| Dashboard requires opening a browser to :3850 | Dashboard data rendered in TUI panels |
| Identity files injected via hook stdout parsing | Identity loaded directly at startup |
| Extraction triggered by session-end hook (fragile timing) | Extraction triggered by native session lifecycle |
| Skills routed through harness slash command system | Skills loaded and executed natively |
| MCP proxied through harness plugin system | MCP client built-in |

### How it improves Signet's operation

**Memory integration becomes zero-overhead.** Instead of shell hooks that spawn subprocesses, parse stdout, and relay data through environment variables, Forge calls the daemon's HTTP API directly from native Rust. Memory injection on every prompt goes from ~200ms (subprocess spawn + HTTP + stdout parse) to ~5ms (direct HTTP on localhost).

**Session lifecycle is first-class.** Forge controls when sessions start, when context compacts, and when extraction runs. No more relying on the host harness to fire the right hook at the right time. The session-end hook's transcript submission — currently the most fragile part of the pipeline — becomes a direct, reliable HTTP POST.

**Model-provider independence.** Signet is no longer locked to whatever model the host tool supports. Forge talks to Anthropic, OpenAI, Gemini, Groq, Ollama, and OpenRouter directly. Switch from Claude to GPT mid-conversation. Use Ollama for local extraction. The provider is a config value, not an infrastructure dependency.

**Identity loads natively.** AGENTS.md, SOUL.md, IDENTITY.md, USER.md — the files that define who the agent is — are read directly from `~/.agents/` at startup. No hook output parsing, no character limits, no truncation from subprocess stdout buffering.

**API keys come from Signet's secret store.** No separate `.env` files or per-tool key management. Forge resolves keys through the daemon's secret API on startup, caches them in memory. Switch providers and the key resolves automatically.

**Config sync is bidirectional.** Forge watches `~/.agents/agent.yaml` for changes (via filesystem notifications) and updates in real-time. It can also write config changes back — change the extraction model from the terminal UI and it syncs to agent.yaml.

**Connector maintenance drops to zero.** No more `connector-claude-code`, `connector-opencode`, `connector-codex`, `connector-openclaw`. Each connector is a maintenance burden that breaks when the upstream tool changes its API. Forge eliminates all of them. Signet's memory, identity, and skills become the platform; LLM providers become interchangeable backends.

### Model Architecture: Four Models, One System

Signet operates with four separate model configurations. Understanding this is critical:

| Model | Who Controls It | Where Configured | Default |
|---|---|---|---|
| **Conversational** | Forge (model picker / CLI) | `forge --model` or Ctrl+O | claude-sonnet-4-6 |
| **Synthesis** | Daemon summary worker | `agent.yaml` → `pipelineV2.synthesis` | claude-code/haiku |
| **Extraction** | Daemon extraction worker | `agent.yaml` → `pipelineV2.extraction` | qwen3:4b (Ollama) |
| **Embedding** | Daemon vector search | `agent.yaml` → `embedding` | nomic-embed-text (Ollama) |

**Switching the conversational model in Forge does NOT affect extraction or embedding.** The daemon manages its own models independently. Extraction and embedding typically run on local Ollama models (qwen3:4b and nomic-embed-text) for zero-cost, low-latency processing. The conversational model can be any cloud provider.

When Forge calls the session-end hook, it sends the raw transcript. The daemon then:
1. Queues a summary job → uses the **synthesis model** to extract facts
2. Processes extraction jobs → uses the **extraction model** for deeper analysis
3. Computes embeddings → uses the **embedding model** for vector search indexing

All three steps happen asynchronously after Forge sends the transcript. Forge never calls these models directly.

### Supported Providers

| Provider | Models | API |
|---|---|---|
| **Anthropic** | Claude Opus 4.6, Sonnet 4.6, Haiku 4.5 | Messages API |
| **OpenAI** | GPT-4o, GPT-4o Mini, o4-mini | Chat Completions |
| **Google** | Gemini 2.5 Flash, Gemini 2.5 Pro | GenerateContent |
| **Groq** | Llama 3.3 70B | OpenAI-compatible |
| **Ollama** | Any local model | OpenAI-compatible |
| **OpenRouter** | Any routed model | OpenAI-compatible |
| **xAI** | Grok models | OpenAI-compatible |

Switch between any provider mid-session with Ctrl+O. API keys resolve automatically from Signet's secret store.

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                     FORGE (Rust)                     │
│                                                       │
│  ┌── forge-tui ────────────────────────────────────┐ │
│  │ Chat View │ Model Picker │ Dashboard │ Command   │ │
│  │ Palette │ Permissions Dialog │ Memory Panel      │ │
│  └──────────────────────────────────────────────────┘ │
│                                                       │
│  ┌── forge-agent ──────────────────────────────────┐ │
│  │ Agentic Loop (msg → LLM → tool calls → loop)    │ │
│  │ Context Window Management + Auto-Compact         │ │
│  │ Permission System │ Session State                │ │
│  └──────────────────────────────────────────────────┘ │
│                                                       │
│  ┌── forge-provider ───┐  ┌── forge-tools ─────────┐ │
│  │ Anthropic (Claude)  │  │ Bash │ Read │ Write     │ │
│  │ OpenAI (GPT/o-*)    │  │ Edit │ Glob │ Grep     │ │
│  │ Google (Gemini)     │  │ Agent (sub-task)        │ │
│  │ Groq │ Ollama       │  │ WebSearch │ WebFetch    │ │
│  │ OpenRouter │ xAI    │  └─────────────────────────┘ │
│  └─────────────────────┘                              │
│                                                       │
│  ┌── forge-signet ─────┐  ┌── forge-mcp ──────────┐ │
│  │ Daemon HTTP Client  │  │ MCP Client (stdio+SSE) │ │
│  │ Memory Search/Store │  │ Tool Discovery          │ │
│  │ Session Hooks       │  │ Signet MCP Tools        │ │
│  │ Config Watcher      │  │ Marketplace Servers     │ │
│  │ Secret Resolution   │  └─────────────────────────┘ │
│  │ Skill Loader        │                              │
│  │ Dashboard Data      │                              │
│  └─────────────────────┘                              │
│                                                       │
└───────────┬─────────────────────┬─────────────────────┘
            │                     │
            ▼                     ▼
    ┌───────────────┐     ┌───────────────┐
    │ AI Providers  │     │ Signet Daemon │
    │ (Anthropic,   │     │ localhost:3850│
    │  OpenAI,      │     │ Memory, Graph,│
    │  Gemini...)   │     │ Pipeline,     │
    └───────────────┘     │ Identity      │
                          └───────────────┘
```

## Usage

```bash
# Default: Anthropic Claude, connects to Signet daemon
forge

# Specify model and provider
forge --model claude-opus-4-6 --provider anthropic

# Run without Signet daemon (standalone mode)
forge --no-daemon

# Resume last session
forge --resume
```

### Key Bindings

| Key | Action |
|-----|--------|
| `Enter` | Send message |
| `Ctrl+O` | Model picker |
| `Ctrl+K` | Command palette |
| `Ctrl+S` | Memory search |
| `F2` | Dashboard overlay |
| `Ctrl+C` | Cancel generation |
| `Ctrl+D` | Quit |
| `Ctrl+L` | Clear screen |
| `PageUp/Down` | Scroll chat |

## Requirements

- Rust toolchain (1.75+)
- Signet daemon running on localhost:3850 (optional but recommended)
- API key for at least one provider (resolved via Signet secrets or environment variables)

## Building

```bash
cargo build --release
```

The binary is at `target/release/forge`.

## License

MIT
