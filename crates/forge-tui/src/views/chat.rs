use crate::theme::Theme;
use crate::widgets::markdown::render_markdown;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
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
    /// Command output that clears when user starts typing
    Ephemeral(String),
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
    /// Animated status line (rendered below streaming text when processing)
    pub activity_line: Option<String>,
    /// Active color theme
    pub theme: &'a Theme,
}

impl<'a> Widget for ChatView<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let t = self.theme;
        let mut lines: Vec<Line> = Vec::new();

        // Welcome message if empty
        if self.entries.is_empty() && self.streaming_text.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  Welcome to Forge — Signet's native AI terminal.",
                Style::default().fg(t.accent),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  Type a message and press Enter to start.",
                Style::default().fg(t.muted),
            )));
            lines.push(Line::from(Span::styled(
                "  Press Ctrl+Q to quit, Ctrl+O for model picker.",
                Style::default().fg(t.muted),
            )));
        }

        for entry in self.entries {
            match entry {
                ChatEntry::UserMessage(text) => {
                    lines.push(Line::from(""));
                    let mut first = true;
                    for line in text.lines() {
                        if first {
                            lines.push(Line::from(vec![
                                Span::styled(
                                    "  > ",
                                    Style::default()
                                        .fg(t.user)
                                        .add_modifier(Modifier::BOLD),
                                ),
                                Span::styled(line, Style::default().fg(t.fg_bright)),
                            ]));
                            first = false;
                        } else {
                            lines.push(Line::from(Span::styled(
                                format!("    {line}"),
                                Style::default().fg(t.fg_bright),
                            )));
                        }
                    }
                }
                ChatEntry::AssistantText(text) => {
                    lines.push(Line::from(""));
                    let md_lines = render_markdown(text, t);
                    for md_line in md_lines {
                        let mut indented_spans = vec![Span::raw("  ")];
                        indented_spans.extend(md_line.spans);
                        lines.push(Line::from(indented_spans));
                    }
                }
                ChatEntry::Ephemeral(text) => {
                    lines.push(Line::from(""));
                    let md_lines = render_markdown(text, t);
                    for md_line in md_lines {
                        let mut indented_spans = vec![Span::raw("  ")];
                        indented_spans.extend(md_line.spans);
                        lines.push(Line::from(indented_spans));
                    }
                }
                ChatEntry::ToolCall { name, status } => {
                    let (indicator, color) = match status {
                        ToolStatus::Running => ("⟳", t.warning),
                        ToolStatus::Complete => ("✓", t.success),
                        ToolStatus::Error => ("✗", t.error),
                    };
                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(indicator, Style::default().fg(color)),
                        Span::styled(" [", Style::default().fg(t.muted)),
                        Span::styled(
                            name,
                            Style::default()
                                .fg(t.tool)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled("]", Style::default().fg(t.muted)),
                    ]));
                }
                ChatEntry::ToolOutput {
                    output, is_error, ..
                } => {
                    let style = if *is_error {
                        Style::default().fg(t.error)
                    } else {
                        Style::default().fg(t.muted)
                    };
                    let max_lines = 15;
                    for line in output.lines().take(max_lines) {
                        let display = if line.len() > 120 {
                            let boundary = line
                                .char_indices()
                                .take_while(|(i, _)| *i <= 117)
                                .last()
                                .map(|(i, c)| i + c.len_utf8())
                                .unwrap_or(117.min(line.len()));
                            format!("{}...", &line[..boundary])
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
                                .fg(t.muted)
                                .add_modifier(Modifier::ITALIC),
                        )));
                    }
                }
                ChatEntry::Error(msg) => {
                    lines.push(Line::from(""));
                    lines.push(Line::from(Span::styled(
                        format!("  Error: {msg}"),
                        Style::default().fg(t.error).add_modifier(Modifier::BOLD),
                    )));
                }
                ChatEntry::Status(msg) => {
                    lines.push(Line::from(Span::styled(
                        format!("  {msg}"),
                        Style::default()
                            .fg(t.muted)
                            .add_modifier(Modifier::ITALIC),
                    )));
                }
            }
        }

        // Streaming text (currently being generated)
        if !self.streaming_text.is_empty() {
            lines.push(Line::from(""));
            let md_lines = render_markdown(self.streaming_text, t);
            for md_line in md_lines {
                let mut indented_spans = vec![Span::raw("  ")];
                indented_spans.extend(md_line.spans);
                lines.push(Line::from(indented_spans));
            }
            // Cursor indicator
            lines.push(Line::from(Span::styled(
                "  ▌",
                Style::default().fg(t.assistant),
            )));
        }

        // Activity indicator (animated spinner during processing)
        if let Some(activity) = &self.activity_line {
            if !activity.is_empty() {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    activity.clone(),
                    Style::default().fg(t.spinner),
                )));
            }
        }

        // Calculate wrapped line count — each Line may span multiple visual rows
        let width = area.width as usize;
        let total_lines: u16 = if width == 0 {
            lines.len() as u16
        } else {
            lines
                .iter()
                .map(|line| {
                    let content_width: usize = line.spans.iter().map(|s| s.content.len()).sum();
                    1u16.max(content_width.div_ceil(width) as u16)
                })
                .sum()
        };
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
