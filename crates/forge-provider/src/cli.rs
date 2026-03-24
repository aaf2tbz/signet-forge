use crate::{CompletionOpts, CompletionStream, Provider, StreamEvent};
use async_trait::async_trait;
use forge_core::{ForgeError, Message, MessageContent, Role, ToolDefinition, TokenUsage};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
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
    fn build_args(&self, prompt: &str, effort: crate::ReasoningEffort) -> Vec<String> {
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
                // Pass reasoning effort to Claude CLI
                if effort != crate::ReasoningEffort::Medium {
                    args.push("--reasoning-effort".to_string());
                    args.push(effort.as_str().to_string());
                }
                args
            }
            CliKind::Codex => {
                let mut args = vec![
                    "exec".to_string(),
                    "--json".to_string(),
                    "--sandbox".to_string(),
                    "read-only".to_string(),
                ];
                if !self.model.is_empty() {
                    args.push("--model".to_string());
                    args.push(self.model.clone());
                }
                // Codex supports --reasoning-effort for o-series models
                if effort != crate::ReasoningEffort::Medium {
                    args.push("--reasoning-effort".to_string());
                    args.push(effort.as_str().to_string());
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
        let args = self.build_args(&prompt, opts.effort);

        debug!(
            "Spawning CLI: {} {}",
            self.cli_path,
            args.join(" ").chars().take(200).collect::<String>()
        );

        let mut child = Command::new(&self.cli_path)
            .args(&args)
            .env("SIGNET_NO_HOOKS", "1") // Prevent recursive Signet hooks
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| ForgeError::provider(format!("Failed to spawn {}: {e}", self.cli_path)))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| ForgeError::provider("No stdout from CLI process"))?;

        let stderr = child.stderr.take();

        let (tx, rx) = tokio::sync::mpsc::channel::<StreamEvent>(256);
        let cli_kind = self.cli_kind;

        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();

            match cli_kind {
                CliKind::Claude => {
                    // Parse Claude's stream-json JSONL output
                    while let Ok(Some(line)) = lines.next_line().await {
                        if line.trim().is_empty() {
                            continue;
                        }
                        let parsed: serde_json::Value = match serde_json::from_str(&line) {
                            Ok(v) => v,
                            Err(_) => {
                                // Not JSON — emit as raw text
                                if tx
                                    .send(StreamEvent::TextDelta(format!("{line}\n")))
                                    .await
                                    .is_err()
                                {
                                    return;
                                }
                                continue;
                            }
                        };

                        let event_type = parsed
                            .get("type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");

                        match event_type {
                            "assistant" => {
                                // Extract text from assistant message
                                if let Some(msg) = parsed.get("message") {
                                    let subtype = msg
                                        .get("type")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("");
                                    if subtype == "text" {
                                        if let Some(text) =
                                            msg.get("text").and_then(|v| v.as_str())
                                        {
                                            if tx
                                                .send(StreamEvent::TextDelta(text.to_string()))
                                                .await
                                                .is_err()
                                            {
                                                return;
                                            }
                                        }
                                    }
                                }
                            }
                            "content_block_delta" => {
                                // Streaming text delta
                                if let Some(delta) = parsed.get("delta") {
                                    if let Some(text) =
                                        delta.get("text").and_then(|v| v.as_str())
                                    {
                                        if tx
                                            .send(StreamEvent::TextDelta(text.to_string()))
                                            .await
                                            .is_err()
                                        {
                                            return;
                                        }
                                    }
                                }
                            }
                            "result" => {
                                // Final result — extract any remaining text
                                if let Some(result) =
                                    parsed.get("result").and_then(|v| v.as_str())
                                {
                                    if !result.is_empty() {
                                        let _ = tx
                                            .send(StreamEvent::TextDelta(result.to_string()))
                                            .await;
                                    }
                                }
                                // Extract usage if present
                                if let Some(cost) = parsed.get("cost_usd") {
                                    debug!("CLI cost: {:?}", cost);
                                }
                                break;
                            }
                            _ => {
                                // Other event types (tool_use, tool_result, etc.)
                                // are handled by the CLI internally — we just show status
                                debug!("CLI event: {event_type}");
                            }
                        }
                    }
                }
                CliKind::Codex => {
                    // Parse Codex JSONL output
                    while let Ok(Some(line)) = lines.next_line().await {
                        if line.trim().is_empty() {
                            continue;
                        }
                        let parsed: serde_json::Value = match serde_json::from_str(&line) {
                            Ok(v) => v,
                            Err(_) => {
                                let _ = tx
                                    .send(StreamEvent::TextDelta(format!("{line}\n")))
                                    .await;
                                continue;
                            }
                        };

                        let event_type = parsed
                            .get("type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");

                        match event_type {
                            "item.completed" => {
                                // Extract text from completed item
                                if let Some(text) = parsed
                                    .get("item")
                                    .and_then(|i| i.get("text"))
                                    .and_then(|t| t.as_str())
                                {
                                    if tx
                                        .send(StreamEvent::TextDelta(text.to_string()))
                                        .await
                                        .is_err()
                                    {
                                        return;
                                    }
                                }
                            }
                            "turn.completed" => {
                                // Extract usage
                                if let Some(usage) = parsed.get("usage") {
                                    let input = usage
                                        .get("input_tokens")
                                        .and_then(|v| v.as_u64())
                                        .unwrap_or(0)
                                        as usize;
                                    let output = usage
                                        .get("output_tokens")
                                        .and_then(|v| v.as_u64())
                                        .unwrap_or(0)
                                        as usize;
                                    let _ = tx
                                        .send(StreamEvent::Usage(TokenUsage {
                                            input_tokens: input,
                                            output_tokens: output,
                                            ..Default::default()
                                        }))
                                        .await;
                                }
                                break;
                            }
                            _ => {
                                debug!("Codex event: {event_type}");
                            }
                        }
                    }
                }
                CliKind::Gemini => {
                    // Gemini CLI — treat as plain text output
                    while let Ok(Some(line)) = lines.next_line().await {
                        if tx
                            .send(StreamEvent::TextDelta(format!("{line}\n")))
                            .await
                            .is_err()
                        {
                            return;
                        }
                    }
                }
            }

            // Wait for the process to finish and capture stderr on failure
            match child.wait().await {
                Ok(status) => {
                    if !status.success() {
                        let code = status.code().unwrap_or(-1);
                        let mut stderr_text = String::new();
                        if let Some(se) = stderr {
                            let mut se_reader = BufReader::new(se);
                            let mut se_lines = se_reader.lines();
                            while let Ok(Some(line)) = se_lines.next_line().await {
                                stderr_text.push_str(&line);
                                stderr_text.push('\n');
                            }
                        }
                        let err_msg = if stderr_text.trim().is_empty() {
                            format!("CLI exited with code {code}")
                        } else {
                            format!("CLI exited with code {code}: {}", stderr_text.trim())
                        };
                        warn!("{err_msg}");
                        let _ = tx.send(StreamEvent::Error(err_msg)).await;
                    }
                }
                Err(e) => {
                    let _ = tx
                        .send(StreamEvent::Error(format!("CLI process error: {e}")))
                        .await;
                }
            }

            let _ = tx.send(StreamEvent::Done).await;
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
