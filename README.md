# Forge

**Signet's native AI terminal.** One binary, any model, real memory.

Forge is a terminal-native agentic AI client built in Rust. It connects directly to the [Signet](https://github.com/Signet-AI/signetai) daemon over localhost HTTP for memory, identity, secrets, and extraction — eliminating the dual-memory problem that exists when Signet runs as a plugin inside other AI harnesses.

17,000 lines of Rust across an 8-crate workspace. Zero JavaScript. Sub-5ms memory recall.

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
cargo build --release
cp target/release/forge ~/.cargo/bin/
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

## How It Works

```
┌─────────────────────────────────────┐
│              Forge TUI              │
│                                     │
│  Input ──► forge-agent loop         │
│            prompt → recall → LLM    │
│            → tools → execute → loop │
│                                     │
│  forge-provider     forge-tools     │
│  (any LLM)          (6 built-in)    │
│                                     │
│  forge-signet                       │
│  memory · identity · secrets        │
└────────────────┬────────────────────┘
                 │ HTTP (localhost:3850)
┌────────────────▼────────────────────┐
│          Signet Daemon              │
│  memory DB · extraction pipeline    │
│  embeddings · knowledge graph       │
│  secrets · session tracking         │
└─────────────────────────────────────┘
```

Forge is a thin client. The Signet daemon holds all persistent state. If Forge crashes, nothing is lost.

### Memory flow

1. **While you type** — speculative pre-recall fires after 500ms, warming the cache with relevant memories before you hit Enter
2. **On submit** — targeted recall against your actual prompt, memories injected into LLM context
3. **During session** — `/recall` and `/remember` for manual search and storage
4. **On quit** — transcript submitted to Signet's extraction pipeline (synthesis, fact extraction, embedding)

### Identity

`SOUL.md`, `IDENTITY.md`, `USER.md`, and `AGENTS.md` load from `~/.agents/` at startup into the system prompt. This is structural identity, not memory recall — it defines who the agent is before any conversation begins.

---

## Providers

Use API keys, installed CLI tools, or local models. Forge auto-detects what you have.

| Provider | How It Works |
|---|---|
| **Claude Code CLI** | Uses your installed `claude` binary — no API key needed |
| **Codex CLI** | Uses your installed `codex` binary |
| **Gemini CLI** | Uses your installed `gemini` binary |
| **Anthropic API** | Direct Messages API with streaming + prompt caching |
| **OpenAI API** | Chat Completions (GPT-4o, o4-mini) |
| **Google API** | GenerateContent (Gemini 2.5 Flash/Pro, 1M context) |
| **Groq / OpenRouter / xAI** | OpenAI-compatible APIs |
| **Ollama** | Any local model, no key needed |

Switch mid-session with `Ctrl+O`. CLI providers show their own model list.

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

All rebindable via `/keybinds` (interactive editor) or `~/.config/forge/keybinds.json`.

| Key | Action |
|-----|--------|
| `Enter` | Send message |
| `Ctrl+O` | Model picker |
| `Ctrl+K` | Command palette |
| `Ctrl+G` | Signet command picker |
| `Ctrl+D` | Dashboard navigator |
| `Ctrl+V` | Paste (text or image) |
| `Ctrl+C` | Cancel generation |
| `Ctrl+Q` | Quit |
| `Ctrl+L` | Clear chat |
| `PageUp/Down` | Scroll |

---

## Commands

Type `/` to see autocomplete suggestions. Arguments like `/effort`, `/theme`, and `/model` show predictive options as you type.

| Command | What it does |
|---|---|
| `/recall <query>` | Search memories |
| `/remember <text>` | Store a memory |
| `/model` | Open model picker |
| `/effort <level>` | Reasoning effort (low/medium/high) |
| `/theme <name>` | Switch theme (signet-dark, signet-light, midnight, amber) |
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
- **Speculative pre-recall** — starts searching memories while you type (500ms debounce)
- **4 themes** — signet-dark, signet-light, midnight, amber with full theme propagation (every element respects the active theme)
- **Animated status** — geometric spinners: `Recalling`, `Thinking`, `Running [Tool]`
- **Prompt caching** — Anthropic system prompt cached server-side for faster TTFT
- **Context compaction** — auto-summarizes at 90% capacity
- **Session persistence** — SQLite auto-save, `--resume` to continue
- **Image support** — drag images into terminal or Ctrl+V to paste
- **Markdown rendering** — headers, code blocks with language labels, bold/italic, blockquotes, lists
- **Slash autocomplete** — predictive dropdown for commands and their arguments
- **Dashboard navigator** — Ctrl+D opens page picker for every Signet dashboard tab
- **Interactive keybind editor** — rebind any key combo from within the terminal
- **MCP client** — stdio transport with JSON-RPC for external tool servers
- **Non-interactive mode** — `forge -p "query"` for scripting and pipes

---

## Architecture

8-crate Cargo workspace:

| Crate | Lines | Purpose |
|---|---|---|
| `forge-cli` | Entry point, arg parsing, Signet setup flow |
| `forge-tui` | TUI rendering, themes, overlays, keybinds |
| `forge-agent` | Agentic loop, message threading, permission system |
| `forge-provider` | LLM abstraction (Anthropic, OpenAI, Google, Groq, Ollama, CLI) |
| `forge-tools` | Bash, Read, Write, Edit, Glob, Grep implementations |
| `forge-signet` | Signet HTTP client, identity loader, config watcher, hooks |
| `forge-mcp` | MCP stdio client with JSON-RPC handshake |
| `forge-core` | Shared types and utilities |

All provider calls stream via SSE or chunked responses. Tool execution is sandboxed with user approval gates. The Signet integration layer handles memory recall, secret resolution, skill loading, and session lifecycle hooks.

---

## License

MIT
