use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

/// A model entry in the picker
#[derive(Debug, Clone)]
pub struct ModelEntry {
    pub provider: String,
    pub model: String,
    pub display_name: String,
    pub context_window: usize,
}

/// State for the model picker overlay
pub struct ModelPicker {
    pub models: Vec<ModelEntry>,
    pub selected: usize,
    pub filter: String,
}

impl ModelPicker {
    pub fn new() -> Self {
        Self {
            models: default_models(),
            selected: 0,
            filter: String::new(),
        }
    }

    pub fn filtered_models(&self) -> Vec<&ModelEntry> {
        if self.filter.is_empty() {
            self.models.iter().collect()
        } else {
            let filter_lower = self.filter.to_lowercase();
            self.models
                .iter()
                .filter(|m| {
                    m.display_name.to_lowercase().contains(&filter_lower)
                        || m.provider.to_lowercase().contains(&filter_lower)
                        || m.model.to_lowercase().contains(&filter_lower)
                })
                .collect()
        }
    }

    pub fn selected_model(&self) -> Option<&ModelEntry> {
        let filtered = self.filtered_models();
        filtered.get(self.selected).copied()
    }

    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn move_down(&mut self) {
        let max = self.filtered_models().len().saturating_sub(1);
        self.selected = (self.selected + 1).min(max);
    }

    pub fn type_char(&mut self, c: char) {
        self.filter.push(c);
        self.selected = 0;
    }

    pub fn backspace(&mut self) {
        self.filter.pop();
        self.selected = 0;
    }

    pub fn draw(&self, frame: &mut Frame) {
        let area = frame.area();
        let width = 56u16.min(area.width.saturating_sub(4));
        let height = 20u16.min(area.height.saturating_sub(4));
        let x = (area.width.saturating_sub(width)) / 2;
        let y = (area.height.saturating_sub(height)) / 2;
        let dialog_area = Rect::new(x, y, width, height);

        frame.render_widget(Clear, dialog_area);

        let filtered = self.filtered_models();
        let mut lines = Vec::new();

        // Search filter
        lines.push(Line::from(vec![
            Span::styled("  Search: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                if self.filter.is_empty() {
                    "type to filter...".to_string()
                } else {
                    self.filter.clone()
                },
                if self.filter.is_empty() {
                    Style::default().fg(Color::DarkGray)
                } else {
                    Style::default().fg(Color::White)
                },
            ),
        ]));
        lines.push(Line::from(""));

        // Model list
        for (i, model) in filtered.iter().enumerate() {
            let is_selected = i == self.selected;
            let style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let provider_style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            let ctx = format_context(model.context_window);

            lines.push(Line::from(vec![
                Span::styled(
                    if is_selected { " > " } else { "   " },
                    style,
                ),
                Span::styled(&model.display_name, style),
                Span::styled(
                    format!("  ({}, {})", model.provider, ctx),
                    provider_style,
                ),
            ]));
        }

        if filtered.is_empty() {
            lines.push(Line::from(Span::styled(
                "   No matching models",
                Style::default().fg(Color::DarkGray),
            )));
        }

        // Footer
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  ↑↓ navigate  Enter select  Esc cancel",
            Style::default().fg(Color::DarkGray),
        )));

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" Select Model (^O) ");

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, dialog_area);
    }
}

fn format_context(tokens: usize) -> String {
    if tokens >= 1_000_000 {
        format!("{}M ctx", tokens / 1_000_000)
    } else if tokens >= 1_000 {
        format!("{}K ctx", tokens / 1_000)
    } else {
        format!("{tokens} ctx")
    }
}

fn default_models() -> Vec<ModelEntry> {
    vec![
        // Anthropic
        ModelEntry {
            provider: "anthropic".into(),
            model: "claude-opus-4-6".into(),
            display_name: "Claude Opus 4.6".into(),
            context_window: 200_000,
        },
        ModelEntry {
            provider: "anthropic".into(),
            model: "claude-sonnet-4-6".into(),
            display_name: "Claude Sonnet 4.6".into(),
            context_window: 200_000,
        },
        ModelEntry {
            provider: "anthropic".into(),
            model: "claude-haiku-4-5-20251001".into(),
            display_name: "Claude Haiku 4.5".into(),
            context_window: 200_000,
        },
        // OpenAI
        ModelEntry {
            provider: "openai".into(),
            model: "gpt-4o".into(),
            display_name: "GPT-4o".into(),
            context_window: 128_000,
        },
        ModelEntry {
            provider: "openai".into(),
            model: "gpt-4o-mini".into(),
            display_name: "GPT-4o Mini".into(),
            context_window: 128_000,
        },
        ModelEntry {
            provider: "openai".into(),
            model: "o4-mini".into(),
            display_name: "o4-mini".into(),
            context_window: 200_000,
        },
        // Google
        ModelEntry {
            provider: "gemini".into(),
            model: "gemini-2.5-flash".into(),
            display_name: "Gemini 2.5 Flash".into(),
            context_window: 1_000_000,
        },
        ModelEntry {
            provider: "gemini".into(),
            model: "gemini-2.5-pro".into(),
            display_name: "Gemini 2.5 Pro".into(),
            context_window: 1_000_000,
        },
        // Groq
        ModelEntry {
            provider: "groq".into(),
            model: "llama-3.3-70b-versatile".into(),
            display_name: "Llama 3.3 70B (Groq)".into(),
            context_window: 128_000,
        },
        // Ollama
        ModelEntry {
            provider: "ollama".into(),
            model: "qwen3:4b".into(),
            display_name: "Qwen3 4B (Local)".into(),
            context_window: 32_768,
        },
        // OpenRouter
        ModelEntry {
            provider: "openrouter".into(),
            model: "anthropic/claude-sonnet-4-6".into(),
            display_name: "Claude Sonnet 4.6 (OpenRouter)".into(),
            context_window: 200_000,
        },
    ]
}
