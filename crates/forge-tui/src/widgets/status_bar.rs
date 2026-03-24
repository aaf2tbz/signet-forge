use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Widget,
};

/// Bottom status bar showing model, tokens, and keybindings
pub struct StatusBar<'a> {
    pub model: &'a str,
    pub provider: &'a str,
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub context_window: usize,
    pub memories_injected: usize,
    pub total_memories: usize,
    pub effort: &'a str,
    pub daemon_healthy: bool,
    pub status_bg: Color,
    pub status_fg: Color,
    pub accent: Color,
    pub muted: Color,
    pub success: Color,
    pub error: Color,
    pub warning: Color,
}

impl<'a> Widget for StatusBar<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Top line: model info and token usage
        let token_total = self.input_tokens + self.output_tokens;
        let token_display = format_tokens(token_total);
        let context_display = format_tokens(self.context_window);

        let health_indicator = if self.daemon_healthy {
            Span::styled("●", Style::default().fg(self.success))
        } else {
            Span::styled("●", Style::default().fg(self.error))
        };

        let info_line = Line::from(vec![
            Span::styled(" [Forge] ", Style::default().fg(self.accent)),
            Span::styled(
                format!("{} ({}) ", self.model, self.provider),
                Style::default().fg(self.status_fg),
            ),
            Span::styled(
                format!("{token_display}/{context_display} "),
                Style::default().fg(self.muted),
            ),
            health_indicator,
            Span::styled(" ", Style::default().fg(self.status_fg)),
            if self.effort != "medium" {
                Span::styled(
                    format!("[{}] ", self.effort),
                    Style::default().fg(if self.effort == "high" {
                        self.warning
                    } else {
                        self.muted
                    }),
                )
            } else {
                Span::styled("", Style::default().fg(self.status_fg))
            },
            if self.total_memories > 0 {
                Span::styled(
                    format!(
                        "{} recalled / {} memories",
                        self.memories_injected, self.total_memories
                    ),
                    Style::default().fg(self.status_fg),
                )
            } else {
                Span::styled(
                    format!("{} memories", self.memories_injected),
                    Style::default().fg(self.status_fg),
                )
            },
        ]);

        if area.height >= 1 {
            buf.set_line(area.x, area.y, &info_line, area.width);
            for x in area.x..area.x + area.width {
                buf[(x, area.y)]
                    .set_bg(self.status_bg);
            }
        }

        // Bottom line: key bindings — all text uses theme colors
        if area.height >= 2 {
            let keys_line = Line::from(vec![
                Span::styled(" ^O", Style::default().fg(self.accent)),
                Span::styled(" model ", Style::default().fg(self.status_fg)),
                Span::styled("^K", Style::default().fg(self.accent)),
                Span::styled(" cmd ", Style::default().fg(self.status_fg)),
                Span::styled("^D", Style::default().fg(self.accent)),
                Span::styled(" dashboard ", Style::default().fg(self.status_fg)),
                Span::styled("^G", Style::default().fg(self.accent)),
                Span::styled(" signet ", Style::default().fg(self.status_fg)),
                Span::styled("^C", Style::default().fg(self.accent)),
                Span::styled(" cancel ", Style::default().fg(self.status_fg)),
                Span::styled("^Q", Style::default().fg(self.accent)),
                Span::styled(" quit", Style::default().fg(self.status_fg)),
            ]);

            buf.set_line(area.x, area.y + 1, &keys_line, area.width);
            for x in area.x..area.x + area.width {
                buf[(x, area.y + 1)]
                    .set_bg(self.status_bg);
            }
        }
    }
}

fn format_tokens(n: usize) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        format!("{n}")
    }
}
