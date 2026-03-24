# Forge

**Signet's native AI terminal.** One binary, any model, real memory.

Forge is a terminal-native agentic AI client built in Rust. It connects directly to the [Signet](https://github.com/Signet-AI/signetai) daemon over localhost HTTP for memory, identity, secrets, and extraction — eliminating the dual-memory problem that exists when Signet runs as a plugin inside other AI harnesses.

17,000+ lines of Rust across an 8-crate workspace. Zero JavaScript. Sub-5ms memory recall.

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

> See [docs/MEMORY_ARCHITECTURE.md](docs/MEMORY_ARCHITECTURE.md) for the full technical breakdown.

---

## The Pipeline

This is how a conversation flows through Forge and Signet — from the moment you type to long-term memory persistence.

```
YOU
 │
 │  Type a message
 ▼
┌─────────────────────────────────────────────────────────────┐
│                     FORGE TERMINAL (TUI)                    │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐    │
│  │ Status Bar                                          │    │
│  │ [Model] [Cmd] [Dashboard] [Signet] [Keybinds] ...  │    │
│  ├─────────────────────────────────────────────────────┤    │
│  │ Chat View                                           │    │
│  │  > your message                                     │    │
│  │                                                     │    │
│  │  ◈ Thinking...                                      │    │
│  │  ◆ Writing...                                       │    │
│  │  ✓ [Edit] path/to/file.rs                           │    │
│  │  ⟳ [Bash] cargo test                                │    │
│  │                                                     │    │
│  │  Response with markdown, code blocks, tool output   │    │
│  ├─────────────────────────────────────────────────────┤    │
│  │ Input (expands with text, wraps, scrolls)           │    │
│  └─────────────────────────────────────────────────────┘    │
│                                                             │
│  forge-agent loop:                                          │
│    prompt → recall → inject memories → LLM                  │
│    → tool calls → execute → results → loop                  │
│                                                             │
│  forge-provider (PTY-based CLI streaming or direct API)     │
│  forge-tools (Bash, Read, Write, Edit, Glob, Grep)          │
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
│  ├─ SQLite (memories, entities, relations, jobs, sessions)  │
│  ├─ FTS5 (full-text search index)                           │
│  ├─ Vector index (embeddings for semantic similarity)       │
│  └─ Knowledge graph (entities, aspects, attributes, deps)   │
└─────────────────────────────────────────────────────────────┘
```

### Memory flow in detail

1. **While you type** — speculative pre-recall fires after 500ms, warming the cache with relevant memories before you hit Enter
2. **On submit** — targeted recall against your actual prompt. Graph traversal finds focal entities and walks the knowledge graph. Hybrid search (BM25 + vector) fills remaining slots. Constraints always surface. Results injected into LLM context.
3. **During session** — `/recall` and `/remember` for manual search and storage. Path feedback tracks which retrieved memories were actually useful.
4. **On quit** — full transcript submitted to Signet's extraction pipeline: LLM fact decomposition, decision stage (add/update/delete), entity linking, embedding, prospective hint generation, and retention decay.

### Identity

`SOUL.md`, `IDENTITY.md`, `USER.md`, and `AGENTS.md` load from `~/.agents/` at startup into the system prompt. This is structural identity, not memory recall — it defines who the agent is before any conversation begins.

### CLI Provider Pipeline

When using CLI providers (Claude Code, Codex, Gemini), Forge spawns the CLI in a **real PTY** for line-buffered streaming output. The CLI handles its own tool execution internally — Forge parses the stream-json events and renders tool cards, results, and text deltas in real-time.

```
Forge TUI
 │
 ├─ Spawns CLI in PTY (portable-pty, cross-platform)
 ├─ 64KB read buffer (handles burst output without backpressure)
 ├─ Watcher thread monitors child exit → drops PTY master → EOF
 │
 ├─ Parses stream-json NDJSON events:
 │   ├─ content_block_start (tool_use) → ToolStart card
 │   ├─ content_block_delta (text_delta) → streaming text
 │   ├─ content_block_delta (input_json_delta) → tool args
 │   ├─ tool_result → ToolResult card with output
 │   └─ result → final response
 │
 └─ /forge-bypass toggles --dangerously-skip-permissions (Claude)
                          --dangerously-bypass-approvals-and-sandbox (Codex)
```

---

## Install

### From release (recommended)

Tagged releases build automatically for macOS (ARM64, x64) and Linux (x64):

```bash
curl -L https://github.com/aaf2tbz/signet-forge/releases/latest/download/forge-macos-arm64.tar.gz | tar xz
mv forge ~/.cargo/bin/     # or /usr/local/bin/
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

**Arch:**
```bash
sudo pacman -S base-devel openssl libxcb libxkbcommon
```

</details>

### Verify

```bash
forge
```

On first run, Forge checks for Signet, offers to install it, runs setup, starts the daemon, discovers your providers, and drops you into a conversation. No config files to write.

---

## Providers

Use API keys, installed CLI tools, or local models. Forge auto-detects what you have.

| Provider | How It Works |
|---|---|
| **Claude Code CLI** | Uses your installed `claude` binary via PTY — no API key needed |
| **Codex CLI** | Uses your installed `codex` binary via PTY |
| **Gemini CLI** | Uses your installed `gemini` binary via PTY |
| **Anthropic API** | Direct Messages API with streaming + prompt caching |
| **OpenAI API** | Chat Completions (GPT-4o, o4-mini) |
| **Google API** | GenerateContent (Gemini 2.5 Flash/Pro, 1M context) |
| **Groq / OpenRouter / xAI** | OpenAI-compatible APIs |
| **Ollama** | Any local model, no key needed |

Switch mid-session with `Ctrl+O`. The model picker always shows both API and CLI models regardless of your current provider. Your choice persists across sessions.

---

## Usage

```bash
forge                                    # Interactive — auto-detects everything
forge --provider claude-cli              # Use Claude Code CLI
forge --provider ollama --model qwen3:4b # Local model
forge --model claude-opus-4-6            # Specific model (infers provider)
forge -p "explain this error" < err.log  # Non-interactive, streams to stdout
forge --resume                           # Continue last session
forge --theme midnight                   # Set theme
forge --no-daemon                        # Standalone, no Signet
```

---

## Key Bindings

All rebindable via `Ctrl+B` (keybind editor overlay) or `~/.config/forge/keybinds.json`. The header bar reflects your custom bindings in real-time.

| Key | Action |
|-----|--------|
| `Enter` | Send message |
| `Ctrl+O` | Model picker |
| `Ctrl+K` | Command palette |
| `Ctrl+G` | Signet command picker |
| `Ctrl+D` | Dashboard navigator |
| `Ctrl+B` | Keybind editor |
| `Ctrl+V` | Paste (text or image) |
| `Ctrl+C` | Cancel generation |
| `Ctrl+Q` | Quit |
| `Ctrl+L` | Clear chat |
| `Tab` | Autocomplete slash command |
| `PageUp/Down` | Scroll |

---

## Commands

Type `/` to see autocomplete suggestions. Press `Tab` to complete. Arguments for `/effort`, `/theme`, and `/model` show predictive options as you type.

| Command | What it does |
|---|---|
| `/recall <query>` | Search memories |
| `/remember <text>` | Store a memory |
| `/model` | Open model picker |
| `/effort <level>` | Reasoning effort (low/medium/high) — persists |
| `/theme <name>` | Switch theme — persists |
| `/forge-bypass` | Toggle CLI permission bypass (skip all approval prompts) |
| `/keybinds` | Interactive keybind editor |
| `/dashboard` | Dashboard page navigator |
| `/status` | Agent and daemon status |
| `/doctor` | Health checks with suggested fixes |
| `/pipeline` | Extraction pipeline status |
| `/logs` | Last 50 daemon log lines |
| `/diagnostics` | Health score across all domains |
| `/embed-audit` | Audit embedding coverage |
| `/embed-backfill` | Backfill missing embeddings |
| `/repair-requeue` | Requeue dead extraction jobs |
| `/repair-leases` | Release stale job leases |
| `/repair-fts` | Repair FTS search index |
| `/secret-list` | List configured secrets |
| `/skill-list` | List installed skills |
| `/clear` | Clear chat |
| `/compact` | Force context compaction |
| `/resume` | Resume last session |

---

## Features

- **Agentic loop** — prompt, recall, LLM, tool calls, execute, loop
- **6 built-in tools** — Bash, Read, Write, Edit, Glob, Grep with permission system
- **PTY-based CLI streaming** — real-time output from Claude/Codex/Gemini CLIs via pseudo-terminal, 64KB buffer, no buffering delays
- **CLI tool visibility** — tool use, results, and code changes from CLI providers render as cards in the chat
- **Speculative pre-recall** — starts searching memories while you type (500ms debounce)
- **4 themes** — signet-dark, signet-light, midnight, amber with full theme propagation
- **Animated status** — contextual cycling verbs: *Thinking, Deliberating, Hypothesizing, Riddling...* (~4s per verb)
- **Persistent settings** — model, provider, effort, theme, bypass saved to `~/.config/forge/settings.json`
- **Permission bypass** — `/forge-bypass` toggles `--dangerously-skip-permissions` (Claude) or `--yolo` (Codex) mid-session
- **Type-ahead input** — compose your next message while the model is thinking or streaming
- **Expanding input box** — grows with content, wraps text, scrolls to cursor, snaps back on send
- **Prompt caching** — Anthropic system prompt cached server-side for faster TTFT
- **Context compaction** — auto-summarizes at 90% capacity
- **Session persistence** — SQLite auto-save, `--resume` to continue
- **Image support** — drag images into terminal or Ctrl+V to paste
- **Markdown rendering** — headers, code blocks with language labels, bold/italic, blockquotes, lists
- **Tab autocomplete** — predictive completion for all slash commands and arguments
- **Dashboard navigator** — Ctrl+D opens page picker for every Signet dashboard tab
- **Interactive keybind editor** — Ctrl+B opens overlay, rebind any key, saves immediately, header updates live
- **MCP client** — stdio transport with JSON-RPC for external tool servers
- **Non-interactive mode** — `forge -p "query"` for scripting and pipes

---

## Architecture

8-crate Cargo workspace:

| Crate | Purpose |
|---|---|
| `forge-cli` | Entry point, arg parsing, Signet onboarding, settings persistence |
| `forge-tui` | TUI rendering, themes, overlays, keybind editor, chat scroll |
| `forge-agent` | Agentic loop, message threading, permission system, effort/bypass |
| `forge-provider` | LLM abstraction — API streaming (Anthropic, OpenAI, Google, Groq, Ollama) + PTY CLI spawning (Claude, Codex, Gemini) with 64KB chunk reads and ANSI stripping |
| `forge-tools` | Bash, Read, Write, Edit, Glob, Grep implementations |
| `forge-signet` | Signet HTTP client, identity loader, config watcher, session hooks |
| `forge-mcp` | MCP stdio client with JSON-RPC handshake |
| `forge-core` | Shared types and utilities |

### Key dependencies

- `portable-pty` — cross-platform PTY for CLI providers (macOS, Linux, Windows)
- `ratatui` — terminal UI framework
- `tokio` — async runtime
- `reqwest` — HTTP client with SSE streaming
- `rusqlite` — session persistence

---

## License

MIT
