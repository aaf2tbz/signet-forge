use crate::input::{key_to_action, Action};
use crate::views::chat::{ChatEntry, ChatView, ToolStatus};
use crate::widgets::status_bar::StatusBar;
use crossterm::event::{self, Event, KeyEventKind};
use forge_agent::{
    AgentEvent, AgentLoop, PermissionManager, PermissionRequest, PermissionResponse, Session,
    SharedSession,
};
use forge_provider::Provider;
use forge_signet::hooks::SessionHooks;
use forge_signet::SignetClient;
use ratatui::{
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
    DefaultTerminal, Frame,
};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tracing::info;

/// Permission dialog state
struct PermissionDialog {
    tool_name: String,
    tool_input: serde_json::Value,
    response_tx: tokio::sync::oneshot::Sender<PermissionResponse>,
    selected: usize, // 0=Allow, 1=Always Allow, 2=Deny
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
    /// Memories injected this session
    memories_injected: usize,
    /// Daemon health status
    daemon_healthy: bool,
    /// Is the agent currently processing?
    processing: bool,
    /// Should quit?
    should_quit: bool,
    /// Agent event receiver
    event_rx: mpsc::Receiver<AgentEvent>,
    /// Permission request receiver
    permission_rx: mpsc::Receiver<PermissionRequest>,
    /// Active permission dialog
    permission_dialog: Option<PermissionDialog>,
    /// The agent loop
    agent: Arc<AgentLoop>,
}

impl App {
    pub async fn new(
        provider: Arc<dyn Provider>,
        signet_client: Option<SignetClient>,
        system_prompt: String,
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

        // Set up session hooks if daemon is available
        let hooks = signet_client.as_ref().map(|client| {
            SessionHooks::new(client.clone(), session_id, cwd.clone())
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
                Ok(context) if !context.is_empty() => {
                    info!("Session start hook returned {} bytes", context.len());
                    memories_injected = context.matches("memory").count().max(1);
                }
                Ok(_) => {}
                Err(e) => {
                    info!("Session start hook failed (non-fatal): {e}");
                }
            }
        }

        let permissions = Arc::new(Mutex::new(PermissionManager::new(vec![
            "Read".to_string(),
            "Glob".to_string(),
            "Grep".to_string(),
        ])));

        let agent = Arc::new(AgentLoop::new(
            provider,
            hooks,
            event_tx,
            permission_tx,
            permissions,
            system_prompt,
        ));

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
            memories_injected,
            daemon_healthy,
            processing: false,
            should_quit: false,
            event_rx,
            permission_rx,
            permission_dialog: None,
            agent,
        }
    }

    /// Run the TUI event loop
    pub async fn run(&mut self, terminal: &mut DefaultTerminal) -> anyhow::Result<()> {
        info!(
            "Forge TUI starting — model: {}, provider: {}",
            self.model, self.provider_name
        );

        loop {
            // Draw
            terminal.draw(|frame| self.draw(frame))?;

            // Handle events with a short timeout so we can process agent events
            if event::poll(std::time::Duration::from_millis(16))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        self.handle_key(key).await;
                    }
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

            if self.should_quit {
                break;
            }
        }

        Ok(())
    }

    fn draw(&self, frame: &mut Frame) {
        let area = frame.area();

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
        let status = StatusBar {
            model: &self.model,
            provider: &self.provider_name,
            input_tokens,
            output_tokens,
            context_window: self.context_window,
            memories_injected: self.memories_injected,
            daemon_healthy: self.daemon_healthy,
        };
        status.render(chunks[0], frame.buffer_mut());

        // Chat area
        let chat = ChatView {
            entries: &self.entries,
            streaming_text: &self.streaming_text,
            scroll_offset: self.scroll_offset,
        };
        chat.render(chunks[1], frame.buffer_mut());

        // Input area
        let input_style = if self.processing {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(Color::White)
        };

        let input_block = Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(Color::DarkGray));

        let input_text = if self.input.is_empty() && !self.processing {
            Paragraph::new(Span::styled(
                " Type a message...",
                Style::default().fg(Color::DarkGray),
            ))
        } else {
            Paragraph::new(Span::styled(format!(" > {}", &self.input), input_style))
        };

        let input_widget = input_text.block(input_block);
        frame.render_widget(input_widget, chunks[2]);

        // Position cursor
        if !self.processing && self.permission_dialog.is_none() {
            frame.set_cursor_position((chunks[2].x + 3 + self.cursor as u16, chunks[2].y + 1));
        }

        // Permission dialog overlay
        if let Some(dialog) = &self.permission_dialog {
            self.draw_permission_dialog(frame, dialog);
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

        let mut lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  Tool: ", Style::default().fg(Color::Yellow)),
                Span::styled(
                    &dialog.tool_name,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
        ];

        for pl in &preview_lines {
            lines.push(Line::from(Span::styled(
                format!("  {pl}"),
                Style::default().fg(Color::DarkGray),
            )));
        }

        lines.push(Line::from(""));

        let mut option_spans = vec![Span::raw("  ")];
        for (i, opt) in options.iter().enumerate() {
            let style = if i == dialog.selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            option_spans.push(Span::styled(*opt, style));
            if i < options.len() - 1 {
                option_spans.push(Span::raw("  "));
            }
        }
        lines.push(Line::from(option_spans));

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow))
            .title(" Allow tool execution? ");

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, dialog_area);
    }

    async fn handle_key(&mut self, key: crossterm::event::KeyEvent) {
        // If permission dialog is open, handle dialog keys
        if self.permission_dialog.is_some() {
            self.handle_permission_key(key).await;
            return;
        }

        let action = key_to_action(key);

        match action {
            Action::Submit if !self.processing && !self.input.is_empty() => {
                let input = self.input.clone();
                self.input.clear();
                self.cursor = 0;
                self.processing = true;

                self.entries.push(ChatEntry::UserMessage(input.clone()));

                // Spawn agent task with shared session
                let agent = Arc::clone(&self.agent);
                let session = Arc::clone(&self.session);

                tokio::spawn(async move {
                    agent.process_message(&session, &input).await;
                });
            }
            Action::InsertChar(c) if !self.processing => {
                self.input.insert(self.cursor, c);
                self.cursor += 1;
            }
            Action::Backspace if !self.processing && self.cursor > 0 => {
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
                    self.processing = false;
                    self.streaming_text.clear();
                    self.entries.push(ChatEntry::Status("Cancelled.".to_string()));
                }
            }
            Action::Quit => {
                self.should_quit = true;
            }
            Action::ClearScreen => {
                self.entries.clear();
                self.streaming_text.clear();
                self.scroll_offset = 0;
            }
            _ => {}
        }
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
                if let Some(dialog) = &self.permission_dialog {
                    Some(match dialog.selected {
                        0 => PermissionResponse::Allow,
                        1 => PermissionResponse::AlwaysAllow,
                        _ => PermissionResponse::Deny,
                    })
                } else {
                    None
                }
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
                self.streaming_text.push_str(&text);
                self.scroll_offset = 0;
            }
            AgentEvent::ToolStart { name, .. } => {
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
            }
            AgentEvent::Error(msg) => {
                self.streaming_text.clear();
                self.entries.push(ChatEntry::Error(msg));
                self.processing = false;
            }
            AgentEvent::Status(msg) => {
                self.entries.push(ChatEntry::Status(msg));
            }
            AgentEvent::ToolApproval(_, name, _) => {
                self.entries.push(ChatEntry::Status(format!(
                    "Waiting for approval: {name}..."
                )));
            }
        }
    }
}
