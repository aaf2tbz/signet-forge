use crate::{CompletionOpts, CompletionStream, Provider, StreamEvent};
use async_trait::async_trait;
use forge_core::{ForgeError, Message, MessageContent, Role, ToolDefinition, TokenUsage};
use futures::StreamExt;
use reqwest::Client;
use serde_json::{json, Value};
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, error};

const GEMINI_API_URL: &str = "https://generativelanguage.googleapis.com/v1beta/models";

pub struct GeminiProvider {
    model: String,
    api_key: String,
    client: Client,
}

impl GeminiProvider {
    pub fn new(model: String, api_key: String) -> Self {
        Self {
            model,
            api_key,
            client: Client::new(),
        }
    }

    fn build_contents(&self, messages: &[Message]) -> Vec<Value> {
        messages
            .iter()
            .filter(|m| m.role != Role::System)
            .map(|m| {
                let role = match m.role {
                    Role::User => "user",
                    Role::Assistant => "model",
                    Role::System => unreachable!(),
                };

                let parts: Vec<Value> = m
                    .content
                    .iter()
                    .filter_map(|c| match c {
                        MessageContent::Text { text } => Some(json!({ "text": text })),
                        MessageContent::ToolUse { name, input, .. } => Some(json!({
                            "functionCall": {
                                "name": name,
                                "args": input,
                            }
                        })),
                        MessageContent::ToolResult {
                            tool_use_id: _,
                            content,
                            ..
                        } => Some(json!({
                            "functionResponse": {
                                "name": "tool",
                                "response": { "result": content },
                            }
                        })),
                    })
                    .collect();

                json!({
                    "role": role,
                    "parts": parts,
                })
            })
            .collect()
    }

    fn build_tools(&self, tools: &[ToolDefinition]) -> Value {
        let declarations: Vec<Value> = tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.input_schema,
                })
            })
            .collect();

        json!([{ "functionDeclarations": declarations }])
    }
}

#[async_trait]
impl Provider for GeminiProvider {
    fn name(&self) -> &str {
        "gemini"
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn context_window(&self) -> usize {
        match self.model.as_str() {
            m if m.contains("pro") => 1_000_000,
            m if m.contains("flash") => 1_000_000,
            _ => 128_000,
        }
    }

    async fn complete(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        opts: &CompletionOpts,
    ) -> Result<CompletionStream, ForgeError> {
        let mut body = json!({
            "contents": self.build_contents(messages),
        });

        // System instruction
        if let Some(system) = &opts.system_prompt {
            body["systemInstruction"] = json!({
                "parts": [{ "text": system }]
            });
        }

        // Generation config
        let mut gen_config = json!({});
        if let Some(max_tokens) = opts.max_tokens {
            gen_config["maxOutputTokens"] = json!(max_tokens);
        }
        if let Some(temp) = opts.temperature {
            gen_config["temperature"] = json!(temp);
        }
        body["generationConfig"] = gen_config;

        if !tools.is_empty() {
            body["tools"] = self.build_tools(tools);
        }

        let url = format!(
            "{}/{}:streamGenerateContent?alt=sse&key={}",
            GEMINI_API_URL, self.model, self.api_key
        );

        debug!("Sending request to Gemini API: model={}", self.model);

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ForgeError::provider(format!("HTTP error: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            return Err(ForgeError::provider(format!(
                "Gemini API error ({status}): {body}"
            )));
        }

        let (tx, rx) = tokio::sync::mpsc::channel::<StreamEvent>(256);
        let mut byte_stream = response.bytes_stream();

        tokio::spawn(async move {
            let mut buffer = String::new();

            while let Some(chunk) = byte_stream.next().await {
                let chunk = match chunk {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx.send(StreamEvent::Error(e.to_string())).await;
                        break;
                    }
                };

                buffer.push_str(&String::from_utf8_lossy(&chunk));

                while let Some(data) = extract_sse_data(&mut buffer) {
                    if let Some(event) = parse_gemini_chunk(&data) {
                        let is_done = matches!(event, StreamEvent::Done);
                        if tx.send(event).await.is_err() {
                            return;
                        }
                        if is_done {
                            return;
                        }
                    }
                }
            }

            let _ = tx.send(StreamEvent::Done).await;
        });

        Ok(Box::pin(ReceiverStream::new(rx)))
    }

    async fn available(&self) -> bool {
        !self.api_key.is_empty()
    }
}

fn extract_sse_data(buffer: &mut String) -> Option<String> {
    let end = buffer.find("\n\n")?;
    let raw = buffer[..end].to_string();
    buffer.drain(..end + 2);

    for line in raw.lines() {
        if let Some(payload) = line.strip_prefix("data: ") {
            return Some(payload.to_string());
        }
    }
    None
}

fn parse_gemini_chunk(data: &str) -> Option<StreamEvent> {
    let chunk: Value = match serde_json::from_str(data) {
        Ok(v) => v,
        Err(e) => {
            error!("Failed to parse Gemini chunk: {e}");
            return None;
        }
    };

    // Check for candidates
    let candidates = chunk.get("candidates")?.as_array()?;
    let candidate = candidates.first()?;
    let content = candidate.get("content")?;
    let parts = content.get("parts")?.as_array()?;

    for part in parts {
        // Text content
        if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
            return Some(StreamEvent::TextDelta(text.to_string()));
        }

        // Function call
        if let Some(fc) = part.get("functionCall") {
            let name = fc.get("name").and_then(|n| n.as_str()).unwrap_or("unknown");
            let args = fc.get("args").cloned().unwrap_or(json!({}));
            return Some(StreamEvent::ToolUseStart {
                id: format!("gemini-{}", uuid::Uuid::new_v4()),
                name: name.to_string(),
            });
        }
    }

    // Check usage
    if let Some(metadata) = chunk.get("usageMetadata") {
        let input = metadata
            .get("promptTokenCount")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let output = metadata
            .get("candidatesTokenCount")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        if input > 0 || output > 0 {
            return Some(StreamEvent::Usage(TokenUsage {
                input_tokens: input,
                output_tokens: output,
                ..Default::default()
            }));
        }
    }

    None
}
