# Signet Forge — Implementation Roadmap

Six phases from scaffold to production. Each phase has a clear deliverable — something you can run and verify before moving to the next.

---

## Phase 1: Foundation ✅
**Goal:** Basic conversational AI terminal that calls one provider and streams responses.

### What gets built
- [x] Cargo workspace with 8 crates (core, provider, tools, mcp, signet, agent, tui, cli)
- [x] `forge-core` — Message, Tool, Config, Error types
- [x] `forge-provider` — Anthropic provider (Messages API + SSE streaming)
- [x] `forge-signet/client` — HTTP client for Signet daemon
- [x] `forge-signet/secrets` — API key resolution from daemon secret store
- [x] `forge-signet/config` — agent.yaml loading, identity file reading (SOUL.md, IDENTITY.md, USER.md, AGENTS.md)
- [x] `forge-signet/hooks` — Session lifecycle hooks (start, prompt-submit, pre-compaction, end)
- [x] `forge-signet/memory` — Memory recall and store via daemon API
- [x] `forge-tools` — 6 built-in tools (Bash, Read, Write, Edit, Glob, Grep)
- [x] `forge-agent` — Agentic loop (message → LLM → tool calls → execute → loop)
- [x] `forge-tui` — Chat view, status bar, input handling, key bindings
- [x] `forge-cli` — CLI entry point with clap (model, provider, daemon-url, no-daemon, resume flags)

---

## Phase 2: Tool Execution + Memory Integration ✅
**Goal:** Full agentic coding loop with tool execution and Signet memory injection on every prompt.

### What gets built
- [x] Wire tool execution into the agentic loop (tool_use parsing → execute → result → loop back to LLM)
- [x] Permission system — auto-approve read-only tools, dialog for write tools, always-confirm for dangerous ops
- [x] Permission approval dialog in TUI (Allow / Deny / Always Allow)
- [x] Session lifecycle hooks firing at the right moments
- [x] Context window management — track token usage, trigger auto-compact at 90% capacity
- [x] Markdown rendering in chat output (pulldown-cmark)
- [x] Tool output rendering (collapsible, truncated for long outputs)
- [ ] Syntax-highlighted code blocks (syntect) — code blocks render with borders, lang-aware highlighting deferred

---

## Phase 3: Multi-Provider + Model Switching ✅
**Goal:** Support all major providers with runtime hot-switching.

### What gets built
- [x] Provider implementations: OpenAI, Gemini, Groq, Ollama, OpenRouter, xAI
- [x] CLI providers via PTY: Claude Code, Codex, Gemini — real pseudo-terminal with 64KB buffer
- [x] Model picker UI (Ctrl+O) — shows both API and CLI models regardless of current provider
- [x] Config file watching with `notify` crate — real-time response to agent.yaml changes
- [x] Session persistence in local SQLite — auto-save on quit, load on resume
- [x] Session resume (`forge --resume`) — restores last session's message history
- [x] Persistent settings — model, provider, effort, theme, bypass saved to `~/.config/forge/settings.json`
- [x] CLI tool detection at startup — auto-discovers installed `claude`, `codex`, `gemini`
- [ ] Connect to daemon's `GET /api/pipeline/model-registry` for dynamic model discovery
- [ ] Extraction model sync — changing primary model optionally updates extraction model
- [ ] Session browser (Ctrl+H) — list past sessions, preview, resume

---

## Phase 4: MCP + Skills + Dashboard ✅
**Goal:** Feature parity with Claude Code for Signet users, plus dashboard integration no other tool has.

### What gets built
- [x] MCP client — stdio transport (subprocess JSON-RPC with initialize handshake)
- [x] MCP tool routing — agent loop tries MCP clients for unknown tools (fallback chain)
- [x] Skill loading from `~/.agents/skills/` — parse SKILL.md frontmatter, register as slash commands
- [x] Command palette (Ctrl+K) — fuzzy search over built-in commands + skills
- [x] CLI tool visibility — tool_use, tool_result, and code changes from CLI stream-json render as cards
- [x] Signet native tools — memory_search, memory_store, knowledge_expand, secret_exec via daemon HTTP
- [x] Dashboard overlay panel (F2) — tabbed view: Memory, Pipeline, Embeddings, Health with live data
- [x] WebSearch tool — DuckDuckGo HTML search, no API key
- [x] WebFetch tool — fetch + strip HTML to text, 50K char limit
- [x] 12 built-in tools total (Bash, Read, Write, Edit, Glob, Grep, WebSearch, WebFetch, memory_search, memory_store, knowledge_expand, secret_exec)

---

## Phase 5: Polish + Cross-Platform Release ✅
**Goal:** Production-ready single binary with CI/CD.

### What gets built
- [x] Cross-platform builds — macOS ARM64/x64, Linux x64 (GitHub Actions matrix)
- [x] GitHub Actions CI/CD — build.yml (check + clippy + build), release.yml (binary releases on tag)
- [x] Theme system — 4 themes (signet-dark, signet-light, midnight, amber) with dedicated spinner colors
- [x] Keyboard shortcut customization — `~/.config/forge/keybinds.json` + interactive editor overlay (Ctrl+B)
- [x] Dynamic header — keybind hints reflect custom bindings in real-time
- [x] Non-interactive mode (`forge -p "prompt"`) — streams response to stdout, exits
- [x] Type-ahead input — compose next message while model is thinking/streaming
- [x] Expanding input box — grows with content, wraps text, scrolls to cursor, snaps back on send
- [x] Tab autocomplete — predictive completion for all slash commands and arguments
- [x] `/forge-bypass` — toggle CLI permission bypass mid-session (Claude: `--dangerously-skip-permissions`, Codex: `--yolo`)
- [x] `/effort` — reasoning effort with persistence across sessions
- [x] Contextual animated status verbs — Thinking, Deliberating, Hypothesizing, Riddling, etc. (~4s cycle)
- [x] Chat auto-scroll — accounts for word-wrapped lines, keeps latest content visible
- [ ] Error recovery — daemon connection lost (reconnect), API timeout (retry), graceful degradation
- [ ] Session import from Claude Code (parse `.claude/sessions/`)
- [ ] Installer scripts for quick setup

---

## Phase 6: Future Vision
These are stretch goals — things that become possible once the foundation is solid.

- [ ] **Windowed mode** — embed ratatui output in a winit window (like WindowedClaude, but with Forge's full capabilities)
- [ ] **Multi-tab sessions** — multiple concurrent conversations
- [ ] **Image display** — sixel or kitty graphics protocol for inline images
- [ ] **Voice input** — whisper integration for dictation
- [ ] **Agent-to-agent collaboration** — multiple Forge instances coordinating via Signet cross-agent API
- [ ] **Multi-agent support** — thread `agent_id` on all daemon calls, per-agent identity files
- [ ] **Remote sessions** — Forge running on a remote server, accessed via SSH with full TUI
- [ ] **Plugin system** — third-party tool and view extensions
- [ ] **Interactive CLI prompts** — detect and respond to CLI approval prompts (write to file? y/n) from within the TUI
- [ ] **Syntax highlighting** — syntect-based code block coloring per language
- [ ] **SSE event stream** — real-time dashboard updates from daemon event bus
- [ ] **Sub-agent tool** — spawn restricted-tool research tasks in parallel
- [ ] **Marketplace MCP proxy** — route daemon marketplace proxy tools through Forge
- [ ] **External MCP config** — configure and connect to arbitrary MCP servers

---

## Crate Dependency Graph

```
forge-cli
  ├── forge-tui
  │     ├── forge-agent
  │     │     ├── forge-provider (+ portable-pty)
  │     │     │     └── forge-core
  │     │     ├── forge-tools
  │     │     │     └── forge-core
  │     │     ├── forge-mcp
  │     │     │     └── forge-core
  │     │     └── forge-signet
  │     │           └── forge-core
  │     ├── forge-provider
  │     └── forge-signet
  └── forge-signet
```

Each crate is independently testable. `forge-core` has zero external runtime dependencies beyond serde. `forge-provider` can be tested against mock HTTP servers. `forge-tools` can be tested with real filesystem operations. `forge-signet` can be tested against a running daemon or mocked responses.
