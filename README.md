# Forge

Signet's native AI terminal. Talk to any model, switch mid-conversation, and let Signet handle the memory.

```bash
forge
```

On first run, Forge checks for Signet, offers to install it, runs setup, starts the daemon, discovers your providers, and drops you into a conversation. No config files to write.

## Providers

Use API keys, installed CLI tools, or local models. Forge finds what you have and lets you pick.

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

Switch mid-session with `Ctrl+O`. CLI providers show their own model list (e.g., switch from Sonnet to Opus while staying on the CLI).

## Signet Integration

Forge talks to the Signet daemon over localhost HTTP. Memory recall takes ~5ms instead of ~200ms through shell hooks.

- **Memory** — per-prompt hybrid search (vector + keyword), speculative pre-recall while you type
- **Identity** — AGENTS.md, SOUL.md, IDENTITY.md, USER.md loaded at startup
- **Extraction** — transcripts submitted on quit, daemon handles synthesis → extraction → embedding
- **Secrets** — API keys from the encrypted secret store
- **Skills** — slash commands from `~/.agents/skills/`
- **Config** — watches `agent.yaml` for live changes

## Commands

Type `/` to see autocomplete suggestions. Press `Ctrl+G` for the interactive command picker.

```
/help                Show all commands
/recall <query>      Search memories
/remember <text>     Store a memory
/status              Agent and daemon status
/doctor              Health checks with fixes
/logs                Last 50 daemon log lines
/diagnostics         Health score across all domains
/pipeline            Extraction pipeline status
/embed-audit         Audit embedding coverage
/embed-backfill      Backfill missing embeddings
/repair-requeue      Requeue dead extraction jobs
/repair-leases       Release stale job leases
/repair-fts          Repair FTS search index
/secret-list         List configured secrets
/skill-list          List installed skills
/effort <level>      Set reasoning effort (low/medium/high)
/model               Open model picker
/theme <name>        Switch theme (signet-dark, signet-light, midnight, amber)
/keybinds            Show key bindings and config path
/clear               Clear chat
/dashboard           Open Signet dashboard in browser
```

## Key Bindings

All rebindable via `~/.config/forge/keybinds.json`.

| Key | Action |
|-----|--------|
| `Enter` | Send message |
| `Ctrl+O` | Model picker |
| `Ctrl+K` | Command palette |
| `Ctrl+G` | Signet command picker |
| `Ctrl+D` | Open dashboard |
| `Ctrl+V` | Paste (text or image) |
| `Ctrl+C` | Cancel generation |
| `Ctrl+Q` | Quit |
| `Ctrl+L` | Clear chat |
| `PageUp/Down` | Scroll |

## Features

- **Agentic loop** — prompt → memory recall → LLM → tool calls → execute → loop
- **6 tools** — Bash, Read, Write, Edit, Glob, Grep with permission system
- **Animated status** — `◇ Recalling` → `◆ Thinking` → `◈ Running [Tool]` with geometric spinners
- **Speculative pre-recall** — starts searching memories while you type (500ms debounce)
- **Prompt caching** — Anthropic system prompt cached server-side for faster TTFT
- **Parallel IO** — memory recall and provider connection warmup run concurrently
- **Context compaction** — auto-summarizes at 90% capacity
- **Session persistence** — SQLite auto-save, `--resume` to continue
- **Image support** — drag images into terminal or Ctrl+V to paste from clipboard
- **Slash autocomplete** — type `/` for greyed-out suggestions that filter as you type
- **Ephemeral output** — command output clears when you start typing
- **Markdown rendering** — headers, code blocks, bold/italic, blockquotes
- **4 themes** — signet-dark, signet-light, midnight, amber (Signet design tokens)
- **MCP client** — stdio transport with JSON-RPC for external tool servers

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

## Building

```bash
cargo build --release
# Binary at target/release/forge
```

## License

MIT
