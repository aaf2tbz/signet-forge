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

        for entry in self.entries {
            match entry {
                ChatEntry::UserMessage(text) => {
                    lines.push(Line::from(""));
                    lines.push(Line::from(vec![
                        Span::styled("  User: ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                        Span::raw(text),
                    ]));
                }
                ChatEntry::AssistantText(text) => {
                    lines.push(Line::from(""));
                    lines.push(Line::from(vec![
                        Span::styled("  Forge: ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                    ]));
                    for line in text.lines() {
                        lines.push(Line::from(format!("  {line}")));
                    }
                }
                ChatEntry::ToolCall { name, status } => {
                    let indicator = match status {
                        ToolStatus::Running => Span::styled("⟳", Style::default().fg(Color::Yellow)),
                        ToolStatus::Complete => Span::styled("✓", Style::default().fg(Color::Green)),
                        ToolStatus::Error => Span::styled("✗", Style::default().fg(Color::Red)),
                    };
                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled("[", Style::default().fg(Color::DarkGray)),
                        Span::styled(name, Style::default().fg(Color::Magenta)),
                        Span::styled("] ", Style::default().fg(Color::DarkGray)),
                        indicator,
                    ]));
                }
                ChatEntry::ToolOutput { output, is_error, .. } => {
                    let style = if *is_error {
                        Style::default().fg(Color::Red)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    };
                    // Show first few lines of output
                    for line in output.lines().take(10) {
                        lines.push(Line::from(Span::styled(
                            format!("    {line}"),
                            style,
                        )));
                    }
                    let total_lines = output.lines().count();
                    if total_lines > 10 {
                        lines.push(Line::from(Span::styled(
                            format!("    ... ({} more lines)", total_lines - 10),
                            Style::default().fg(Color::DarkGray),
                        )));
                    }
                }
                ChatEntry::Error(msg) => {
                    lines.push(Line::from(Span::styled(
                        format!("  Error: {msg}"),
                        Style::default().fg(Color::Red),
                    )));
                }
                ChatEntry::Status(msg) => {
                    lines.push(Line::from(Span::styled(
                        format!("  {msg}"),
                        Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
                    )));
                }
            }
        }

        // Streaming text (currently being generated)
        if !self.streaming_text.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("  Forge: ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            ]));
            for line in self.streaming_text.lines() {
                lines.push(Line::from(format!("  {line}")));
            }
            lines.push(Line::from(Span::styled(
                "  ▌",
                Style::default().fg(Color::Green),
            )));
        }

        let paragraph = Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((self.scroll_offset, 0));

        paragraph.render(area, buf);
    }
}
