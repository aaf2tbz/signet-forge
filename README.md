# Signet Forge

Signet's native AI terminal. A Rust TUI that gives Signet its own conversational interface — calling provider APIs directly, executing tools natively, and integrating with the Signet daemon over localhost for memory, extraction, identity, and session management.

## What This Is

Signet is an open-source AI agent memory and identity system. It's provider-agnostic — it already runs extraction on local Ollama models (qwen3:4b), embeds with nomic-embed-text locally, and connects to any harness (Claude Code, OpenCode, Codex, OpenClaw) through connector adapters.

The problem isn't that Signet is locked to a provider. **The problem is that Signet doesn't have its own terminal.** Every conversation goes through someone else's tool — Claude Code, OpenCode, Codex — and Signet hooks in via shell subprocesses, parsing stdout, hoping the harness fires hooks at the right time. Each harness has its own config format, session model, tool system, and permission model. Connectors must be written and maintained for each one.

**Forge is Signet's own harness.** Instead of hooking into another tool, Signet drives the conversation directly.

## What Changes

| With Connectors | With Forge |
|---|---|
| Shell subprocess hooks (~200ms per call) | Direct HTTP to daemon (~5ms) |
| One connector per harness to maintain | Zero connectors needed |
| Config split across settings.json, agent.yaml, .opencode.json | One config: agent.yaml |
| Identity injected via hook stdout parsing | Identity files read directly at startup |
| Extraction triggered by session-end hook (fragile) | Native session lifecycle with reliable transcript submission |
| Skills routed through harness slash command system | Skills loaded and executed natively |
| MCP proxied through harness plugin system | MCP client built-in |
| Dashboard requires a browser on :3850 | Dashboard data in TUI panels |

### Why This Matters for Signet

**Memory integration drops to ~5ms.** Forge calls the daemon's HTTP API directly from native Rust instead of spawning subprocesses that parse stdout and relay data through environment variables.

**Session lifecycle is reliable.** The session-end hook — currently the most fragile part of the pipeline because it depends on the host harness firing correctly — becomes a direct HTTP POST that Forge controls entirely.

**Identity loads without intermediaries.** AGENTS.md, SOUL.md, IDENTITY.md, USER.md are read directly from `~/.agents/` at startup. No character limits, no stdout buffer truncation, no hook output parsing.

**API keys come from Signet's secret store.** Forge resolves keys through the daemon's secret API. No separate `.env` files per tool.

**Config watches in real-time.** Forge monitors `~/.agents/agent.yaml` via filesystem notifications. Change the extraction model from the dashboard and Forge picks it up immediately.

**Connector maintenance drops to zero.** No more `connector-claude-code`, `connector-opencode`, `connector-codex`, `connector-openclaw`. Each connector breaks when the upstream tool changes its API. Forge eliminates all of them.

## Four Models, One System

Signet uses four separate model configurations. Forge only controls the first one — the daemon manages the rest independently.

| Model | Purpose | Configured In | Default |
|---|---|---|---|
| **Conversational** | What the user talks to | Forge CLI / Ctrl+O | claude-sonnet-4-6 |
| **Synthesis** | Summarizes transcripts into facts | `agent.yaml` → `pipelineV2.synthesis` | claude-code/haiku |
| **Extraction** | Deeper fact/entity analysis | `agent.yaml` → `pipelineV2.extraction` | qwen3:4b (Ollama) |
| **Embedding** | Vector embeddings for memory search | `agent.yaml` → `embedding` | nomic-embed-text (Ollama) |

Switching the conversational model does NOT touch extraction or embedding. Those typically run on local Ollama models for zero-cost, low-latency processing. The conversational model can be any cloud or local provider.

When Forge sends a transcript to the daemon on session end, the daemon asynchronously:
1. Runs the **synthesis model** to extract session facts
2. Runs the **extraction model** for deeper entity/relationship analysis
3. Computes **embeddings** for vector search indexing

Forge never calls these models directly.

## Providers

Forge calls provider APIs directly — no intermediary harness. Switch mid-session with Ctrl+O.

| Provider | Example Models | Notes |
|---|---|---|
| Anthropic | Claude Opus 4.6, Sonnet 4.6, Haiku 4.5 | Messages API with streaming |
| OpenAI | GPT-4o, o4-mini | Chat Completions |
| Google | Gemini 2.5 Flash, Gemini 2.5 Pro | GenerateContent (1M context) |
| Groq | Llama 3.3 70B | OpenAI-compatible, fast inference |
| Ollama | Any local model | OpenAI-compatible, no API key |
| OpenRouter | Any routed model | Aggregator for 100+ models |
| xAI | Grok | OpenAI-compatible |

API keys resolve from Signet's secret store or environment variables. Ollama needs no key.

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                     FORGE (Rust)                     │
│                                                       │
│  ┌── forge-tui ────────────────────────────────────┐ │
│  │ Chat │ Model Picker │ Command Palette │ Perms   │ │
│  │ Markdown Rendering │ Status Bar │ Themes        │ │
│  └──────────────────────────────────────────────────┘ │
│                                                       │
│  ┌── forge-agent ──────────────────────────────────┐ │
│  │ Agentic Loop (msg → LLM → tool calls → loop)    │ │
│  │ Context Compaction │ Permission System           │ │
│  │ Session Persistence (SQLite)                     │ │
│  └──────────────────────────────────────────────────┘ │
│                                                       │
│  ┌── forge-provider ───┐  ┌── forge-tools ─────────┐ │
│  │ Anthropic │ OpenAI  │  │ Bash │ Read │ Write     │ │
│  │ Gemini │ Groq       │  │ Edit │ Glob │ Grep      │ │
│  │ Ollama │ OpenRouter  │  └─────────────────────────┘ │
│  │ xAI                 │                              │
│  └─────────────────────┘                              │
│                                                       │
│  ┌── forge-signet ─────┐  ┌── forge-mcp ──────────┐ │
│  │ Daemon Client       │  │ MCP Client (stdio)     │ │
│  │ Session Hooks       │  │ Tool Discovery         │ │
│  │ Memory Recall/Store │  └─────────────────────────┘ │
│  │ Secret Resolution   │                              │
│  │ Config Watcher      │                              │
│  │ Skill Loader        │                              │
│  └─────────────────────┘                              │
│                                                       │
└───────────┬─────────────────────┬─────────────────────┘
            │                     │
            ▼                     ▼
    ┌───────────────┐     ┌───────────────┐
    │ AI Providers  │     │ Signet Daemon │
    │ (cloud/local) │     │ localhost:3850│
    └───────────────┘     └───────────────┘
```

**8 crates:** forge-core (types), forge-provider (7 providers), forge-tools (6 tools), forge-mcp (MCP client), forge-signet (daemon integration), forge-agent (agentic loop), forge-tui (terminal UI), forge-cli (entry point).

## Usage

```bash
# Default: Claude Sonnet, connects to Signet daemon
forge

# Specify model and provider
forge --model claude-opus-4-6 --provider anthropic

# Use a local model
forge --model qwen3:4b --provider ollama

# Non-interactive: single prompt, streams response to stdout
forge -p "explain this error" < error.log

# Run without Signet daemon (standalone, no memory)
forge --no-daemon

# Resume last session
forge --resume

# Choose a theme
forge --theme midnight
```

### Key Bindings

| Key | Action |
|-----|--------|
| `Enter` | Send message |
| `Ctrl+O` | Model picker — switch provider/model mid-session |
| `Ctrl+K` | Command palette — search commands and skills |
| `Ctrl+C` | Cancel current generation |
| `Ctrl+D` | Quit (auto-saves session, submits transcript) |
| `Ctrl+L` | Clear chat |
| `PageUp/Down` | Scroll history |
| `Y/A/N` | Permission dialog (Allow / Always Allow / Deny) |

### Themes

Four built-in themes: `signet-dark` (default), `signet-light`, `midnight`, `amber`.

## Features

- **Agentic loop** — prompt → LLM → tool calls → execute → loop until done
- **6 built-in tools** — Bash, Read, Write, Edit, Glob, Grep
- **Permission system** — auto-approve read-only tools, dialog for write operations
- **Markdown rendering** — headers, code blocks, lists, bold/italic, blockquotes
- **Context compaction** — auto-summarizes at 90% context window capacity
- **Session persistence** — SQLite auto-save, `--resume` to continue
- **Config watching** — real-time response to agent.yaml changes
- **MCP client** — stdio transport with JSON-RPC for external tool servers
- **Skills** — loads slash commands from `~/.agents/skills/`
- **Signet hooks** — session-start (memory injection), prompt-submit (per-prompt recall), session-end (transcript extraction)

## Requirements

- Rust 1.75+
- Signet daemon on localhost:3850 (optional — Forge works standalone without memory)
- API key for at least one provider (from Signet secrets or env vars)

## Building

```bash
cargo build --release
# Binary at target/release/forge
```

Tag a release to trigger CI builds for macOS (ARM64/x64) and Linux (x64):

```bash
git tag v0.1.0
git push origin v0.1.0
```

## License

MIT
