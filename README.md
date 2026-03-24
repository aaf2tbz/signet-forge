# Signet Forge

Signet's native AI terminal. A Rust TUI that gives Signet its own conversational interface — calling provider APIs and installed CLIs directly, executing tools natively, and integrating with the Signet daemon over localhost for memory, extraction, identity, and session management.

## What This Is

[Signet](https://github.com/Signet-AI/signetai) is an open-source AI agent memory and identity system. It's provider-agnostic — it runs extraction on local Ollama models (qwen3:4b), embeds with nomic-embed-text locally, and connects to any harness (Claude Code, OpenCode, Codex, OpenClaw) through connector adapters.

The problem isn't that Signet is locked to a provider. **The problem is that Signet doesn't have its own terminal.** Every conversation goes through someone else's tool — Claude Code, OpenCode, Codex — and Signet hooks in via shell subprocesses, parsing stdout, hoping the harness fires hooks at the right time. Each harness has its own config format, session model, tool system, and permission model. Connectors must be written and maintained for each one. When a harness changes its API, Signet breaks.

**Forge is Signet's own harness.** Instead of hooking into another tool, Signet drives the conversation directly.

## What Forge Provides

### First-Run Onboarding

When you run `forge` for the first time, it walks you through everything:

1. **Signet detection** — checks if Signet is installed, offers to install it if not
2. **Setup wizard** — runs `signet setup` to create your agent identity, configure providers, and initialize the memory database
3. **Daemon startup** — starts the Signet daemon automatically if it's not running
4. **Provider discovery** — scans daemon secrets, environment variables, and installed CLI tools (Claude Code, Codex, Gemini CLI) to find available providers
5. **Interactive selection** — if multiple providers are available, presents a picker to choose which one to use

No manual configuration required. If you have `claude` installed, Forge finds it and uses it.

### Provider Support

Forge supports two categories of providers — **API-based** and **CLI-based**. You don't need API keys if you have an authenticated CLI tool installed.

**API Providers** (direct API calls, requires key from Signet secrets or env var):

| Provider | Example Models | Notes |
|---|---|---|
| Anthropic | Claude Opus 4.6, Sonnet 4.6, Haiku 4.5 | Messages API with streaming |
| OpenAI | GPT-4o, o4-mini | Chat Completions |
| Google | Gemini 2.5 Flash, Gemini 2.5 Pro | GenerateContent (1M context) |
| Groq | Llama 3.3 70B | OpenAI-compatible, fast inference |
| Ollama | Any local model | Local, no API key needed |
| OpenRouter | Any routed model | Aggregator for 100+ models |
| xAI | Grok | OpenAI-compatible |

**CLI Providers** (uses installed CLI tools — they handle their own auth):

| CLI | Binary | Output Format |
|---|---|---|
| Claude Code | `claude` | `--output-format stream-json` |
| Codex | `codex` | `exec --json` JSONL |
| Gemini CLI | `gemini` | Plain text |

Switch providers mid-session with **Ctrl+O**. API keys resolve from Signet's encrypted secret store, environment variables, or not at all for CLI/Ollama providers.

### Signet Integration

Forge communicates with the Signet daemon over localhost HTTP (~5ms per call vs ~200ms for shell hooks).

| What | How |
|---|---|
| **Memory recall** | Per-prompt hybrid search (vector + keyword) via daemon API |
| **Memory storage** | `/remember <text>` stores directly to daemon |
| **Identity** | AGENTS.md, SOUL.md, IDENTITY.md, USER.md loaded at startup |
| **Extraction** | Session transcripts submitted on quit — daemon runs synthesis → extraction → embedding pipeline |
| **Secrets** | API keys resolved from daemon's encrypted secret store |
| **Config** | Watches `agent.yaml` in real-time via filesystem notifications |
| **Skills** | Loads slash commands from `~/.agents/skills/` |
| **Dashboard** | F2 opens the web dashboard at localhost:3850 |

### Built-in Signet Commands

Type `/signet-help` in the terminal or press **Ctrl+H** to open the interactive command picker.

**Status & Diagnostics:**
| Command | Description |
|---|---|
| `/status` | Agent and daemon status |
| `/doctor` | Health checks with fix suggestions |
| `/logs` | Last 50 daemon log lines |
| `/health` | Full daemon health report |
| `/diagnostics` | Composite health score across all domains |
| `/pipeline` | Extraction pipeline status |

**Memory:**
| Command | Description |
|---|---|
| `/recall <query>` | Search memories |
| `/remember <text>` | Store a new memory |
| `/recall-test` | Test memory search with a sample query |

**Management:**
| Command | Description |
|---|---|
| `/embed-audit` | Audit embedding coverage |
| `/embed-backfill` | Backfill missing embeddings |
| `/skill-list` | List installed skills |
| `/secret-list` | List configured secrets |
| `/sync` | Sync built-in templates and skills |

**Repair:**
| Command | Description |
|---|---|
| `/repair-requeue` | Requeue dead extraction jobs |
| `/repair-leases` | Release stale job leases |
| `/repair-fts` | Check and repair FTS search index |

**Daemon:**
| Command | Description |
|---|---|
| `/daemon-restart` | Restart the Signet daemon |
| `/daemon-stop` | Stop the Signet daemon |

### Four Models, One System

Signet uses four separate model configurations. Forge only controls the conversational model — the daemon manages the rest independently.

| Model | Purpose | Configured In | Default |
|---|---|---|---|
| **Conversational** | What the user talks to | Forge / Ctrl+O | claude-sonnet-4-6 |
| **Synthesis** | Summarizes transcripts into facts | `agent.yaml` → `pipelineV2.synthesis` | claude-code/haiku |
| **Extraction** | Deeper fact/entity analysis | `agent.yaml` → `pipelineV2.extraction` | qwen3:4b (Ollama) |
| **Embedding** | Vector embeddings for search | `agent.yaml` → `embedding` | nomic-embed-text (Ollama) |

Switching the conversational model does NOT touch extraction or embedding. Those run on local Ollama models for zero-cost, low-latency processing.

### Agentic Loop

Forge runs a full agentic coding loop:

```
1. User types prompt
2. Forge calls daemon → hybrid memory recall → inject relevant memories
3. Build context: identity + memories + conversation history + user message
4. Send to provider → stream response to TUI
5. If tool calls: check permissions → execute → append result → loop to step 4
6. If text only: turn complete → update status bar
7. On quit: submit transcript → daemon runs extraction pipeline
```

### Tools

Six built-in tools with a permission system:

| Tool | Permission | Description |
|---|---|---|
| **Bash** | Write (approval required) | Execute shell commands |
| **Read** | Read (auto-approved) | Read files with line numbers |
| **Write** | Write (approval required) | Create files |
| **Edit** | Write (approval required) | String replacement editing |
| **Glob** | Read (auto-approved) | File pattern matching |
| **Grep** | Read (auto-approved) | Content search with regex |

### TUI Features

- **Markdown rendering** — headers, code blocks, lists, bold/italic, blockquotes with syntax highlighting
- **Animated status indicators** — `◇ Recalling memories...` → `◆ Thinking...` → `◈ Running [Tool]...` with Signet-themed geometric spinners
- **Context compaction** — auto-summarizes at 90% context window capacity
- **Session persistence** — SQLite auto-save, `--resume` to continue where you left off
- **Status bar** — model, provider, token usage, memory count (`N recalled / M total`), daemon health
- **Four themes** — `signet-dark` (default), `signet-light`, `midnight`, `amber`

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                     FORGE (Rust)                     │
│                                                       │
│  ┌── forge-tui ────────────────────────────────────┐ │
│  │ Chat │ Model Picker │ Command Palette │ Perms   │ │
│  │ Signet Commands │ Status Bar │ Themes            │ │
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
│  │ xAI │ CLI Providers │                              │
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
    │ (API/CLI/     │     │ localhost:3850│
    │  local)       │     │               │
    └───────────────┘     └───────────────┘
```

**8 crates:** forge-core (types), forge-provider (7 API + 3 CLI providers), forge-tools (6 tools), forge-mcp (MCP client), forge-signet (daemon integration), forge-agent (agentic loop), forge-tui (terminal UI), forge-cli (entry point + onboarding).

## Usage

```bash
# First run — walks through Signet install + setup if needed
forge

# Specify model and provider
forge --model claude-opus-4-6 --provider anthropic

# Use an installed CLI tool (no API key needed)
forge --provider claude-cli

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
| `Ctrl+G` | Signet command picker — diagnostics, repair, management |
| `Ctrl+D` | Open Signet dashboard in browser |
| `Ctrl+C` | Cancel current generation |
| `Ctrl+Q` | Quit (auto-saves session, submits transcript for extraction) |
| `Ctrl+L` | Clear chat |
| `PageUp/Down` | Scroll history |
| `Y/A/N` | Permission dialog (Allow / Always Allow / Deny) |

### Slash Command Autocomplete

Type `/` in the input to see a filtered list of all available commands as greyed-out suggestions. Keep typing to narrow results — e.g., `/rep` shows only repair commands. Built-in commands include `/help`, `/clear`, `/model`, `/dashboard`, `/resume`, `/recall <query>`, `/remember <text>`, plus all Signet diagnostic and management commands.

### CLI Model Switching

When using a CLI provider (e.g., `claude-cli`), the model picker (Ctrl+O) shows CLI-compatible models first — selecting one stays on the CLI and passes `--model` to the binary. API models are listed below if you want to switch away from the CLI entirely.

## Why Forge Instead of Connectors

| With Connectors | With Forge |
|---|---|
| Shell subprocess hooks (~200ms per call) | Direct HTTP to daemon (~5ms) |
| One connector per harness to maintain | Zero connectors needed |
| Config split across settings.json, agent.yaml, .opencode.json | One config: agent.yaml |
| Identity injected via hook stdout parsing | Identity files read directly at startup |
| Extraction triggered by session-end hook (fragile) | Native session lifecycle with reliable transcript submission |
| Skills routed through harness slash command system | Skills loaded and executed natively |
| MCP proxied through harness plugin system | MCP client built-in |
| API keys require separate .env per tool | Keys from Signet's encrypted secret store |
| Only API providers supported | API providers + installed CLIs + local models |
| No access to daemon diagnostics | Full troubleshooter via /commands and Ctrl+H |

## Requirements

- Rust 1.75+ (for building from source)
- Signet (optional — Forge offers to install it on first run)
- At least one of: an API key, an installed CLI tool (`claude`, `codex`, `gemini`), or Ollama

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
