use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Widget,
};

/// Top header bar — identity + model + status
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
    pub active_agent: Option<&'a str>,
    pub agent_name: &'a str,
    pub keybinds: &'a crate::keybinds::KeyBindConfig,
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
        // ─── Line 1: Identity + Model ───────────────────────
        // ◆ Forge · claude-opus-4-6 (claude-cli)                    ● 19/1672 memories
        let health = if self.daemon_healthy {
            Span::styled("●", Style::default().fg(self.success))
        } else {
            Span::styled("●", Style::default().fg(self.error))
        };

        let name = if self.agent_name != "Assistant" {
            self.agent_name
        } else {
            "Forge"
        };

        let mut left = vec![
            Span::styled(" ◆ ", Style::default().fg(self.accent)),
            Span::styled(
                name,
                Style::default().fg(self.accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" · ", Style::default().fg(self.muted)),
            Span::styled(
                format!("{} ({})", self.model, self.provider),
                Style::default().fg(self.status_fg),
            ),
        ];

        // Effort indicator (only if non-default)
        if self.effort != "medium" {
            left.push(Span::styled(" · ", Style::default().fg(self.muted)));
            left.push(Span::styled(
                self.effort,
                Style::default().fg(if self.effort == "high" {
                    self.warning
                } else {
                    self.muted
                }),
            ));
        }

        // Agent indicator
        if let Some(agent) = self.active_agent {
            left.push(Span::styled(" · ", Style::default().fg(self.muted)));
            left.push(Span::styled(
                format!("@{agent}"),
                Style::default().fg(self.accent),
            ));
        }

        // Right side: health + memories
        let token_total = self.input_tokens + self.output_tokens;
        let tokens = format_tokens(token_total);
        let ctx = format_tokens(self.context_window);
        let mem_text = if self.total_memories > 0 {
            format!("{}/{} mem", self.memories_injected, self.total_memories)
        } else {
            format!("{} mem", self.memories_injected)
        };
        let right_text = format!("{tokens}/{ctx}  {mem_text} ");
        let right_width = right_text.len() + 3; // health dot + spaces

        // Fill the line
        if area.height >= 1 {
            buf.set_line(area.x, area.y, &Line::from(left), area.width);
            // Right-align health + memory info
            if area.width as usize > right_width {
                let rx = area.x + area.width - right_width as u16;
                let right_line = Line::from(vec![
                    Span::styled(&right_text, Style::default().fg(self.muted)),
                    health,
                    Span::styled(" ", Style::default()),
                ]);
                buf.set_line(rx, area.y, &right_line, right_width as u16);
            }
            for x in area.x..area.x + area.width {
                buf[(x, area.y)].set_bg(self.status_bg);
            }
        }

        // ─── Line 2: Compact keybind hints ──────────────────
        // ? help · ^O model · ^K cmd · ^D dash · ^R voice · ^Q quit
        if area.height >= 2 {
            let sep = Style::default().fg(self.muted);
            let key = Style::default().fg(self.accent);
            let label = Style::default().fg(self.muted);

            let hints: &[(&str, &str)] = &[
                ("model_picker", "model"),
                ("command_palette", "cmd"),
                ("dashboard", "dash"),
                ("signet_commands", "signet"),
                ("voice_input", "voice"),
                ("keybinds", "keys"),
                ("session_browser", "sessions"),
                ("quit", "quit"),
            ];

            let mut spans = vec![Span::styled(" ", sep)];
            let mut used = 1u16;
            for (i, (action, name)) in hints.iter().enumerate() {
                let combo = self.keybinds.get(action);
                let width = combo.len() as u16 + name.len() as u16 + 1;
                if used + width + 3 > area.width {
                    break;
                }
                if i > 0 {
                    spans.push(Span::styled(" · ", sep));
                    used += 3;
                }
                spans.push(Span::styled(combo.to_string(), key));
                spans.push(Span::styled(format!(" {name}"), label));
                used += width;
            }

            buf.set_line(area.x, area.y + 1, &Line::from(spans), area.width);
            for x in area.x..area.x + area.width {
                buf[(x, area.y + 1)].set_bg(self.status_bg);
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
