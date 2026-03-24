use crate::{CompletionOpts, CompletionStream, Provider, StreamEvent};
use async_trait::async_trait;
use forge_core::{ForgeError, Message, MessageContent, Role, ToolDefinition, TokenUsage};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, warn};

/// A provider that delegates to an installed CLI tool (claude, codex, gemini, etc.)
/// The CLI handles its own authentication, tool execution, and agent loop.
/// Forge streams the output to the TUI and handles Signet integration.
pub struct CliProvider {
    cli_kind: CliKind,
    cli_path: String,
    model: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CliKind {
    Claude,
    Codex,
    Gemini,
}

impl CliProvider {
    pub fn new(kind: CliKind, cli_path: String, model: String) -> Self {
        Self {
            cli_kind: kind,
            cli_path,
            model,
        }
    }

    /// Build the command arguments for each CLI tool
    fn build_args(&self, prompt: &str, opts: &crate::CompletionOpts) -> Vec<String> {
        match self.cli_kind {
            CliKind::Claude => {
                let mut args = vec![
                    "-p".to_string(),
                    prompt.to_string(),
                    "--output-format".to_string(),
                    "stream-json".to_string(),
                    "--verbose".to_string(),
                ];
                if !self.model.is_empty() {
                    args.push("--model".to_string());
                    args.push(self.model.clone());
                }
                if opts.bypass {
                    args.push("--dangerously-skip-permissions".to_string());
                }
                args
            }
            CliKind::Codex => {
                let mut args = vec![
                    "exec".to_string(),
                    "--json".to_string(),
                ];
                if opts.bypass {
                    args.push("--dangerously-bypass-approvals-and-sandbox".to_string());
                } else {
                    args.push("--sandbox".to_string());
                    args.push("read-only".to_string());
                }
                if !self.model.is_empty() {
                    args.push("--model".to_string());
                    args.push(self.model.clone());
                }
                if opts.effort != crate::ReasoningEffort::Medium {
                    args.push("--reasoning-effort".to_string());
                    args.push(opts.effort.as_str().to_string());
                }
                args.push(prompt.to_string());
                args
            }
            CliKind::Gemini => {
                let mut args = vec!["-p".to_string(), prompt.to_string()];
                if !self.model.is_empty() {
                    args.push("--model".to_string());
                    args.push(self.model.clone());
                }
                // Gemini has no permission bypass — sandbox is off by default
                args
            }
        }
    }

    /// Build a single prompt string from the message history
    fn build_prompt(messages: &[Message], opts: &CompletionOpts) -> String {
        let mut parts = Vec::new();

        // System prompt
        if let Some(system) = &opts.system_prompt {
            parts.push(format!("<system>\n{system}\n</system>"));
        }

        // Conversation history (skip the last user message, we'll add it separately)
        let history_msgs: Vec<&Message> = messages
            .iter()
            .filter(|m| m.role != Role::System)
            .collect();

        if history_msgs.len() > 1 {
            parts.push("<conversation_history>".to_string());
            for msg in &history_msgs[..history_msgs.len() - 1] {
                let role_label = match msg.role {
                    Role::User => "User",
                    Role::Assistant => "Assistant",
                    Role::System => continue,
                };
                for content in &msg.content {
                    match content {
                        MessageContent::Text { text } => {
                            parts.push(format!("{role_label}: {text}"));
                        }
                        MessageContent::ToolUse { name, input, .. } => {
                            parts.push(format!(
                                "Assistant [tool: {name}]: {}",
                                serde_json::to_string(input).unwrap_or_default()
                            ));
                        }
                        MessageContent::ToolResult { content, .. } => {
                            let truncated = if content.len() > 2000 {
                                format!("{}... (truncated)", &content[..2000])
                            } else {
                                content.clone()
                            };
                            parts.push(format!("Tool result: {truncated}"));
                        }
                    }
                }
            }
            parts.push("</conversation_history>".to_string());
        }

        // Latest user message
        if let Some(last) = history_msgs.last() {
            for content in &last.content {
                if let MessageContent::Text { text } = content {
                    parts.push(text.clone());
                }
            }
        }

        parts.join("\n\n")
    }
}

#[async_trait]
impl Provider for CliProvider {
    fn name(&self) -> &str {
        match self.cli_kind {
            CliKind::Claude => "claude-cli",
            CliKind::Codex => "codex-cli",
            CliKind::Gemini => "gemini-cli",
        }
    }

    fn model(&self) -> &str {
        if self.model.is_empty() {
            match self.cli_kind {
                CliKind::Claude => "claude-sonnet-4-6",
                CliKind::Codex => "codex",
                CliKind::Gemini => "gemini-2.5-flash",
            }
        } else {
            &self.model
        }
    }

    fn context_window(&self) -> usize {
        match self.cli_kind {
            CliKind::Claude => 200_000,
            CliKind::Codex => 200_000,
            CliKind::Gemini => 1_000_000,
        }
    }

    async fn complete(
        &self,
        messages: &[Message],
        _tools: &[ToolDefinition],
        opts: &CompletionOpts,
    ) -> Result<CompletionStream, ForgeError> {
        let prompt = Self::build_prompt(messages, opts);
        let args = self.build_args(&prompt, opts);

        debug!(
            "Spawning CLI via PTY: {} {}",
            self.cli_path,
            args.join(" ").chars().take(200).collect::<String>()
        );

        // Use a PTY so the CLI gets line-buffered stdout (thinks it's a terminal).
        // Without this, piped stdout is fully-buffered and output arrives in large
        // chunks or only after the process exits — no live streaming.
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize { rows: 24, cols: 200, pixel_width: 0, pixel_height: 0 })
            .map_err(|e| ForgeError::provider(format!("Failed to open PTY: {e}")))?;

        let mut cmd = CommandBuilder::new(&self.cli_path);
        cmd.args(&args);
        cmd.env("SIGNET_NO_HOOKS", "1");
        // PTY gives us line-buffered output (the real win), but we parse JSON
        // so suppress colors/interactive features that break JSON parsing
        cmd.env("TERM", "dumb");
        cmd.env("NO_COLOR", "1");
        cmd.env("LANG", "en_US.UTF-8");

        let mut child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| ForgeError::provider(format!("Failed to spawn {}: {e}", self.cli_path)))?;

        // Drop the slave side — we only read from the master
        drop(pair.slave);

        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| ForgeError::provider(format!("Failed to clone PTY reader: {e}")))?;

        let (tx, rx) = tokio::sync::mpsc::channel::<StreamEvent>(256);
        let cli_kind = self.cli_kind;

        // Read PTY output on a blocking thread with large buffer (64KB like WindowedClaude)
        // to handle burst output from agent/bash tool runs without backpressure.
        tokio::task::spawn_blocking(move || {
            use std::io::Read;
            let mut raw = reader;
            let mut buf = vec![0u8; 64 * 1024];
            let mut leftover = String::new();

            // Macro to send events from the blocking thread
            macro_rules! send {
                ($event:expr) => {
                    if tx.blocking_send($event).is_err() { return; }
                };
            }

            // Strip ANSI escape sequences for JSON parsing
            fn strip_ansi(s: &str) -> String {
                let mut out = String::with_capacity(s.len());
                let mut chars = s.chars();
                while let Some(c) = chars.next() {
                    if c == '\x1b' {
                        // Skip ESC [ ... (letter) sequences
                        if let Some(next) = chars.next() {
                            if next == '[' {
                                for inner in chars.by_ref() {
                                    if inner.is_ascii_alphabetic() || inner == 'm' || inner == 'K' || inner == 'H' || inner == 'J' {
                                        break;
                                    }
                                }
                            }
                            // Also skip ESC ] ... BEL/ST (OSC sequences)
                        }
                    } else if c == '\r' {
                        // Skip carriage returns from PTY
                        continue;
                    } else {
                        out.push(c);
                    }
                }
                out
            }

            // Read in large chunks, split into lines, process each
            'outer: loop {
                let n = match raw.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                };

                let chunk = String::from_utf8_lossy(&buf[..n]);
                leftover.push_str(&chunk);

                while let Some(nl) = leftover.find('\n') {
                    let line = strip_ansi(leftover[..nl].trim());
                    leftover.drain(..nl + 1);

                    if line.is_empty() {
                        continue;
                    }

                match cli_kind {
                    CliKind::Claude => {
                        let parsed: serde_json::Value = match serde_json::from_str(&line) {
                            Ok(v) => v,
                            Err(_) => {
                                send!(StreamEvent::TextDelta(format!("{line}\n")));
                                continue;
                            }
                        };

                        let event_type = parsed
                            .get("type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");

                        match event_type {
                            "assistant" => {
                                if let Some(msg) = parsed.get("message") {
                                    let subtype = msg
                                        .get("type")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("");
                                    if subtype == "text" {
                                        if let Some(text) = msg.get("text").and_then(|v| v.as_str()) {
                                            send!(StreamEvent::TextDelta(text.to_string()));
                                        }
                                    }
                                }
                            }
                            "content_block_delta" => {
                                if let Some(delta) = parsed.get("delta") {
                                    if let Some(text) = delta.get("text").and_then(|v| v.as_str()) {
                                        send!(StreamEvent::TextDelta(text.to_string()));
                                    }
                                }
                            }
                            "result" => {
                                if let Some(result) = parsed.get("result").and_then(|v| v.as_str()) {
                                    if !result.is_empty() {
                                        let _ = tx.blocking_send(StreamEvent::TextDelta(result.to_string()));
                                    }
                                }
                                break 'outer;
                            }
                            _ => {}
                        }
                    }
                    CliKind::Codex => {
                        let parsed: serde_json::Value = match serde_json::from_str(&line) {
                            Ok(v) => v,
                            Err(_) => {
                                let _ = tx.blocking_send(StreamEvent::TextDelta(format!("{line}\n")));
                                continue;
                            }
                        };

                        let event_type = parsed
                            .get("type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");

                        match event_type {
                            "item.completed" => {
                                if let Some(text) = parsed
                                    .get("item")
                                    .and_then(|i| i.get("text"))
                                    .and_then(|t| t.as_str())
                                {
                                    send!(StreamEvent::TextDelta(text.to_string()));
                                }
                            }
                            "turn.completed" => {
                                if let Some(usage) = parsed.get("usage") {
                                    let input = usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                                    let output = usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                                    let _ = tx.blocking_send(StreamEvent::Usage(TokenUsage {
                                        input_tokens: input,
                                        output_tokens: output,
                                        ..Default::default()
                                    }));
                                }
                                break 'outer;
                            }
                            _ => {}
                        }
                    }
                    CliKind::Gemini => {
                        send!(StreamEvent::TextDelta(format!("{line}\n")));
                    }
                }
                } // end while let Some(nl) — line splitting
            } // end 'outer loop — chunk reading

            // Wait for the process to finish
            match child.wait() {
                Ok(status) => {
                    if !status.success() {
                        let code = status.exit_code();
                        let err_msg = format!("CLI exited with code {code}");
                        warn!("{err_msg}");
                        let _ = tx.blocking_send(StreamEvent::Error(err_msg));
                    }
                }
                Err(e) => {
                    let _ = tx.blocking_send(StreamEvent::Error(format!("CLI process error: {e}")));
                }
            }

            let _ = tx.blocking_send(StreamEvent::Done);
        });

        Ok(Box::pin(ReceiverStream::new(rx)))
    }

    async fn available(&self) -> bool {
        // Check if the CLI binary exists and is executable
        tokio::process::Command::new(&self.cli_path)
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

/// Detect installed CLI tools and return their paths
pub async fn detect_cli_tools() -> Vec<(CliKind, String)> {
    let checks = [
        (CliKind::Claude, "claude"),
        (CliKind::Codex, "codex"),
        (CliKind::Gemini, "gemini"),
    ];

    let mut found = Vec::new();

    for (kind, name) in &checks {
        if let Ok(output) = tokio::process::Command::new("which")
            .arg(name)
            .output()
            .await
        {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path.is_empty() {
                    debug!("Found CLI tool: {name} at {path}");
                    found.push((*kind, path));
                }
            }
        }
    }

    found
}

/// Default model for each CLI tool
pub fn default_model_for_cli(kind: CliKind) -> &'static str {
    match kind {
        CliKind::Claude => "claude-sonnet-4-6",
        CliKind::Codex => "codex",
        CliKind::Gemini => "gemini-2.5-flash",
    }
}

/// Display name for each CLI kind
pub fn cli_display_name(kind: CliKind) -> &'static str {
    match kind {
        CliKind::Claude => "Claude Code",
        CliKind::Codex => "Codex",
        CliKind::Gemini => "Gemini CLI",
    }
}
