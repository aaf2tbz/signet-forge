use crate::widgets::markdown::render_markdown;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget, Wrap},
};

/// A single entry in the chat view
#[derive(Debug, Clone)]
pub enum ChatEntry {
    UserMessage(String),
    AssistantText(String),
    ToolCall {
        name: String,
        status: ToolStatus,
    },
    ToolOutput {
        name: String,
        output: String,
        is_error: bool,
    },
    Error(String),
    Status(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ToolStatus {
    Running,
    Complete,
    Error,
}

/// The main chat view widget
pub struct ChatView<'a> {
    pub entries: &'a [ChatEntry],
    pub streaming_text: &'a str,
    pub scroll_offset: u16,
}

impl<'a> Widget for ChatView<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut lines: Vec<Line> = Vec::new();

        // Welcome message if empty
        if self.entries.is_empty() && self.streaming_text.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  Welcome to Forge — Signet's native AI terminal.",
                Style::default().fg(Color::Cyan),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  Type a message and press Enter to start.",
                Style::default().fg(Color::DarkGray),
            )));
            lines.push(Line::from(Span::styled(
                "  Press Ctrl+D to quit, Ctrl+O for model picker.",
                Style::default().fg(Color::DarkGray),
            )));
        }

        for entry in self.entries {
            match entry {
                ChatEntry::UserMessage(text) => {
                    lines.push(Line::from(""));
                    // Show user message with each line
                    let mut first = true;
                    for line in text.lines() {
                        if first {
                            lines.push(Line::from(vec![
                                Span::styled(
                                    "  > ",
                                    Style::default()
                                        .fg(Color::Cyan)
                                        .add_modifier(Modifier::BOLD),
                                ),
                                Span::styled(line, Style::default().fg(Color::White)),
                            ]));
                            first = false;
                        } else {
                            lines.push(Line::from(Span::styled(
                                format!("    {line}"),
                                Style::default().fg(Color::White),
                            )));
                        }
                    }
                }
                ChatEntry::AssistantText(text) => {
                    lines.push(Line::from(""));
                    let md_lines = render_markdown(text);
                    for md_line in md_lines {
                        // Indent markdown lines
                        let mut indented_spans = vec![Span::raw("  ")];
                        indented_spans.extend(md_line.spans);
                        lines.push(Line::from(indented_spans));
                    }
                }
                ChatEntry::ToolCall { name, status } => {
                    let (indicator, color) = match status {
                        ToolStatus::Running => ("⟳", Color::Yellow),
                        ToolStatus::Complete => ("✓", Color::Green),
                        ToolStatus::Error => ("✗", Color::Red),
                    };
                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(indicator, Style::default().fg(color)),
                        Span::styled(" [", Style::default().fg(Color::DarkGray)),
                        Span::styled(
                            name,
                            Style::default()
                                .fg(Color::Magenta)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled("]", Style::default().fg(Color::DarkGray)),
                    ]));
                }
                ChatEntry::ToolOutput {
                    output, is_error, ..
                } => {
                    let style = if *is_error {
                        Style::default().fg(Color::Red)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    };
                    // Show first few lines of output, indented
                    let max_lines = 15;
                    for line in output.lines().take(max_lines) {
                        // Truncate long lines
                        let display = if line.len() > 120 {
                            format!("{}...", &line[..117])
                        } else {
                            line.to_string()
                        };
                        lines.push(Line::from(Span::styled(
                            format!("    {display}"),
                            style,
                        )));
                    }
                    let total_lines = output.lines().count();
                    if total_lines > max_lines {
                        lines.push(Line::from(Span::styled(
                            format!("    ... ({} more lines)", total_lines - max_lines),
                            Style::default()
                                .fg(Color::DarkGray)
                                .add_modifier(Modifier::ITALIC),
                        )));
                    }
                }
                ChatEntry::Error(msg) => {
                    lines.push(Line::from(""));
                    lines.push(Line::from(Span::styled(
                        format!("  Error: {msg}"),
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                    )));
                }
                ChatEntry::Status(msg) => {
                    lines.push(Line::from(Span::styled(
                        format!("  {msg}"),
                        Style::default()
                            .fg(Color::DarkGray)
                            .add_modifier(Modifier::ITALIC),
                    )));
                }
            }
        }

        // Streaming text (currently being generated)
        if !self.streaming_text.is_empty() {
            lines.push(Line::from(""));
            let md_lines = render_markdown(self.streaming_text);
            for md_line in md_lines {
                let mut indented_spans = vec![Span::raw("  ")];
                indented_spans.extend(md_line.spans);
                lines.push(Line::from(indented_spans));
            }
            // Cursor indicator
            lines.push(Line::from(Span::styled(
                "  ▌",
                Style::default().fg(Color::Green),
            )));
        }

        // Auto-scroll: if scroll_offset is 0, show the bottom
        let total_lines = lines.len() as u16;
        let visible_lines = area.height;
        let scroll = if self.scroll_offset == 0 {
            total_lines.saturating_sub(visible_lines)
        } else {
            total_lines
                .saturating_sub(visible_lines)
                .saturating_sub(self.scroll_offset)
        };

        let paragraph = Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0));

        paragraph.render(area, buf);
    }
}
