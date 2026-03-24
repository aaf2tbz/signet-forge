use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap},
};

/// A Signet diagnostic/management command
#[derive(Debug, Clone)]
pub struct SignetCommand {
    pub key: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub kind: CommandKind,
}

#[derive(Debug, Clone)]
pub enum CommandKind {
    /// Run a CLI command (signet ...)
    Cli(&'static [&'static str]),
    /// Call a daemon API endpoint
    ApiGet(&'static str),
    ApiPost(&'static str),
    /// Handled internally by the TUI
    Internal(&'static str),
}

/// All available Signet commands
pub fn all_commands() -> Vec<SignetCommand> {
    vec![
        // Basic / built-in commands
        SignetCommand {
            key: "help",
            label: "/help",
            description: "Show all available commands",
            kind: CommandKind::Internal("help"),
        },
        SignetCommand {
            key: "signet-help",
            label: "/signet-help",
            description: "Show all Signet commands",
            kind: CommandKind::Internal("help"),
        },
        SignetCommand {
            key: "clear",
            label: "/clear",
            description: "Clear the chat history",
            kind: CommandKind::Internal("clear"),
        },
        SignetCommand {
            key: "model",
            label: "/model",
            description: "Open model picker (same as Ctrl+O)",
            kind: CommandKind::Internal("model"),
        },
        SignetCommand {
            key: "compact",
            label: "/compact",
            description: "Force context compaction",
            kind: CommandKind::Internal("compact"),
        },
        SignetCommand {
            key: "resume",
            label: "/resume",
            description: "Resume the last saved session",
            kind: CommandKind::Internal("resume"),
        },
        SignetCommand {
            key: "dashboard",
            label: "/dashboard",
            description: "Open Signet dashboard in browser",
            kind: CommandKind::Internal("dashboard"),
        },
        SignetCommand {
            key: "theme",
            label: "/theme <name>",
            description: "Switch theme (signet-dark, signet-light, midnight, amber)",
            kind: CommandKind::Internal("theme"),
        },
        SignetCommand {
            key: "effort",
            label: "/effort <level>",
            description: "Set reasoning effort (low, medium, high)",
            kind: CommandKind::Internal("effort"),
        },
        SignetCommand {
            key: "keybinds",
            label: "/keybinds",
            description: "Show current key bindings (edit ~/.config/forge/keybinds.json)",
            kind: CommandKind::Internal("keybinds"),
        },
        // Status & Diagnostics
        SignetCommand {
            key: "status",
            label: "/status",
            description: "Show agent and daemon status",
            kind: CommandKind::Cli(&["status"]),
        },
        SignetCommand {
            key: "doctor",
            label: "/doctor",
            description: "Run health checks and suggest fixes",
            kind: CommandKind::Cli(&["doctor"]),
        },
        SignetCommand {
            key: "logs",
            label: "/logs",
            description: "View last 50 daemon log lines",
            kind: CommandKind::Cli(&["daemon", "logs", "--lines", "50"]),
        },
        SignetCommand {
            key: "health",
            label: "/health",
            description: "Full daemon health report",
            kind: CommandKind::ApiGet("/health"),
        },
        SignetCommand {
            key: "diagnostics",
            label: "/diagnostics",
            description: "Composite health score across all domains",
            kind: CommandKind::ApiGet("/api/diagnostics"),
        },
        // Memory
        SignetCommand {
            key: "recall",
            label: "/recall <query>",
            description: "Search memories by query",
            kind: CommandKind::Cli(&["recall"]),
        },
        SignetCommand {
            key: "remember",
            label: "/remember <text>",
            description: "Store a new memory",
            kind: CommandKind::Cli(&["remember"]),
        },
        SignetCommand {
            key: "recall-test",
            label: "/recall-test",
            description: "Test memory search with a sample query",
            kind: CommandKind::Cli(&["recall", "test", "query"]),
        },
        // Embeddings
        SignetCommand {
            key: "embed-audit",
            label: "/embed-audit",
            description: "Audit embedding coverage and health",
            kind: CommandKind::Cli(&["embed", "audit"]),
        },
        SignetCommand {
            key: "embed-backfill",
            label: "/embed-backfill",
            description: "Backfill missing embeddings",
            kind: CommandKind::Cli(&["embed", "backfill"]),
        },
        // Skills & Secrets
        SignetCommand {
            key: "skill-list",
            label: "/skill-list",
            description: "List installed skills",
            kind: CommandKind::Cli(&["skill", "list"]),
        },
        SignetCommand {
            key: "secret-list",
            label: "/secret-list",
            description: "List configured secrets",
            kind: CommandKind::Cli(&["secret", "list"]),
        },
        // Sync & Updates
        SignetCommand {
            key: "sync",
            label: "/sync",
            description: "Sync built-in templates and skills",
            kind: CommandKind::Cli(&["sync"]),
        },
        // Daemon management
        SignetCommand {
            key: "daemon-restart",
            label: "/daemon-restart",
            description: "Restart the Signet daemon",
            kind: CommandKind::Cli(&["daemon", "restart"]),
        },
        SignetCommand {
            key: "daemon-stop",
            label: "/daemon-stop",
            description: "Stop the Signet daemon",
            kind: CommandKind::Cli(&["daemon", "stop"]),
        },
        // Repair
        SignetCommand {
            key: "repair-requeue",
            label: "/repair-requeue",
            description: "Requeue dead extraction jobs",
            kind: CommandKind::ApiPost("/api/repair/requeue-dead"),
        },
        SignetCommand {
            key: "repair-leases",
            label: "/repair-leases",
            description: "Release stale job leases",
            kind: CommandKind::ApiPost("/api/repair/release-leases"),
        },
        SignetCommand {
            key: "repair-fts",
            label: "/repair-fts",
            description: "Check and repair FTS search index",
            kind: CommandKind::ApiPost("/api/repair/check-fts"),
        },
        // Pipeline
        SignetCommand {
            key: "pipeline",
            label: "/pipeline",
            description: "Show extraction pipeline status",
            kind: CommandKind::ApiGet("/api/pipeline/status"),
        },
    ]
}

/// Interactive command picker state
pub struct CommandPicker {
    pub commands: Vec<SignetCommand>,
    pub selected: usize,
    pub filter: String,
}

impl CommandPicker {
    pub fn new() -> Self {
        Self {
            commands: all_commands(),
            selected: 0,
            filter: String::new(),
        }
    }

    pub fn filtered(&self) -> Vec<&SignetCommand> {
        if self.filter.is_empty() {
            self.commands.iter().collect()
        } else {
            let query = self.filter.to_lowercase();
            self.commands
                .iter()
                .filter(|c| {
                    c.key.contains(&query)
                        || c.label.contains(&query)
                        || c.description.to_lowercase().contains(&query)
                })
                .collect()
        }
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        let max = self.filtered().len().saturating_sub(1);
        if self.selected < max {
            self.selected += 1;
        }
    }

    pub fn selected_command(&self) -> Option<SignetCommand> {
        self.filtered().get(self.selected).map(|c| (*c).clone())
    }

    pub fn push_char(&mut self, c: char) {
        self.filter.push(c);
        self.selected = 0;
    }

    pub fn pop_char(&mut self) {
        self.filter.pop();
        self.selected = 0;
    }
}

impl Widget for &CommandPicker {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Center the overlay
        let width = 60.min(area.width.saturating_sub(4));
        let height = 22.min(area.height.saturating_sub(4));
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        let popup = Rect::new(x, y, width, height);

        // Clear background
        Clear.render(popup, buf);

        let block = Block::default()
            .title(" Signet Commands (Ctrl+G) ")
            .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));

        let inner = block.inner(popup);
        block.render(popup, buf);

        let mut lines = Vec::new();

        // Filter bar
        let filter_display = if self.filter.is_empty() {
            "  Type to filter...".to_string()
        } else {
            format!("  Filter: {}_", self.filter)
        };
        lines.push(Line::from(Span::styled(
            filter_display,
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(""));

        // Command list
        let filtered = self.filtered();
        for (i, cmd) in filtered.iter().enumerate() {
            let is_selected = i == self.selected;
            let marker = if is_selected { "▸" } else { " " };

            if is_selected {
                lines.push(Line::from(vec![
                    Span::styled(
                        format!(" {marker} "),
                        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("{:<20}", cmd.label),
                        Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        cmd.description,
                        Style::default().fg(Color::Gray),
                    ),
                ]));
            } else {
                lines.push(Line::from(vec![
                    Span::raw(format!(" {marker} ")),
                    Span::styled(
                        format!("{:<20}", cmd.label),
                        Style::default().fg(Color::Gray),
                    ),
                    Span::styled(
                        cmd.description,
                        Style::default().fg(Color::DarkGray),
                    ),
                ]));
            }
        }

        if filtered.is_empty() {
            lines.push(Line::from(Span::styled(
                "  No matching commands",
                Style::default().fg(Color::DarkGray),
            )));
        }

        // Footer
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            " ↑/↓ navigate  Enter run  Esc close",
            Style::default().fg(Color::DarkGray),
        )));

        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
        paragraph.render(inner, buf);
    }
}

/// Generate help text for /signet-help
pub fn help_text() -> String {
    let mut text = String::new();
    text.push_str("◆ Signet Commands\n\n");
    text.push_str("  Status & Diagnostics:\n");
    for cmd in all_commands().iter().filter(|c| {
        matches!(
            c.key,
            "status" | "doctor" | "logs" | "health" | "diagnostics"
        )
    }) {
        text.push_str(&format!("    {:<22} {}\n", cmd.label, cmd.description));
    }
    text.push_str("\n  Memory:\n");
    for cmd in all_commands()
        .iter()
        .filter(|c| matches!(c.key, "recall" | "remember" | "recall-test"))
    {
        text.push_str(&format!("    {:<22} {}\n", cmd.label, cmd.description));
    }
    text.push_str("\n  Embeddings:\n");
    for cmd in all_commands()
        .iter()
        .filter(|c| matches!(c.key, "embed-audit" | "embed-backfill"))
    {
        text.push_str(&format!("    {:<22} {}\n", cmd.label, cmd.description));
    }
    text.push_str("\n  Management:\n");
    for cmd in all_commands().iter().filter(|c| {
        matches!(
            c.key,
            "skill-list" | "secret-list" | "sync" | "pipeline"
        )
    }) {
        text.push_str(&format!("    {:<22} {}\n", cmd.label, cmd.description));
    }
    text.push_str("\n  Daemon:\n");
    for cmd in all_commands()
        .iter()
        .filter(|c| matches!(c.key, "daemon-restart" | "daemon-stop"))
    {
        text.push_str(&format!("    {:<22} {}\n", cmd.label, cmd.description));
    }
    text.push_str("\n  Repair:\n");
    for cmd in all_commands().iter().filter(|c| {
        matches!(c.key, "repair-requeue" | "repair-leases" | "repair-fts")
    }) {
        text.push_str(&format!("    {:<22} {}\n", cmd.label, cmd.description));
    }
    text.push_str("\n  Press Ctrl+G to open the interactive command picker.\n");
    text
}

/// Render a slash command autocomplete dropdown above the input area.
/// Shows filtered commands as greyed-out suggestions.
pub fn render_autocomplete(input: &str, area: Rect, buf: &mut Buffer) {
    let query = input.trim_start_matches('/');
    if query.is_empty() {
        // Show all commands when just "/" is typed
        render_suggestions(&all_commands(), area, buf);
        return;
    }

    let query_lower = query.to_lowercase();
    let matches: Vec<SignetCommand> = all_commands()
        .into_iter()
        .filter(|c| c.key.starts_with(&query_lower) || c.label.contains(&query_lower))
        .collect();

    if !matches.is_empty() {
        render_suggestions(&matches, area, buf);
    }
}

fn render_suggestions(commands: &[SignetCommand], area: Rect, buf: &mut Buffer) {
    // Show max 8 suggestions, positioned above the input area
    let max_show = 8.min(commands.len());
    let height = (max_show + 1) as u16; // +1 for border
    let width = 50u16.min(area.width.saturating_sub(4));

    // Position above the input area (area is the input rect)
    let y = area.y.saturating_sub(height + 1);
    let x = area.x + 2;
    let popup = Rect::new(x, y, width, height);

    // Clear background
    for row in popup.y..popup.y + popup.height {
        for col in popup.x..popup.x + popup.width {
            if col < buf.area().width && row < buf.area().height {
                buf[(col, row)]
                    .set_char(' ')
                    .set_bg(Color::Rgb(25, 25, 25));
            }
        }
    }

    // Render each suggestion
    for (i, cmd) in commands.iter().take(max_show).enumerate() {
        let row = popup.y + i as u16;
        if row >= buf.area().height {
            break;
        }

        let label_span = Span::styled(
            format!(" {:<18}", cmd.label),
            Style::default().fg(Color::DarkGray),
        );
        let desc_span = Span::styled(
            cmd.description,
            Style::default().fg(Color::Rgb(60, 60, 60)),
        );
        let line = Line::from(vec![label_span, desc_span]);
        buf.set_line(popup.x, row, &line, popup.width);
    }

    if commands.len() > max_show {
        let more_row = popup.y + max_show as u16;
        if more_row < buf.area().height {
            let more = Span::styled(
                format!(" ... {} more", commands.len() - max_show),
                Style::default().fg(Color::Rgb(50, 50, 50)),
            );
            buf.set_line(popup.x, more_row, &Line::from(more), popup.width);
        }
    }
}
