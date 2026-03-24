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
    pub daemon_healthy: bool,
}

impl<'a> Widget for StatusBar<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Top line: model info and token usage
        let token_total = self.input_tokens + self.output_tokens;
        let token_display = format_tokens(token_total);
        let context_display = format_tokens(self.context_window);

        let health_indicator = if self.daemon_healthy {
            Span::styled("●", Style::default().fg(Color::Green))
        } else {
            Span::styled("●", Style::default().fg(Color::Red))
        };

        let info_line = Line::from(vec![
            Span::styled(" [Forge] ", Style::default().fg(Color::Cyan)),
            Span::raw(format!("{} ({}) ", self.model, self.provider)),
            Span::styled(
                format!("{token_display}/{context_display} "),
                Style::default().fg(Color::DarkGray),
            ),
            health_indicator,
            Span::raw(" "),
            if self.total_memories > 0 {
                Span::raw(format!(
                    "{} recalled / {} memories",
                    self.memories_injected, self.total_memories
                ))
            } else {
                Span::raw(format!("{} memories", self.memories_injected))
            },
        ]);

        if area.height >= 1 {
            buf.set_line(area.x, area.y, &info_line, area.width);
            // Fill background
            for x in area.x..area.x + area.width {
                buf[(x, area.y)]
                    .set_bg(Color::Rgb(30, 30, 30));
            }
        }

        // Bottom line: key bindings
        if area.height >= 2 {
            let keys_line = Line::from(vec![
                Span::styled(" ^O", Style::default().fg(Color::Yellow)),
                Span::raw(" model "),
                Span::styled("^K", Style::default().fg(Color::Yellow)),
                Span::raw(" cmd "),
                Span::styled("^D", Style::default().fg(Color::Yellow)),
                Span::raw(" dashboard "),
                Span::styled("^G", Style::default().fg(Color::Yellow)),
                Span::raw(" signet "),
                Span::styled("^C", Style::default().fg(Color::Yellow)),
                Span::raw(" cancel "),
                Span::styled("^Q", Style::default().fg(Color::Yellow)),
                Span::raw(" quit"),
            ]);

            buf.set_line(area.x, area.y + 1, &keys_line, area.width);
            for x in area.x..area.x + area.width {
                buf[(x, area.y + 1)]
                    .set_bg(Color::Rgb(30, 30, 30));
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
