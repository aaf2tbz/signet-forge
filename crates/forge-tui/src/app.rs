use crate::input::{key_to_action, Action};
use crate::views::chat::{ChatEntry, ChatView, ToolStatus};
use crate::widgets::status_bar::StatusBar;
use crossterm::event::{self, Event, KeyEventKind};
use forge_agent::{AgentEvent, AgentLoop, Session};
use forge_provider::Provider;
use forge_signet::hooks::SessionHooks;
use forge_signet::SignetClient;
use ratatui::{
    layout::{Constraint, Layout},
    style::{Color, Style},
    text::Span,
    widgets::{Block, Borders, Paragraph, Widget},
    DefaultTerminal, Frame,
};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::info;

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
    /// Session
    session: Session,
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
    /// Agent event sender (for spawning agent tasks)
    event_tx: mpsc::Sender<AgentEvent>,
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

        let session = Session::new(&model, &provider_name, cwd.clone());

        let (event_tx, event_rx) = mpsc::channel::<AgentEvent>(256);

        // Set up session hooks if daemon is available
        let hooks = signet_client.as_ref().map(|client| {
            SessionHooks::new(client.clone(), session.id.clone(), cwd)
        });

        let daemon_healthy = if let Some(client) = &signet_client {
            client.is_available().await
        } else {
            false
        };

        let agent = Arc::new(AgentLoop::new(
            provider,
            hooks,
            event_tx.clone(),
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
            memories_injected: 0,
            daemon_healthy,
            processing: false,
            should_quit: false,
            event_rx,
            event_tx,
            agent,
        }
    }

    /// Run the TUI event loop
    pub async fn run(&mut self, terminal: &mut DefaultTerminal) -> anyhow::Result<()> {
        info!("Forge TUI starting — model: {}, provider: {}", self.model, self.provider_name);

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

        // Status bar
        let status = StatusBar {
            model: &self.model,
            provider: &self.provider_name,
            input_tokens: self.session.total_input_tokens,
            output_tokens: self.session.total_output_tokens,
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
            Paragraph::new(Span::styled(
                format!(" > {}", &self.input),
                input_style,
            ))
        };

        let input_widget = input_text.block(input_block);
        frame.render_widget(input_widget, chunks[2]);

        // Position cursor
        if !self.processing {
            frame.set_cursor_position((
                chunks[2].x + 3 + self.cursor as u16,
                chunks[2].y + 1,
            ));
        }
    }

    async fn handle_key(&mut self, key: crossterm::event::KeyEvent) {
        let action = key_to_action(key);

        match action {
            Action::Submit if !self.processing && !self.input.is_empty() => {
                let input = self.input.clone();
                self.input.clear();
                self.cursor = 0;
                self.processing = true;

                self.entries.push(ChatEntry::UserMessage(input.clone()));

                // Spawn agent task
                let agent = Arc::clone(&self.agent);
                let mut session = Session::new(&self.model, &self.provider_name, self.session.project.clone());
                // Transfer existing messages
                session.messages = self.session.messages.clone();
                session.total_input_tokens = self.session.total_input_tokens;
                session.total_output_tokens = self.session.total_output_tokens;

                // Spawn agent task
                tokio::spawn(async move {
                    agent.process_message(&mut session, &input).await;
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
                    // TODO: cancel current generation
                    self.processing = false;
                    self.streaming_text.clear();
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

    fn handle_agent_event(&mut self, event: AgentEvent) {
        match event {
            AgentEvent::TextDelta(text) => {
                self.streaming_text.push_str(&text);
                // Auto-scroll to bottom
                self.scroll_offset = 0;
            }
            AgentEvent::ToolStart { name, .. } => {
                // Flush streaming text if any
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
                // Update the tool call status
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
            AgentEvent::Usage(usage) => {
                self.session.total_input_tokens += usage.input_tokens;
                self.session.total_output_tokens += usage.output_tokens;
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
            AgentEvent::ToolApproval { .. } => {
                // TODO: Phase 2 — show approval dialog
            }
        }
    }
}
