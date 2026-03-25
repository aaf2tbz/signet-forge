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
    pub total_secrets: usize,
    pub secrets_used: usize,
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
    pub spinner: Color,
}

impl<'a> Widget for StatusBar<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let sep = Style::default().fg(self.muted);

        // ─── Line 1: The forge bar ──────────────────────────
        //  ◆ Boogy                        0/200K │ 19/1672 │ ●
        if area.height >= 1 {
            let name = if self.agent_name != "Assistant" {
                self.agent_name
            } else {
                "Forge"
            };

            // Left: identity
            let mut left = vec![
                Span::styled(" ◆ ", Style::default().fg(self.spinner)),
                Span::styled(
                    name,
                    Style::default().fg(self.accent).add_modifier(Modifier::BOLD),
                ),
            ];

            // Model + provider (dimmer)
            left.push(Span::styled("  ", sep));
            left.push(Span::styled(
                self.model,
                Style::default().fg(self.status_fg),
            ));
            left.push(Span::styled(
                format!(" ({})", self.provider),
                Style::default().fg(self.muted),
            ));

            // Effort badge — always visible
            left.push(Span::styled("  ", sep));
            let effort_color = match self.effort {
                "high" => self.warning,
                "low" => self.muted,
                _ => self.accent,
            };
            left.push(Span::styled(
                format!("◈ {}", self.effort),
                Style::default().fg(effort_color),
            ));

            // Agent tag
            if let Some(agent) = self.active_agent {
                left.push(Span::styled("  ", sep));
                left.push(Span::styled(
                    format!("@{agent}"),
                    Style::default().fg(self.accent),
                ));
            }

            buf.set_line(area.x, area.y, &Line::from(left), area.width);

            // Right: gauges — tokens │ memories │ health
            let health = if self.daemon_healthy { "●" } else { "○" };
            let health_color = if self.daemon_healthy { self.success } else { self.error };

            let tokens = format_tokens(self.input_tokens + self.output_tokens);
            let ctx = format_tokens(self.context_window);
            let mem = if self.total_memories > 0 {
                format!("{}/{}", self.memories_injected, self.total_memories)
            } else {
                format!("{}", self.memories_injected)
            };

            let right = vec![
                Span::styled(format!("{tokens}/{ctx}"), Style::default().fg(self.muted)),
                Span::styled(" │ ", sep),
                Span::styled(
                    format!("{}/{} sec", self.secrets_used, self.total_secrets),
                    Style::default().fg(if self.secrets_used > 0 { self.success } else { self.muted }),
                ),
                Span::styled(" │ ", sep),
                Span::styled(format!("{mem} mem"), Style::default().fg(self.status_fg)),
                Span::styled(" │ ", sep),
                Span::styled(health, Style::default().fg(health_color)),
                Span::styled(" ", Style::default()),
            ];

            let right_width: usize = right.iter().map(|s| s.content.len()).sum();
            if (area.width as usize) > right_width {
                let rx = area.x + area.width - right_width as u16;
                buf.set_line(rx, area.y, &Line::from(right), right_width as u16);
            }

            for x in area.x..area.x + area.width {
                buf[(x, area.y)].set_bg(self.status_bg);
            }
        }

        // ─── Line 2: Keybind hints ─────────────────────────
        //  ^O model · ^K cmd · ^D dash · ^R voice · ^Q quit
        if area.height >= 2 {
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
