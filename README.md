# Forge

**Signet's native AI terminal.** One binary, any model, real memory.

Forge is a terminal-native agentic AI client built in Rust. It connects directly to the [Signet](https://github.com/Signet-AI/signetai) daemon over localhost HTTP for memory, identity, secrets, and extraction — eliminating the dual-memory problem that exists when Signet runs as a plugin inside other AI harnesses.

17,000+ lines of Rust across an 8-crate workspace. Zero JavaScript. Sub-5ms memory recall. 12 built-in tools. 4 themes. Cross-platform.

---

## Why Forge Exists

When Signet runs inside Claude Code, Cursor, or any other host, **two memory systems compete**: Signet's vector-indexed database and the host's built-in file-based memory. They don't sync, they duplicate facts, and the agent has no clear source of truth.

Forge solves this by being the harness. There's no host memory system to fight because Forge *is* the host, and it routes everything through Signet.

| | Connector model | Forge |
|---|---|---|
| Memory systems | Two, no sync | One |
| Recall latency | ~200ms (shell hooks + MCP) | ~5ms (localhost HTTP) |
| Storage path | Depends which tool fires first | Always Signet |
| Host auto-memory | Can't disable | Doesn't exist |
| Search | Static file index | Vector + keyword hybrid |
| Identity | Injected via hooks | Native — loaded at startup |
| Agent name | Generic "Assistant" | From IDENTITY.md (e.g. [Boogy]) |

> See [docs/MEMORY_ARCHITECTURE.md](docs/MEMORY_ARCHITECTURE.md) for the full technical breakdown.

---

## The Pipeline

How a conversation flows from typing to long-term memory.

```
YOU
 │
 │  Type a message (type-ahead works while model is thinking)
 ▼
┌─────────────────────────────────────────────────────────────┐
│                     FORGE TERMINAL (TUI)                    │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐    │
│  │ [Forge] model (provider) ● 19 recalled / 1672 mem  │    │
│  │ [^O Model] [^K Cmd] [^D Dashboard] [^B Keybinds].. │    │
│  ├─────────────────────────────────────────────────────┤    │
│  │                                                     │    │
│  │  [Boogy]                                            │    │
│  │  response with markdown, code blocks, tool output   │    │
│  │                                                     │    │
│  │  ◈ Deliberating...                                  │    │
│  │  ✓ [Edit] src/main.rs                               │    │
│  │  ⟳ [Bash] cargo test                                │    │
│  │                                                     │    │
│  │  > your message                                     │    │
│  ├─────────────────────────────────────────────────────┤    │
│  │ Input (expands, wraps, scrolls to cursor)           │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                             │
│  forge-agent loop:                                          │
│    prompt → recall → inject memories → LLM                  │
│    → tool calls → execute → results → loop                  │
│                                                             │
│  forge-provider (PTY CLI streaming or direct API)           │
│  forge-tools (12 built-in: core + web + signet)             │
│  forge-signet (memory, identity, secrets, hooks)             │
└─────────────────────┬───────────────────────────────────────┘
                      │ HTTP (localhost:3850)
                      ▼
┌─────────────────────────────────────────────────────────────┐
│                     SIGNET DAEMON                            │
│                                                             │
│  SESSION LIFECYCLE                                          │
│  ├─ session-start → claim runtime path, load identity       │
│  ├─ user-prompt-submit → inject recalled memories           │
│  ├─ pre-compaction → get summary instructions               │
│  └─ session-end → submit transcript for extraction          │
│                                                             │
│  MEMORY PIPELINE (async, post-session)                      │
│  ├─ Extraction → LLM decomposes transcript into facts       │
│  ├─ Decision → ADD/UPDATE/DELETE against existing memories   │
│  ├─ Entity Linking → knowledge graph nodes + relations       │
│  ├─ Embedding → vector indexing for semantic search          │
│  ├─ Hints → prospective queries for future recall            │
│  └─ Retention → decay, dedup, orphan cleanup                │
│                                                             │
│  RETRIEVAL (on every prompt)                                │
│  ├─ Graph traversal → focal entities → scored candidates    │
│  ├─ Hybrid search → BM25 keyword + vector similarity        │
│  ├─ Constraint surfacing → always-inject entity constraints  │
│  └─ Reranking → final scored results injected into context  │
│                                                             │
│  STORAGE                                                    │
│  ├─ SQLite + FTS5 + Vector index + Knowledge graph          │
│  └─ Entities, aspects, attributes, dependencies, relations  │
└─────────────────────────────────────────────────────────────┘
```

### Memory flow

1. **While you type** — speculative pre-recall fires after 500ms, warming the cache
2. **On submit** — graph traversal finds focal entities, hybrid search fills remaining slots, constraints always surface
3. **During session** — `/recall` and `/remember` for manual search/storage, path feedback tracks usefulness
4. **On quit** — transcript submitted for extraction, fact decomposition, entity linking, embedding

### Identity

`SOUL.md`, `IDENTITY.md`, `USER.md`, and `AGENTS.md` load from `~/.agents/` at startup. The agent's name (from `**name:**` in IDENTITY.md) displays as `[Boogy]` before every response.

### CLI Provider Pipeline

CLI providers spawn via **real PTY** (portable-pty, cross-platform) with 64KB read buffer. A watcher thread monitors child exit and drops the PTY master to signal EOF.

```
Forge TUI → PTY spawn → 64KB chunk reads → ANSI strip → JSON parse
  ├─ content_block_start (tool_use) → ToolStart card
  ├─ content_block_delta (text_delta) → streaming text
  ├─ tool_result → ToolResult card with output
  └─ /forge-bypass → --dangerously-skip-permissions (Claude)
                     --dangerously-bypass-approvals-and-sandbox (Codex)
```

---

## Install

### From release (recommended)

```bash
curl -L https://github.com/aaf2tbz/signet-forge/releases/latest/download/forge-macos-arm64.tar.gz | tar xz
mv forge ~/.cargo/bin/
```

### From source

```bash
git clone https://github.com/aaf2tbz/signet-forge.git
cd signet-forge
cargo install --path crates/forge-cli
```

<details>
<summary>Linux dependencies</summary>

**Ubuntu/Debian:**
```bash
sudo apt install -y build-essential pkg-config libssl-dev libxcb-shape0-dev \
  libxcb-xfixes0-dev libxkbcommon-dev
```

**Fedora/RHEL:**
```bash
sudo dnf install -y gcc openssl-devel libxcb-devel libxkbcommon-devel
```

</details>

On first run, Forge checks for Signet, offers to install it, runs setup, starts the daemon, discovers your providers, and drops you into a conversation.

---

## Providers

| Provider | How It Works |
|---|---|
| **Claude Code CLI** | PTY-spawned `claude` binary — no API key needed |
| **Codex CLI** | PTY-spawned `codex` binary |
| **Gemini CLI** | PTY-spawned `gemini` binary |
| **Anthropic API** | Direct Messages API with streaming + prompt caching |
| **OpenAI API** | Chat Completions (GPT-4o, o4-mini) |
| **Google API** | GenerateContent (Gemini 2.5 Flash/Pro, 1M context) |
| **Groq / OpenRouter / xAI** | OpenAI-compatible APIs |
| **Ollama** | Any local model, no key needed |

Model picker (Ctrl+O) always shows API + CLI models. Daemon registry models merge in when available. Your choice persists across sessions.

---

## Usage

```bash
forge                                    # Interactive — auto-detects everything
forge --auth                             # Auth wizard (browser login + API key paste)
forge --auth --auth-provider openai      # Auth one provider directly
forge --auth --auth-provider claude-cli  # Claude CLI auth (paste token or browser login)
forge --auth-only                        # Configure auth, then exit
forge --provider claude-cli              # Use Claude Code CLI
forge --provider ollama --model qwen3:4b # Local model
forge --model claude-opus-4-6            # Specific model (infers provider)
forge --signet-token <token>             # Signet daemon auth for team/hybrid mode
forge --signet-actor my-agent            # Override x-signet-actor header
forge -p "explain this error" < err.log  # Non-interactive, streams to stdout
forge --resume                           # Continue last session
forge --theme midnight                   # Set theme
forge --no-daemon                        # Standalone, no Signet
```

### Auth setup flow

- **Provider multi-select UI** in terminal: `↑/↓` move, `Space` toggle, `Enter` continue
- **API providers**: opens key page + paste API key
- **CLI providers** (`claude-cli`, `codex-cli`, `gemini-cli`): choose between
  1. **Paste auth/API token** (saved locally and injected into CLI env)
  2. **Run auth login flow** (browser/device login)

Forge stores local auth values in your platform config dir:
- macOS: `~/Library/Application Support/forge/credentials.json`
- Linux: `~/.config/forge/credentials.json`

### Signet daemon auth

Forge now matches Signet's daemon auth contract:

- Sends `Authorization: Bearer <token>` when `--signet-token` is set
- Sends `x-signet-actor` and `x-signet-actor-type: agent` on daemon requests
- Reads token/actor from env as well: `FORGE_SIGNET_TOKEN`, `SIGNET_AUTH_TOKEN`, `SIGNET_TOKEN`, `FORGE_SIGNET_ACTOR`, `SIGNET_ACTOR`

This matters when Signet runs in authenticated `team` mode, or in `hybrid`
mode behind a non-loopback/proxied setup.

---

## Key Bindings

All rebindable via `Ctrl+B` (editor overlay) or `~/.config/forge/keybinds.json`. Header updates live.

| Key | Action |
|-----|--------|
| `Enter` | Send message |
| `Ctrl+O` | Model picker |
| `Ctrl+K` | Command palette |
| `Ctrl+G` | Signet command picker |
| `Ctrl+D` | Dashboard navigator |
| `Ctrl+H` | Session browser |
| `Ctrl+B` | Keybind editor |
| `F2` | Dashboard panel (Memory/Pipeline/Embeddings/Health) |
| `Ctrl+V` | Paste |
| `Ctrl+C` | Cancel generation |
| `Ctrl+Q` | Quit |
| `Ctrl+L` | Clear chat |
| `Tab` | Autocomplete slash command |
| `PageUp/Down` | Scroll |

---

## Commands

Type `/` and press `Tab` to complete. Arguments for `/effort`, `/theme`, `/model` show predictive options.

| Command | What it does |
|---|---|
| `/recall <query>` | Search memories |
| `/remember <text>` | Store a memory |
| `/model` | Open model picker |
| `/auth` | Show provider auth setup instructions |
| `/effort <level>` | Reasoning effort (low/medium/high) — persists |
| `/theme <name>` | Switch theme — persists |
| `/forge-bypass` | Toggle CLI permission bypass |
| `/extraction-model` | View/change extraction pipeline model |
| `/keybinds` | Keybind editor |
| `/dashboard` | Dashboard page navigator |
| `/status` | Agent and daemon status |
| `/doctor` | Health checks |
| `/pipeline` | Extraction pipeline status |
| `/logs` | Daemon log tail |
| `/diagnostics` | Health score across all domains |
| `/embed-audit` | Embedding coverage audit |
| `/embed-backfill` | Backfill missing embeddings |
| `/repair-requeue` | Requeue dead jobs |
| `/repair-leases` | Release stale leases |
| `/repair-fts` | Repair FTS index |
| `/secret-list` | List secrets |
| `/skill-list` | List skills |
| `/clear` | Clear chat |
| `/compact` | Force context compaction |
| `/resume` | Resume last session |

---

## Tools (12 built-in)

### Core (6)
| Tool | Permission | Description |
|------|-----------|-------------|
| Bash | Write | Execute shell commands |
| Read | ReadOnly | Read file contents |
| Write | Write | Create/overwrite files |
| Edit | Write | Find-and-replace in files |
| Glob | ReadOnly | Find files by pattern |
| Grep | ReadOnly | Search file contents with regex |

### Web (2)
| Tool | Permission | Description |
|------|-----------|-------------|
| WebSearch | ReadOnly | DuckDuckGo search (no API key) |
| WebFetch | ReadOnly | Fetch URL, strip HTML to text |

### Signet (4, when daemon connected)
| Tool | Permission | Description |
|------|-----------|-------------|
| memory_search | ReadOnly | Hybrid vector + keyword recall |
| memory_store | Write | Save new memory to Signet |
| knowledge_expand | ReadOnly | Drill into knowledge graph entity |
| secret_exec | Write | Run command with secrets injected |

---

## Features

### Chat
- **Agent identity** — `[signet identity]` label from IDENTITY.md on every response
- **Markdown rendering** — headers, code blocks, bold/italic, blockquotes, lists
- **Contextual animated verbs** — Thinking, Deliberating, Hypothesizing, Riddling, Constructing, Squandering, Galloping, Fiddling (~4s cycle)
- **Content padding** — breathing room from header and input box
- **Auto-scroll** — hidden buffer render for exact wrapped height (no estimation drift)
- **Unicode-width** — proper emoji/CJK display width calculations

### Input
- **Type-ahead** — compose while model is thinking/streaming
- **Expanding box** — grows with content, wraps, scrolls to cursor, snaps back
- **Tab completion** — predictive for all commands and arguments
- **Paste/drag-drop** — text and image files

### Providers
- **PTY-based CLI streaming** — real pseudo-terminal, 64KB buffer, no buffering delays
- **CLI tool visibility** — tool cards render from CLI stream-json events
- **CLI auth choices** — token paste or interactive browser login per provider
- **Permission bypass** — `/forge-bypass` mid-session toggle

### Overlays
- **Model picker (Ctrl+O)** — API + CLI + daemon registry models
- **Command palette (Ctrl+K)** — fuzzy search commands + skills
- **Dashboard panel (F2)** — Memory, Pipeline, Embeddings, Health tabs
- **Session browser (Ctrl+H)** — resume any of last 20 sessions
- **Keybind editor (Ctrl+B)** — rebind any key, saves immediately
- **Dashboard navigator (Ctrl+D)** — open any Signet dashboard page

### Persistence
- **Settings** — model, provider, effort, theme, bypass saved to `~/.config/forge/settings.json`
- **Sessions** — SQLite auto-save, `--resume` or Ctrl+H to continue
- **Keybinds** — `~/.config/forge/keybinds.json` with live header updates

### Platform
- **Cross-platform** — macOS ARM64/x64, Linux x64
- **4 themes** — signet-dark, signet-light, midnight, amber
- **Terminal resize** — content reflows, hints truncate, scroll resets
- **Non-interactive** — `forge -p "query"` for scripting

---

## Architecture

8-crate Cargo workspace:

| Crate | Purpose |
|---|---|
| `forge-cli` | Entry point, arg parsing, Signet onboarding, settings persistence |
| `forge-tui` | TUI rendering, themes, overlays, keybind editor, chat scroll, agent name |
| `forge-agent` | Agentic loop, message threading, permission system, effort/bypass, MCP routing |
| `forge-provider` | LLM abstraction — API streaming + PTY CLI spawning with 64KB chunk reads |
| `forge-tools` | 12 tools: Bash, Read, Write, Edit, Glob, Grep, WebSearch, WebFetch, 4 Signet |
| `forge-signet` | Signet HTTP client, identity loader, agent name parser, config watcher, hooks |
| `forge-mcp` | MCP stdio client with JSON-RPC 2.0 handshake |
| `forge-core` | Shared types, errors, message format |

### Key dependencies

- `portable-pty` — cross-platform PTY for CLI providers
- `ratatui` — terminal UI framework
- `tokio` — async runtime
- `reqwest` — HTTP + SSE streaming
- `rusqlite` — session persistence
- `unicode-width` — accurate display width for scroll calculations
- `arboard` — clipboard access
- `pulldown-cmark` — markdown parsing

---

## License

MIT
