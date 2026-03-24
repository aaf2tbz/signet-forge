use crate::input::Action;
use crate::keybinds::KeyBindConfig;
use crate::views::chat::{ChatEntry, ChatView, ToolStatus};
use crate::views::command_palette::{CommandKind as PaletteCommandKind, CommandPalette};
use crate::views::dashboard_nav::DashboardNav;
use crate::views::model_picker::ModelPicker;
use crate::views::signet_commands::{self, CommandPicker};
use crate::widgets::status_bar::StatusBar;
use crossterm::event::{self, Event, KeyEventKind};
use forge_agent::{
    AgentEvent, AgentLoop, PermissionManager, PermissionRequest, PermissionResponse, Session,
    SessionStore, SharedSession,
};
use forge_provider::{self, Provider};
use forge_signet::hooks::SessionHooks;
use forge_signet::secrets::resolve_api_key;
use forge_signet::{ConfigEvent, ConfigWatcher, SignetClient};
use ratatui::{
    layout::{Constraint, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
    DefaultTerminal, Frame,
};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, info};

/// Permission dialog state
struct PermissionDialog {
    tool_name: String,
    tool_input: serde_json::Value,
    response_tx: tokio::sync::oneshot::Sender<PermissionResponse>,
    selected: usize, // 0=Allow, 1=Always Allow, 2=Deny
}

/// What the agent is currently doing (for animated indicators)
#[derive(Debug, Clone, PartialEq)]
enum ProcessingPhase {
    Idle,
    RecallingMemories,
    Thinking,
    Streaming,
    ExecutingTool(String),
}

impl ProcessingPhase {
    /// Spinner frames — Signet-themed geometric sequence
    const FRAMES: &[&str] = &["◇", "◈", "◆", "◈"];

    fn render(&self, tick: usize) -> String {
        let frame = Self::FRAMES[tick % Self::FRAMES.len()];
        match self {
            Self::Idle | Self::Streaming => String::new(),
            Self::RecallingMemories => {
                let dots = ".".repeat((tick / 4) % 4);
                format!("  {frame} Recalling memories{dots}")
            }
            Self::Thinking => {
                let dots = ".".repeat((tick / 4) % 4);
                format!("  {frame} Thinking{dots}")
            }
            Self::ExecutingTool(name) => {
                let dots = ".".repeat((tick / 4) % 4);
                format!("  {frame} Running {name}{dots}")
            }
        }
    }
}

/// Application state
pub struct App {
    /// Current input text
    input: String,
    /// Cursor position in input
    cursor: usize,
    /// Chat history entries
    entries: Vec<ChatEntry>,
    /// Currently streaming text
    streaming_text: String,
    /// Chat scroll offset
    scroll_offset: u16,
    /// Shared session
    session: SharedSession,
    /// Provider info
    model: String,
    provider_name: String,
    context_window: usize,
    /// Active color theme
    theme: crate::theme::Theme,
    /// Keybinding configuration (loaded from ~/.config/forge/keybinds.json)
    keybinds: KeyBindConfig,
    /// Attached image paths (from drag-and-drop or paste)
    attached_images: Vec<String>,
    /// CLI binary path (if using a CLI provider)
    cli_path: Option<String>,
    /// Current reasoning effort level (shared with agent loop)
    effort: Arc<Mutex<forge_provider::ReasoningEffort>>,
    /// Memories recalled for current prompt
    memories_injected: usize,
    /// Total memories in database
    total_memories: usize,
    /// Daemon health status
    daemon_healthy: bool,
    /// Is the agent currently processing?
    processing: bool,
    /// Current processing phase (for animated status)
    processing_phase: ProcessingPhase,
    /// Animation tick counter (increments every frame)
    tick: usize,
    /// Handle to the current agent task (for cancellation)
    agent_task: Option<tokio::task::JoinHandle<()>>,
    /// Should quit?
    should_quit: bool,
    /// Agent event receiver
    event_rx: mpsc::Receiver<AgentEvent>,
    /// Permission request receiver
    permission_rx: mpsc::Receiver<PermissionRequest>,
    /// Active permission dialog
    permission_dialog: Option<PermissionDialog>,
    /// Model picker overlay
    model_picker: Option<ModelPicker>,
    /// Command palette overlay
    command_palette: Option<CommandPalette>,
    /// Signet command picker (Ctrl+G)
    command_picker: Option<CommandPicker>,
    /// Dashboard navigator (Ctrl+Tab)
    dashboard_nav: Option<DashboardNav>,
    /// Loaded skills
    skills: Vec<forge_signet::Skill>,
    /// Signet client for API key resolution on model switch
    signet_client: Option<SignetClient>,
    /// Shared permissions manager
    permissions: Arc<Mutex<PermissionManager>>,
    /// System prompt
    system_prompt: String,
    /// The agent loop
    agent: Arc<AgentLoop>,
    /// Config watcher event receiver
    config_rx: Option<mpsc::Receiver<ConfigEvent>>,
    /// Session persistence store
    session_store: Option<SessionStore>,
    /// Speculative recall — fires while user is typing to pre-warm the cache
    speculative_query: String,
    speculative_handle: Option<tokio::task::JoinHandle<()>>,
    last_keystroke: std::time::Instant,
    recall_cache: forge_signet::recall_cache::RecallCache,
    /// Pipeline summary (extraction + embedding models)
    pipeline_info: String,
}

impl App {
    pub async fn new(
        provider: Arc<dyn Provider>,
        signet_client: Option<SignetClient>,
        system_prompt: String,
        cli_path: Option<String>,
        theme_name: &str,
    ) -> Self {
        let model = provider.model().to_string();
        let provider_name = provider.name().to_string();
        let context_window = provider.context_window();

        let cwd = std::env::current_dir()
            .ok()
            .map(|p| p.display().to_string());

        let session = Session::shared(&model, &provider_name, cwd.clone());
        let session_id = session.lock().await.id.clone();

        let (event_tx, event_rx) = mpsc::channel::<AgentEvent>(256);
        let (permission_tx, permission_rx) = mpsc::channel::<PermissionRequest>(8);

        // Shared recall cache — used by both agent hooks and TUI speculative recall
        let recall_cache = forge_signet::recall_cache::RecallCache::new();

        // Set up session hooks if daemon is available
        let hooks = signet_client.as_ref().map(|client| {
            SessionHooks::with_cache(
                client.clone(),
                session_id,
                cwd.clone(),
                recall_cache.clone(),
            )
        });

        let daemon_healthy = if let Some(client) = &signet_client {
            client.is_available().await
        } else {
            false
        };

        // Call session-start hook to get initial context
        let mut memories_injected = 0;
        if let Some(hooks) = &hooks {
            match hooks.session_start().await {
                Ok((context, count)) if !context.is_empty() => {
                    debug!("Session start: {} bytes, {} memories", context.len(), count);
                    memories_injected = count;
                }
                Ok(_) => {
                    debug!("Session start hook returned empty context");
                }
                Err(e) => {
                    debug!("Session start hook failed (non-fatal): {e}");
                }
            }
        }

        let permissions = Arc::new(Mutex::new(PermissionManager::new(vec![
            "Read".to_string(),
            "Glob".to_string(),
            "Grep".to_string(),
        ])));

        let effort = Arc::new(Mutex::new(forge_provider::ReasoningEffort::default()));

        let agent = Arc::new(AgentLoop::new(
            provider,
            hooks,
            event_tx,
            permission_tx,
            Arc::clone(&permissions),
            system_prompt.clone(),
            Arc::clone(&effort),
        ));

        // Start config watcher
        let config_rx = match ConfigWatcher::start() {
            Ok((_watcher, rx)) => {
                // Keep watcher alive by leaking it (it runs in a background thread)
                // The watcher is dropped when the app exits
                std::mem::forget(_watcher);
                Some(rx)
            }
            Err(e) => {
                info!("Config watcher unavailable: {e}");
                None
            }
        };

        // Open session store
        let session_store = match SessionStore::open() {
            Ok(store) => Some(store),
            Err(e) => {
                info!("Session persistence unavailable: {e}");
                None
            }
        };

        // Load pipeline info from agent config
        let pipeline_info = forge_signet::config::load_agent_config()
            .map(|c| c.pipeline_summary())
            .unwrap_or_else(|_| "unknown".to_string());

        // Load skills from ~/.agents/skills/
        let skills = forge_signet::skills::load_skills();
        debug!("Loaded {} skills", skills.len());

        // Fetch total memory count from daemon
        let total_memories = if let Some(client) = &signet_client {
            client.memory_count().await
        } else {
            0
        };

        Self {
            input: String::new(),
            cursor: 0,
            entries: Vec::new(),
            streaming_text: String::new(),
            scroll_offset: 0,
            session,
            model,
            provider_name,
            context_window,
            theme: crate::theme::Theme::by_name(theme_name),
            keybinds: KeyBindConfig::load(),
            attached_images: Vec::new(),
            cli_path,
            effort,
            memories_injected,
            total_memories,
            daemon_healthy,
            processing: false,
            processing_phase: ProcessingPhase::Idle,
            tick: 0,
            agent_task: None,
            should_quit: false,
            event_rx,
            permission_rx,
            permission_dialog: None,
            model_picker: None,
            command_palette: None,
            command_picker: None,
            dashboard_nav: None,
            skills,
            signet_client,
            permissions,
            system_prompt,
            agent,
            config_rx,
            session_store,
            pipeline_info,
            speculative_query: String::new(),
            speculative_handle: None,
            last_keystroke: std::time::Instant::now(),
            recall_cache,
        }
    }

    /// Run the TUI event loop
    pub async fn run(&mut self, terminal: &mut DefaultTerminal) -> anyhow::Result<()> {
        info!(
            "Forge TUI starting — model: {}, provider: {}",
            self.model, self.provider_name
        );

        // Enable bracketed paste so we can detect drag-and-drop file paths
        let _ = crossterm::execute!(std::io::stdout(), crossterm::event::EnableBracketedPaste);

        loop {
            // Increment animation tick
            self.tick = self.tick.wrapping_add(1);

            // Draw
            terminal.draw(|frame| self.draw(frame))?;

            // Handle events with a short timeout so we can process agent events
            if event::poll(std::time::Duration::from_millis(50))? {
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
                        self.handle_key(key).await;
                    }
                    Event::Paste(text) => {
                        // Bracketed paste — handle file drops and multi-line paste
                        if !self.processing {
                            self.handle_paste(&text);
                        }
                    }
                    _ => {}
                }
            }

            // Drain agent events
            while let Ok(event) = self.event_rx.try_recv() {
                self.handle_agent_event(event);
            }

            // Check for permission requests
            while let Ok(request) = self.permission_rx.try_recv() {
                self.permission_dialog = Some(PermissionDialog {
                    tool_name: request.tool_name,
                    tool_input: request.tool_input,
                    response_tx: request.response_tx,
                    selected: 0,
                });
            }

            // Drain config change events silently — just update internal state
            if let Some(rx) = &mut self.config_rx {
                while let Ok(event) = rx.try_recv() {
                    match event {
                        ConfigEvent::Reloaded(config) => {
                            self.pipeline_info = config.pipeline_summary();
                            // Silent update — no chat spam
                        }
                        ConfigEvent::Error(_) => {
                            // Ignore config errors silently
                        }
                    }
                }
            }

            // Speculative pre-recall — fire after 500ms of no typing
            if !self.processing
                && !self.input.is_empty()
                && !self.input.starts_with('/')
                && self.input != self.speculative_query
                && self.last_keystroke.elapsed() > std::time::Duration::from_millis(500)
            {
                if let Some(signet) = &self.signet_client {
                let query = self.input.clone();
                self.speculative_query = query.clone();

                // Cancel any in-flight speculative task
                if let Some(handle) = self.speculative_handle.take() {
                    handle.abort();
                }

                let client = signet.clone();
                let cache = self.recall_cache.clone();
                self.speculative_handle = Some(tokio::spawn(async move {
                    // Call daemon recall and store result in shared cache
                    let body = serde_json::json!({
                        "harness": "forge",
                        "sessionId": "speculative",
                        "userMessage": query,
                        "runtimePath": "plugin",
                    });
                    if let Ok(result) = client
                        .post("/api/hooks/user-prompt-submit", &body)
                        .await
                    {
                        let injection = result
                            .get("inject")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let count = result
                            .get("memoryCount")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0) as usize;
                        if !injection.is_empty() {
                            cache.put(query, injection, count).await;
                        }
                    }
                }));
            }
            }

            if self.should_quit {
                // Auto-save session before quitting
                self.save_session().await;
                break;
            }
        }

        // Disable bracketed paste
        let _ = crossterm::execute!(std::io::stdout(), crossterm::event::DisableBracketedPaste);

        // Submit transcript for extraction
        self.submit_transcript().await;

        Ok(())
    }

    fn draw(&self, frame: &mut Frame) {
        let area = frame.area();

        // Fill the entire terminal with the theme background
        let bg_block = Block::default().style(Style::default().bg(self.theme.bg));
        frame.render_widget(bg_block, area);

        // Layout: [status_bar(2)] [chat(flex)] [input(3)]
        let chunks = Layout::vertical([
            Constraint::Length(2),  // status bar
            Constraint::Min(5),    // chat area
            Constraint::Length(3), // input area
        ])
        .split(area);

        // Read session state for display
        let (input_tokens, output_tokens) = if let Ok(s) = self.session.try_lock() {
            (s.total_input_tokens, s.total_output_tokens)
        } else {
            (0, 0)
        };

        // Status bar
        let effort_str = self
            .effort
            .try_lock()
            .map(|e| e.as_str().to_string())
            .unwrap_or_else(|_| "medium".to_string());

        let status = StatusBar {
            model: &self.model,
            provider: &self.provider_name,
            input_tokens,
            output_tokens,
            context_window: self.context_window,
            memories_injected: self.memories_injected,
            total_memories: self.total_memories,
            effort: &effort_str,
            daemon_healthy: self.daemon_healthy,
            status_bg: self.theme.status_bg,
            status_fg: self.theme.status_fg,
            accent: self.theme.accent,
            muted: self.theme.muted,
            success: self.theme.success,
            error: self.theme.error,
            warning: self.theme.warning,
        };
        status.render(chunks[0], frame.buffer_mut());

        // Chat area — render animated activity line when processing
        let activity_line = if self.processing && self.processing_phase != ProcessingPhase::Streaming {
            let rendered = self.processing_phase.render(self.tick);
            if rendered.is_empty() { None } else { Some(rendered) }
        } else {
            None
        };

        let chat = ChatView {
            entries: &self.entries,
            streaming_text: &self.streaming_text,
            scroll_offset: self.scroll_offset,
            activity_line,
            theme: &self.theme,
        };
        chat.render(chunks[1], frame.buffer_mut());

        // Input area — themed
        let input_style = if self.processing {
            Style::default().fg(self.theme.muted)
        } else {
            Style::default().fg(self.theme.fg)
        };

        let input_block = Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(self.theme.border));

        let input_text = if self.input.is_empty() && !self.processing {
            Paragraph::new(Span::styled(
                " Type a message...",
                Style::default().fg(self.theme.muted),
            ))
        } else {
            Paragraph::new(Span::styled(format!(" > {}", &self.input), input_style))
        };

        let input_widget = input_text.block(input_block);
        frame.render_widget(input_widget, chunks[2]);

        // Slash command autocomplete dropdown
        if !self.processing && self.input.starts_with('/') && self.input.len() < 30 {
            signet_commands::render_autocomplete(&self.input, chunks[2], frame.buffer_mut(), &self.theme);
        }

        // Position cursor
        if !self.processing && self.permission_dialog.is_none() {
            frame.set_cursor_position((chunks[2].x + 3 + self.cursor as u16, chunks[2].y + 1));
        }

        // Permission dialog overlay
        if let Some(dialog) = &self.permission_dialog {
            self.draw_permission_dialog(frame, dialog);
        }

        // Model picker overlay
        if let Some(picker) = &self.model_picker {
            picker.draw(frame, &self.theme);
        }

        // Command palette overlay
        if let Some(palette) = &self.command_palette {
            palette.draw(frame, &self.theme);
        }

        // Signet command picker overlay (Ctrl+G)
        if let Some(picker) = &self.command_picker {
            let area = frame.area();
            picker.render_themed(area, frame.buffer_mut(), &self.theme);
        }

        // Dashboard navigator overlay (Ctrl+Tab)
        if let Some(nav) = &self.dashboard_nav {
            let area = frame.area();
            nav.render_themed(area, frame.buffer_mut(), &self.theme);
        }
    }

    fn draw_permission_dialog(&self, frame: &mut Frame, dialog: &PermissionDialog) {
        let area = frame.area();

        // Center the dialog
        let dialog_width = 60u16.min(area.width.saturating_sub(4));
        let dialog_height = 10u16.min(area.height.saturating_sub(4));
        let x = (area.width.saturating_sub(dialog_width)) / 2;
        let y = (area.height.saturating_sub(dialog_height)) / 2;
        let dialog_area = ratatui::layout::Rect::new(x, y, dialog_width, dialog_height);

        // Clear the area behind the dialog
        frame.render_widget(Clear, dialog_area);

        // Build dialog content
        let input_preview = serde_json::to_string_pretty(&dialog.tool_input)
            .unwrap_or_else(|_| format!("{:?}", dialog.tool_input));
        let preview_lines: Vec<&str> = input_preview.lines().take(3).collect();

        let options = ["[Y] Allow", "[A] Always Allow", "[N] Deny"];

        let t = &self.theme;

        let mut lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  Tool: ", Style::default().fg(t.warning)),
                Span::styled(
                    &dialog.tool_name,
                    Style::default()
                        .fg(t.fg_bright)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
        ];

        for pl in &preview_lines {
            lines.push(Line::from(Span::styled(
                format!("  {pl}"),
                Style::default().fg(t.muted),
            )));
        }

        lines.push(Line::from(""));

        let mut option_spans = vec![Span::styled("  ", Style::default().fg(t.fg))];
        for (i, opt) in options.iter().enumerate() {
            let style = if i == dialog.selected {
                Style::default()
                    .fg(t.selected_fg)
                    .bg(t.selected_bg)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(t.fg)
            };
            option_spans.push(Span::styled(*opt, style));
            if i < options.len() - 1 {
                option_spans.push(Span::styled("  ", Style::default().fg(t.fg)));
            }
        }
        lines.push(Line::from(option_spans));

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(t.warning))
            .title(" Allow tool execution? ")
            .title_style(Style::default().fg(t.warning).add_modifier(Modifier::BOLD));

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, dialog_area);
    }

    async fn handle_key(&mut self, key: crossterm::event::KeyEvent) {
        // If dashboard navigator is open, handle nav keys
        if self.dashboard_nav.is_some() {
            self.handle_dashboard_nav_key(key).await;
            return;
        }

        // If Signet command picker is open, handle picker keys
        if self.command_picker.is_some() {
            self.handle_command_picker_key(key).await;
            return;
        }

        // If command palette is open, handle palette keys
        if self.command_palette.is_some() {
            self.handle_command_palette_key(key).await;
            return;
        }

        // If model picker is open, handle picker keys
        if self.model_picker.is_some() {
            self.handle_model_picker_key(key).await;
            return;
        }

        // If permission dialog is open, handle dialog keys
        if self.permission_dialog.is_some() {
            self.handle_permission_key(key).await;
            return;
        }

        // Resolve key event through configurable keybinds first, fall back to input.rs
        let action = if let Some(action_id) = self.keybinds.resolve(&key) {
            match action_id {
                "submit" => Action::Submit,
                "cancel" => Action::Cancel,
                "quit" => Action::Quit,
                "model_picker" => Action::ModelPicker,
                "command_palette" => Action::CommandPalette,
                "signet_commands" => Action::SignetCommands,
                "dashboard" => Action::Dashboard,
                "dashboard_nav" => Action::DashboardNav,
                "clear_screen" => Action::ClearScreen,
                "scroll_up" => Action::ScrollUp,
                "scroll_down" => Action::ScrollDown,
                "newline" => Action::NewLine,
                "paste" => Action::Paste,
                _ => Action::None,
            }
        } else {
            // Fall back to hardcoded input handling for basic editing keys
            crate::input::key_to_action(key)
        };

        match action {
            Action::Submit if !self.processing && !self.input.is_empty() => {
                let input = self.input.clone();
                self.input.clear();
                self.cursor = 0;

                // Always reset scroll to bottom when user submits
                self.scroll_offset = 0;

                // Check for slash commands before sending to LLM
                if input.starts_with('/') {
                    self.handle_slash_command(&input).await;
                } else {
                    self.processing = true;
                    self.processing_phase = ProcessingPhase::RecallingMemories;
                    self.entries.push(ChatEntry::UserMessage(input.clone()));

                    let agent = Arc::clone(&self.agent);
                    let session = Arc::clone(&self.session);
                    self.agent_task = Some(tokio::spawn(async move {
                        agent.process_message(&session, &input).await;
                    }));
                }
            }
            Action::InsertChar(c) if !self.processing => {
                // Clear ephemeral command output when user starts typing
                if self.input.is_empty() {
                    self.entries.retain(|e| !matches!(e, ChatEntry::Ephemeral(_)));
                }
                self.input.insert(self.cursor, c);
                self.cursor += 1;
                self.last_keystroke = std::time::Instant::now();
            }
            Action::Backspace if !self.processing && self.cursor > 0 => {
                self.last_keystroke = std::time::Instant::now();
                self.cursor -= 1;
                self.input.remove(self.cursor);
            }
            Action::Delete if !self.processing && self.cursor < self.input.len() => {
                self.input.remove(self.cursor);
            }
            Action::CursorLeft if self.cursor > 0 => {
                self.cursor -= 1;
            }
            Action::CursorRight if self.cursor < self.input.len() => {
                self.cursor += 1;
            }
            Action::Home => {
                self.cursor = 0;
            }
            Action::End => {
                self.cursor = self.input.len();
            }
            Action::ScrollUp => {
                self.scroll_offset = self.scroll_offset.saturating_add(3);
            }
            Action::ScrollDown => {
                self.scroll_offset = self.scroll_offset.saturating_sub(3);
            }
            Action::Cancel => {
                if self.processing {
                    // Abort the running agent task (kills CLI subprocess too)
                    if let Some(handle) = self.agent_task.take() {
                        handle.abort();
                    }
                    // Flush any partial streaming text
                    if !self.streaming_text.is_empty() {
                        self.entries
                            .push(ChatEntry::AssistantText(self.streaming_text.clone()));
                        self.streaming_text.clear();
                    }
                    self.processing = false;
                    self.processing_phase = ProcessingPhase::Idle;
                    self.entries.push(ChatEntry::Status("Cancelled.".to_string()));
                }
            }
            Action::Quit => {
                self.should_quit = true;
            }
            Action::ModelPicker if !self.processing => {
                // If currently on a CLI provider, show CLI models first
                if self.provider_name.ends_with("-cli") {
                    // Find the CLI path from the current provider
                    let cli_path = self.cli_path.clone().unwrap_or_default();
                    self.model_picker =
                        Some(ModelPicker::with_cli(&self.provider_name, &cli_path));
                } else {
                    self.model_picker = Some(ModelPicker::new());
                }
            }
            Action::CommandPalette if !self.processing => {
                self.command_palette = Some(CommandPalette::new(&self.skills));
            }
            Action::Paste if !self.processing => {
                self.clipboard_paste();
            }
            Action::ClearScreen => {
                self.entries.clear();
                self.streaming_text.clear();
                self.scroll_offset = 0;
            }
            Action::SignetCommands if !self.processing => {
                self.command_picker = Some(CommandPicker::new());
            }
            Action::DashboardNav if !self.processing => {
                self.dashboard_nav = Some(DashboardNav::new());
            }
            Action::Dashboard if !self.processing => {
                self.dashboard_nav = Some(DashboardNav::new());
            }
            _ => {}
        }
    }

    async fn handle_dashboard_nav_key(&mut self, key: crossterm::event::KeyEvent) {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc => {
                self.dashboard_nav = None;
            }
            KeyCode::Up => {
                if let Some(nav) = &mut self.dashboard_nav {
                    nav.move_up();
                }
            }
            KeyCode::Down => {
                if let Some(nav) = &mut self.dashboard_nav {
                    nav.move_down();
                }
            }
            KeyCode::Enter => {
                let url = self.dashboard_nav.as_ref().and_then(|nav| {
                    let base = self
                        .signet_client
                        .as_ref()
                        .map(|c| c.base_url().to_string())
                        .unwrap_or_else(|| "http://localhost:3850".to_string());
                    nav.selected_url(&base)
                });

                self.dashboard_nav = None;

                if let Some(url) = url {
                    let result = std::process::Command::new("open")
                        .arg(&url)
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status();

                    match result {
                        Ok(s) if s.success() => {
                            self.entries.push(ChatEntry::Status(format!(
                                "Dashboard opened: {url}"
                            )));
                        }
                        _ => {
                            self.entries.push(ChatEntry::Error(format!(
                                "Failed to open dashboard. Visit: {url}"
                            )));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    async fn handle_command_picker_key(&mut self, key: crossterm::event::KeyEvent) {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc => {
                self.command_picker = None;
            }
            KeyCode::Up => {
                if let Some(picker) = &mut self.command_picker {
                    picker.move_up();
                }
            }
            KeyCode::Down => {
                if let Some(picker) = &mut self.command_picker {
                    picker.move_down();
                }
            }
            KeyCode::Enter => {
                if let Some(picker) = &self.command_picker {
                    if let Some(cmd) = picker.selected_command() {
                        self.command_picker = None;
                        // Route through slash command handler so Internal commands work
                        self.handle_slash_command(&format!("/{}", cmd.key)).await;
                    }
                }
            }
            KeyCode::Backspace => {
                if let Some(picker) = &mut self.command_picker {
                    picker.pop_char();
                }
            }
            KeyCode::Char(c) => {
                if let Some(picker) = &mut self.command_picker {
                    picker.push_char(c);
                }
            }
            _ => {}
        }
    }

    /// Handle /slash commands typed in the input
    /// Handle pasted text — detect image file paths or insert as text
    fn handle_paste(&mut self, text: &str) {
        let trimmed = text.trim();

        // Check if it's an image file path (dragged into terminal)
        if is_image_path(trimmed) {
            let path = trimmed
                .trim_matches('\'')
                .trim_matches('"')
                .to_string();

            if std::path::Path::new(&path).exists() {
                self.attached_images.push(path.clone());
                self.entries.push(ChatEntry::Status(format!(
                    "◇ Image attached: {} ({} total)",
                    std::path::Path::new(&path)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| path.clone()),
                    self.attached_images.len()
                )));
                return;
            }
        }

        // Regular text paste — insert at cursor
        for c in trimmed.chars() {
            if c == '\n' {
                self.input.push('\n');
            } else {
                self.input.insert(self.cursor, c);
                self.cursor += 1;
            }
        }
        self.last_keystroke = std::time::Instant::now();
    }

    /// Paste from system clipboard
    fn clipboard_paste(&mut self) {
        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            // Try image first
            if let Ok(img) = clipboard.get_image() {
                // Save to temp file and attach
                let temp_path = std::env::temp_dir().join(format!(
                    "forge-paste-{}.png",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis()
                ));
                // Convert arboard image to PNG
                if let Ok(()) = save_clipboard_image(&img, &temp_path) {
                    self.attached_images.push(temp_path.display().to_string());
                    self.entries.push(ChatEntry::Status(format!(
                        "◇ Image pasted from clipboard ({} total)",
                        self.attached_images.len()
                    )));
                    return;
                }
            }

            // Fall back to text
            if let Ok(text) = clipboard.get_text() {
                if !text.is_empty() {
                    self.handle_paste(&text);
                }
            }
        }
    }

    async fn handle_slash_command(&mut self, input: &str) {
        let trimmed = input.trim_start_matches('/');
        let (cmd_name, args) = match trimmed.split_once(' ') {
            Some((name, rest)) => (name, rest.trim()),
            None => (trimmed, ""),
        };

        // Commands with arguments
        match cmd_name {
            "recall" if !args.is_empty() => {
                self.entries
                    .push(ChatEntry::Status(format!("◇ Recalling: {args}")));
                self.run_signet_cli(&["recall", args]).await;
                return;
            }
            "remember" if !args.is_empty() => {
                self.entries
                    .push(ChatEntry::Status("◇ Storing memory...".to_string()));
                self.run_signet_cli(&["remember", args]).await;
                return;
            }
            _ => {}
        }

        // Match against registered commands
        let commands = signet_commands::all_commands();
        if let Some(cmd) = commands.iter().find(|c| c.key == cmd_name) {
            match &cmd.kind {
                signet_commands::CommandKind::Internal(action) => {
                    match *action {
                        "help" => {
                            let help = signet_commands::help_text();
                            self.entries.push(ChatEntry::Ephemeral(help));
                        }
                        "clear" => {
                            self.entries.clear();
                            self.streaming_text.clear();
                            self.scroll_offset = 0;
                        }
                        "model" => {
                            if self.provider_name.ends_with("-cli") {
                                let path = self.cli_path.clone().unwrap_or_default();
                                self.model_picker = Some(ModelPicker::with_cli(&self.provider_name, &path));
                            } else {
                                self.model_picker = Some(ModelPicker::new());
                            }
                        }
                        "dashboard" => {
                            self.dashboard_nav = Some(DashboardNav::new());
                        }
                        "resume" => {
                            self.resume_last_session().await;
                        }
                        "compact" => {
                            self.entries
                                .push(ChatEntry::Status("Context compaction is automatic at 90% capacity.".to_string()));
                        }
                        "theme" => {
                            if args.is_empty() {
                                self.entries.push(ChatEntry::Status(format!(
                                    "Current theme: {}. Available: {}",
                                    self.theme.name,
                                    crate::theme::Theme::all_names().join(", ")
                                )));
                            } else {
                                self.theme = crate::theme::Theme::by_name(args);
                                self.entries.push(ChatEntry::Status(format!(
                                    "Theme set to: {}", self.theme.name
                                )));
                            }
                        }
                        "keybinds" => {
                            let text = self.keybinds.display_text();
                            self.entries.push(ChatEntry::Ephemeral(text));
                            // Save defaults if config doesn't exist yet
                            let _ = self.keybinds.save();
                        }
                        "effort" => {
                            if args.is_empty() {
                                let current = self.effort.lock().await;
                                self.entries.push(ChatEntry::Status(format!(
                                    "Current effort: {}. Usage: /effort low|medium|high",
                                    current.as_str()
                                )));
                            } else {
                                let new_effort = forge_provider::ReasoningEffort::parse(args);
                                *self.effort.lock().await = new_effort;
                                self.entries.push(ChatEntry::Status(format!(
                                    "Effort set to: {}", new_effort.as_str()
                                )));
                            }
                        }
                        _ => {}
                    }
                }
                _ => {
                    self.execute_signet_command(cmd).await;
                }
            }
        } else {
            self.entries.push(ChatEntry::Error(format!(
                "Unknown command: /{cmd_name}. Type /help for available commands."
            )));
        }
    }

    /// Execute a Signet command (CLI or API)
    async fn execute_signet_command(&mut self, cmd: &signet_commands::SignetCommand) {
        self.entries
            .push(ChatEntry::Status(format!("◇ Running {}...", cmd.label)));

        match &cmd.kind {
            signet_commands::CommandKind::Cli(args) => {
                self.run_signet_cli(args).await;
            }
            signet_commands::CommandKind::ApiGet(path) => {
                if let Some(client) = &self.signet_client {
                    match client.get(path).await {
                        Ok(resp) => {
                            let formatted =
                                serde_json::to_string_pretty(&resp).unwrap_or_default();
                            self.entries
                                .push(ChatEntry::Ephemeral(format!("```json\n{formatted}\n```")));
                        }
                        Err(e) => {
                            self.entries
                                .push(ChatEntry::Error(format!("API error: {e}")));
                        }
                    }
                } else {
                    self.entries.push(ChatEntry::Error(
                        "Signet daemon not connected".to_string(),
                    ));
                }
            }
            signet_commands::CommandKind::ApiPost(path) => {
                if let Some(client) = &self.signet_client {
                    match client
                        .post(path, &serde_json::json!({}))
                        .await
                    {
                        Ok(resp) => {
                            let formatted =
                                serde_json::to_string_pretty(&resp).unwrap_or_default();
                            self.entries.push(ChatEntry::Ephemeral(format!(
                                "```json\n{formatted}\n```"
                            )));
                        }
                        Err(e) => {
                            self.entries
                                .push(ChatEntry::Error(format!("API error: {e}")));
                        }
                    }
                } else {
                    self.entries.push(ChatEntry::Error(
                        "Signet daemon not connected".to_string(),
                    ));
                }
            }
            signet_commands::CommandKind::Internal(_) => {
                // Internal commands are handled in handle_slash_command directly
            }
        }
    }

    /// Run a signet CLI command and display output
    async fn run_signet_cli(&mut self, args: &[&str]) {
        match tokio::process::Command::new("signet")
            .args(args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .await
        {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                if !stdout.trim().is_empty() {
                    self.entries
                        .push(ChatEntry::Ephemeral(stdout.trim().to_string()));
                }
                if !stderr.trim().is_empty() {
                    self.entries.push(ChatEntry::Error(stderr.trim().to_string()));
                }
                if stdout.trim().is_empty() && stderr.trim().is_empty() {
                    self.entries
                        .push(ChatEntry::Status("Command completed.".to_string()));
                }
            }
            Err(e) => {
                self.entries.push(ChatEntry::Error(format!(
                    "Failed to run signet: {e}"
                )));
            }
        }
    }

    async fn handle_command_palette_key(&mut self, key: crossterm::event::KeyEvent) {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc => {
                self.command_palette = None;
            }
            KeyCode::Up => {
                if let Some(palette) = &mut self.command_palette {
                    palette.move_up();
                }
            }
            KeyCode::Down => {
                if let Some(palette) = &mut self.command_palette {
                    palette.move_down();
                }
            }
            KeyCode::Backspace => {
                if let Some(palette) = &mut self.command_palette {
                    palette.backspace();
                }
            }
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                if let Some(palette) = &mut self.command_palette {
                    palette.type_char(c);
                }
            }
            KeyCode::Enter => {
                let selection = self
                    .command_palette
                    .as_ref()
                    .and_then(|p| p.selected_command().cloned());
                self.command_palette = None;

                if let Some(cmd) = selection {
                    self.execute_command(&cmd).await;
                }
            }
            _ => {}
        }
    }

    async fn execute_command(&mut self, cmd: &crate::views::command_palette::CommandEntry) {
        match &cmd.kind {
            PaletteCommandKind::BuiltIn(action) => match action.as_str() {
                "model_picker" => {
                    if self.provider_name.ends_with("-cli") {
                        let path = self.cli_path.clone().unwrap_or_default();
                        self.model_picker = Some(ModelPicker::with_cli(&self.provider_name, &path));
                    } else {
                        self.model_picker = Some(ModelPicker::new());
                    }
                }
                "clear" => {
                    self.entries.clear();
                    self.streaming_text.clear();
                    self.scroll_offset = 0;
                }
                "quit" => {
                    self.should_quit = true;
                }
                "remember" => {
                    self.entries.push(ChatEntry::Status(
                        "Type /remember <content> in the input to save a memory.".to_string(),
                    ));
                }
                "recall" => {
                    self.entries.push(ChatEntry::Status(
                        "Type /recall <query> in the input to search memories.".to_string(),
                    ));
                }
                _ => {}
            },
            PaletteCommandKind::Skill(_content) => {
                // Inject skill content as a system message for the next prompt
                self.entries.push(ChatEntry::Status(format!(
                    "Skill /{} activated — type your prompt.",
                    cmd.name
                )));
                // Prepend skill content to input for next submission
                self.input = format!("[Skill: {}] ", cmd.name);
                self.cursor = self.input.len();
            }
        }
    }

    async fn handle_model_picker_key(&mut self, key: crossterm::event::KeyEvent) {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc => {
                self.model_picker = None;
            }
            KeyCode::Up => {
                if let Some(picker) = &mut self.model_picker {
                    picker.move_up();
                }
            }
            KeyCode::Down => {
                if let Some(picker) = &mut self.model_picker {
                    picker.move_down();
                }
            }
            KeyCode::Backspace => {
                if let Some(picker) = &mut self.model_picker {
                    picker.backspace();
                }
            }
            KeyCode::Char(c) if !key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                if let Some(picker) = &mut self.model_picker {
                    picker.type_char(c);
                }
            }
            KeyCode::Enter => {
                let selection = self
                    .model_picker
                    .as_ref()
                    .and_then(|p| p.selected_model().cloned());
                self.model_picker = None;

                if let Some(entry) = selection {
                    self.switch_model_entry(&entry).await;
                }
            }
            _ => {}
        }
    }

    async fn switch_model_entry(&mut self, entry: &crate::views::model_picker::ModelEntry) {
        self.switch_model(&entry.provider, &entry.model, entry.context_window, entry.cli_path.clone()).await;
    }

    async fn switch_model(&mut self, provider_name: &str, model: &str, context_window: usize, new_cli_path: Option<String>) {
        // Create new provider — CLI or API based
        let new_provider: Arc<dyn Provider> = if let Some(cli_path) = &new_cli_path {
            // CLI provider — no API key needed
            let kind = match provider_name {
                "claude-cli" => forge_provider::cli::CliKind::Claude,
                "codex-cli" => forge_provider::cli::CliKind::Codex,
                "gemini-cli" => forge_provider::cli::CliKind::Gemini,
                _ => {
                    self.entries.push(ChatEntry::Error(format!(
                        "Unknown CLI provider: {provider_name}"
                    )));
                    return;
                }
            };
            Arc::from(forge_provider::create_cli_provider(kind, cli_path, model))
        } else if provider_name == "ollama" {
            match forge_provider::create_provider(provider_name, model, "") {
                Ok(p) => Arc::from(p),
                Err(e) => {
                    self.entries
                        .push(ChatEntry::Error(format!("Failed to create provider: {e}")));
                    return;
                }
            }
        } else {
            // API provider — resolve key
            let api_key = match resolve_api_key(self.signet_client.as_ref(), provider_name).await {
                Ok(key) => key,
                Err(e) => {
                    self.entries.push(ChatEntry::Error(format!(
                        "No API key for {provider_name}: {e}"
                    )));
                    return;
                }
            };
            match forge_provider::create_provider(provider_name, model, &api_key) {
                Ok(p) => Arc::from(p),
                Err(e) => {
                    self.entries
                        .push(ChatEntry::Error(format!("Failed to create provider: {e}")));
                    return;
                }
            }
        };

        // Rebuild agent with new provider
        let cwd = std::env::current_dir()
            .ok()
            .map(|p| p.display().to_string());
        let session_id = self.session.lock().await.id.clone();

        let hooks = self.signet_client.as_ref().map(|client| {
            SessionHooks::new(client.clone(), session_id, cwd)
        });

        let (event_tx, event_rx) = mpsc::channel::<AgentEvent>(256);
        let (permission_tx, permission_rx) = mpsc::channel::<PermissionRequest>(8);

        self.agent = Arc::new(AgentLoop::new(
            new_provider,
            hooks,
            event_tx,
            permission_tx,
            Arc::clone(&self.permissions),
            self.system_prompt.clone(),
            Arc::clone(&self.effort),
        ));

        self.event_rx = event_rx;
        self.permission_rx = permission_rx;
        self.model = model.to_string();
        self.provider_name = provider_name.to_string();
        self.context_window = context_window;
        self.cli_path = new_cli_path;

        self.entries.push(ChatEntry::Status(format!(
            "Switched to {} ({})",
            model, provider_name
        )));

        info!("Model switched to {model} ({provider_name})");
    }

    async fn handle_permission_key(&mut self, key: crossterm::event::KeyEvent) {
        use crossterm::event::KeyCode;

        let response = match key.code {
            // Y or Enter on Allow
            KeyCode::Char('y') | KeyCode::Char('Y') => Some(PermissionResponse::Allow),
            // A for Always Allow
            KeyCode::Char('a') | KeyCode::Char('A') => Some(PermissionResponse::AlwaysAllow),
            // N or Escape for Deny
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                Some(PermissionResponse::Deny)
            }
            // Enter confirms current selection
            KeyCode::Enter => {
                self.permission_dialog.as_ref().map(|dialog| match dialog.selected {
                        0 => PermissionResponse::Allow,
                        1 => PermissionResponse::AlwaysAllow,
                        _ => PermissionResponse::Deny,
                    })
            }
            // Arrow keys to navigate options
            KeyCode::Left | KeyCode::Right | KeyCode::Tab => {
                if let Some(dialog) = &mut self.permission_dialog {
                    match key.code {
                        KeyCode::Left => {
                            dialog.selected = dialog.selected.saturating_sub(1);
                        }
                        KeyCode::Right | KeyCode::Tab => {
                            dialog.selected = (dialog.selected + 1).min(2);
                        }
                        _ => {}
                    }
                }
                None
            }
            _ => None,
        };

        if let Some(response) = response {
            if let Some(dialog) = self.permission_dialog.take() {
                let _ = dialog.response_tx.send(response);
            }
        }
    }

    fn handle_agent_event(&mut self, event: AgentEvent) {
        match event {
            AgentEvent::TextDelta(text) => {
                if self.processing_phase != ProcessingPhase::Streaming {
                    self.processing_phase = ProcessingPhase::Streaming;
                }
                self.streaming_text.push_str(&text);
                if self.streaming_text.len() > 512 * 1024 {
                    self.entries
                        .push(ChatEntry::AssistantText(self.streaming_text.clone()));
                    self.streaming_text.clear();
                }
                self.scroll_offset = 0;
            }
            AgentEvent::ToolStart { name, .. } => {
                self.processing_phase = ProcessingPhase::ExecutingTool(name.clone());
                if !self.streaming_text.is_empty() {
                    self.entries
                        .push(ChatEntry::AssistantText(self.streaming_text.clone()));
                    self.streaming_text.clear();
                }
                self.entries.push(ChatEntry::ToolCall {
                    name,
                    status: ToolStatus::Running,
                });
            }
            AgentEvent::ToolResult {
                name,
                output,
                is_error,
                ..
            } => {
                // Update tool call status
                if let Some(entry) = self.entries.iter_mut().rev().find(|e| {
                    matches!(e, ChatEntry::ToolCall { name: n, status: ToolStatus::Running } if *n == name)
                }) {
                    *entry = ChatEntry::ToolCall {
                        name: name.clone(),
                        status: if is_error {
                            ToolStatus::Error
                        } else {
                            ToolStatus::Complete
                        },
                    };
                }
                self.entries.push(ChatEntry::ToolOutput {
                    name,
                    output,
                    is_error,
                });
            }
            AgentEvent::Usage(_) => {
                // Token counts are updated in the shared session directly
            }
            AgentEvent::TurnComplete => {
                if !self.streaming_text.is_empty() {
                    self.entries
                        .push(ChatEntry::AssistantText(self.streaming_text.clone()));
                    self.streaming_text.clear();
                }
                self.processing = false;
                self.processing_phase = ProcessingPhase::Idle;
            }
            AgentEvent::Error(msg) => {
                self.streaming_text.clear();
                self.entries.push(ChatEntry::Error(msg));
                self.processing = false;
                self.processing_phase = ProcessingPhase::Idle;
            }
            AgentEvent::Status(msg) => {
                // Update processing phase based on status message
                if msg.contains("Recalling") {
                    self.processing_phase = ProcessingPhase::RecallingMemories;
                } else if msg.contains("Thinking") {
                    self.processing_phase = ProcessingPhase::Thinking;
                } else if msg.contains("Compacting") {
                    self.processing_phase = ProcessingPhase::ExecutingTool("compaction".to_string());
                }
                // Don't push static status entries for animated phases
                // — they're rendered as the activity line instead
            }
            AgentEvent::ToolApproval(_, name, _) => {
                self.entries.push(ChatEntry::Status(format!(
                    "Waiting for approval: {name}..."
                )));
            }
            AgentEvent::MemoryCount(count) => {
                self.memories_injected = count;
            }
        }
    }

    /// Save current session to SQLite for later resume
    async fn save_session(&self) {
        let store = match &self.session_store {
            Some(s) => s,
            None => return,
        };

        let s = self.session.lock().await;
        if s.messages.is_empty() {
            return;
        }

        if let Err(e) = store.save_session(
            &s.id,
            &s.model,
            &s.provider,
            s.project.as_deref(),
            &s.started_at.to_rfc3339(),
            &s.messages,
            s.total_input_tokens,
            s.total_output_tokens,
        ) {
            info!("Failed to save session: {e}");
        } else {
            info!("Session {} saved ({} messages)", s.id, s.messages.len());
        }
    }

    /// Submit transcript to Signet daemon for extraction on quit
    async fn submit_transcript(&self) {
        let s = self.session.lock().await;
        if s.messages.is_empty() {
            return;
        }

        let transcript = s.transcript();
        let session_id = s.id.clone();
        let project = s.project.clone();
        drop(s); // Release lock before async call

        if let Some(client) = &self.signet_client {
            let hooks = SessionHooks::new(client.clone(), session_id, project);
            if let Err(e) = hooks.session_end(&transcript).await {
                info!("Session-end hook failed (non-fatal): {e}");
            }
        }
    }

    /// Load a previous session from SQLite (for --resume)
    pub async fn resume_last_session(&mut self) -> bool {
        let store = match &self.session_store {
            Some(s) => s,
            None => return false,
        };

        let session_id = match store.last_session_id() {
            Some(id) => id,
            None => {
                self.entries
                    .push(ChatEntry::Status("No previous session found.".to_string()));
                return false;
            }
        };

        match store.load_messages(&session_id) {
            Ok(messages) if !messages.is_empty() => {
                let mut s = self.session.lock().await;
                s.messages = messages;
                let count = s.messages.len();
                drop(s);

                // Replay messages into chat entries
                self.entries
                    .push(ChatEntry::Status(format!("Resumed session {session_id} ({count} messages)")));

                true
            }
            Ok(_) => {
                self.entries
                    .push(ChatEntry::Status("Previous session was empty.".to_string()));
                false
            }
            Err(e) => {
                self.entries
                    .push(ChatEntry::Error(format!("Failed to resume: {e}")));
                false
            }
        }
    }
}

/// Check if a path looks like an image file
fn is_image_path(text: &str) -> bool {
    let path = text
        .trim()
        .trim_matches('\'')
        .trim_matches('"');
    let lower = path.to_lowercase();
    lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".gif")
        || lower.ends_with(".webp")
        || lower.ends_with(".bmp")
        || lower.ends_with(".svg")
}

/// Save an arboard clipboard image to a PNG file
fn save_clipboard_image(
    img: &arboard::ImageData,
    path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    // arboard gives us RGBA bytes
    let width = img.width as u32;
    let height = img.height as u32;

    let file = std::fs::File::create(path)?;
    let writer = std::io::BufWriter::new(file);

    let mut encoder = png::Encoder::new(writer, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);

    let mut writer = encoder.write_header()?;
    writer.write_image_data(&img.bytes)?;

    Ok(())
}
