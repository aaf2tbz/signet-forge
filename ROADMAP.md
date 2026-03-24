# Signet Forge — Implementation Roadmap

Five phases from scaffold to production. Each phase has a clear deliverable — something you can run and verify before moving to the next.

---

## Phase 1: Foundation (Weeks 1–3)
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

### Deliverable
`forge` launches, connects to Signet daemon, resolves API key, loads identity files, sends prompt to Claude, streams the response in the TUI.

---

## Phase 2: Tool Execution + Memory Integration (Weeks 4–6)
**Goal:** Full agentic coding loop with tool execution and Signet memory injection on every prompt.

### What gets built
- [x] Wire tool execution into the agentic loop (tool_use parsing → execute → result → loop back to LLM)
- [x] Permission system — auto-approve read-only tools, dialog for write tools, always-confirm for dangerous ops
- [x] Permission approval dialog in TUI (Allow / Deny / Always Allow)
- [x] Session lifecycle hooks firing at the right moments:
  - Session start → inject memories into system prompt
  - Each prompt → inject per-prompt memories
  - Pre-compaction → get summary instructions from daemon
  - Session end → submit transcript for extraction
- [x] Context window management — track token usage, trigger auto-compact at 90% capacity
- [x] Markdown rendering in chat output (pulldown-cmark)
- [ ] Syntax-highlighted code blocks (syntect) — code blocks render with borders, lang-aware highlighting is Phase 5
- [x] Tool output rendering (collapsible, truncated for long outputs)

### Deliverable
Ask Forge to read a file, edit code, run a test — it executes tools, loops back to the LLM with results, and Signet memories are injected on every prompt. Session transcripts are submitted for extraction when the session ends.

---

## Phase 3: Multi-Provider + Model Switching (Weeks 7–8)
**Goal:** Support all major providers with runtime hot-switching.

### What gets built
- [x] Provider implementations: OpenAI, Gemini, Groq, Ollama, OpenRouter, xAI
- [x] Model picker UI (Ctrl+O) — dropdown overlay with filter, arrow navigation, Enter to select
- [ ] Connect to daemon's `GET /api/pipeline/model-registry` for dynamic model discovery
- [ ] Extraction model sync — changing the primary model optionally updates the extraction model in agent.yaml
- [ ] Config file watching with `notify` crate — real-time response to agent.yaml changes
- [ ] Bidirectional config — change settings from terminal UI, writes back to agent.yaml
- [ ] Session persistence in local SQLite — save/restore conversation history
- [ ] Session resume (`forge --resume`)
- [ ] Session browser (Ctrl+H) — list past sessions, preview, resume

### Deliverable
Switch between Claude, GPT-4o, Gemini mid-conversation with Ctrl+O. Close Forge, reopen, `forge --resume` picks up where you left off. Edit agent.yaml externally and Forge picks up the change in real-time.

---

## Phase 4: MCP + Skills + Dashboard (Weeks 9–11)
**Goal:** Feature parity with Claude Code for Signet users, plus dashboard integration no other tool has.

### What gets built
- [ ] MCP client — stdio transport (subprocess JSON-RPC) and HTTP/SSE transport
- [ ] Connect to Signet marketplace MCP servers
- [ ] Connect to external MCP servers (configured in forge config)
- [ ] Signet's built-in MCP tools available natively (memory_search, memory_store, secret_exec, etc.)
- [ ] Skill loading from `~/.agents/skills/` — parse SKILL.md frontmatter, register as slash commands
- [ ] Command palette (Ctrl+K) — fuzzy search over commands + skills
- [ ] Dashboard overlay panels (F2):
  - Memory panel — recent memories, importance scores, search
  - Pipeline status — queue depth, job states, health score
  - Knowledge graph stats — entity count, relation count
  - Predictor status — sidecar health, training stats
  - Embedding health — model status, dimension, pending count
- [ ] SSE event stream consumer for real-time dashboard updates
- [ ] Sub-agent tool (spawn restricted-tool research tasks)
- [ ] WebSearch and WebFetch tools

### Deliverable
Full-featured AI coding terminal. MCP servers connected, skills invokable via slash commands, dashboard data visible without opening a browser. `Ctrl+K` discovers everything.

---

## Phase 5: Polish + Cross-Platform Release (Weeks 12–14)
**Goal:** Production-ready single binary with CI/CD.

### What gets built
- [ ] Cross-platform builds — macOS universal (ARM64 + x86_64), Linux x64/ARM64, Windows x64
- [ ] GitHub Actions CI/CD — build on push/PR, binary releases on git tag
- [ ] Theme system — signet-dark (default), signet-light, community themes
- [ ] Keyboard shortcut customization (keybindings config file)
- [ ] Error recovery — daemon connection lost (reconnect), API timeout (retry), graceful degradation
- [ ] Non-interactive mode (`forge -p "prompt"`) for scripting
- [ ] Session import from Claude Code (parse `.claude/sessions/`)
- [ ] `cargo install signet-forge` support
- [ ] Installer scripts for quick setup

### Deliverable
`forge` ships as a single binary. Download from GitHub releases or `cargo install`. Works on macOS, Linux, Windows. CI builds and tests on every push.

---

## Phase 6: Future Vision
These are stretch goals — things that become possible once the foundation is solid.

- [ ] **Windowed mode** — embed ratatui output in a winit window (like WindowedClaude, but with all of Forge's capabilities)
- [ ] **Multi-tab sessions** — multiple concurrent conversations
- [ ] **Image display** — sixel or kitty graphics protocol for inline images
- [ ] **Voice input** — whisper integration for dictation
- [ ] **Agent-to-agent collaboration** — multiple Forge instances coordinating via Signet
- [ ] **Remote sessions** — Forge running on a remote server, accessed via SSH with full TUI
- [ ] **Plugin system** — third-party tool and view extensions

---

## Crate Dependency Graph

```
forge-cli
  ├── forge-tui
  │     ├── forge-agent
  │     │     ├── forge-provider
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
