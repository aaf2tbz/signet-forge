use crate::session::Session;
use forge_core::{Message, MessageContent, TokenUsage, ToolCall};
use forge_provider::{CompletionOpts, Provider, StreamEvent};
use forge_signet::hooks::SessionHooks;
use forge_tools;
use futures::StreamExt;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

/// Events sent from the agent loop to the TUI
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// Streaming text from the assistant
    TextDelta(String),
    /// A tool is being called
    ToolStart { id: String, name: String },
    /// Tool execution result
    ToolResult {
        id: String,
        name: String,
        output: String,
        is_error: bool,
    },
    /// Tool needs permission approval
    ToolApproval {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    /// Token usage update
    Usage(TokenUsage),
    /// Agent turn complete
    TurnComplete,
    /// Error occurred
    Error(String),
    /// Thinking/status message
    Status(String),
}

/// The core agentic loop
pub struct AgentLoop {
    provider: Arc<dyn Provider>,
    hooks: Option<SessionHooks>,
    event_tx: mpsc::Sender<AgentEvent>,
    system_prompt: String,
}

impl AgentLoop {
    pub fn new(
        provider: Arc<dyn Provider>,
        hooks: Option<SessionHooks>,
        event_tx: mpsc::Sender<AgentEvent>,
        system_prompt: String,
    ) -> Self {
        Self {
            provider,
            hooks,
            event_tx,
            system_prompt,
        }
    }

    /// Process a user message through the full agentic loop
    pub async fn process_message(&self, session: &mut Session, user_input: &str) {
        // 1. Call user-prompt-submit hook for memory injection
        let mut memory_context = String::new();
        if let Some(hooks) = &self.hooks {
            match hooks.prompt_submit(user_input).await {
                Ok(injection) if !injection.is_empty() => {
                    debug!("Memory injection: {} bytes", injection.len());
                    memory_context = injection;
                }
                Ok(_) => {}
                Err(e) => {
                    debug!("Prompt hook failed (non-fatal): {e}");
                }
            }
        }

        // 2. Add user message to session
        session.add_message(Message::user(user_input));

        // 3. Run the agentic loop
        loop {
            // Build system prompt with memory context
            let full_system = if memory_context.is_empty() {
                self.system_prompt.clone()
            } else {
                format!("{}\n\n{}", self.system_prompt, memory_context)
            };

            let opts = CompletionOpts {
                system_prompt: Some(full_system),
                max_tokens: Some(8192),
                ..Default::default()
            };

            let tools = forge_tools::all_definitions();

            // 4. Call the provider
            let stream = match self
                .provider
                .complete(&session.messages, &tools, &opts)
                .await
            {
                Ok(s) => s,
                Err(e) => {
                    error!("Provider error: {e}");
                    let _ = self.event_tx.send(AgentEvent::Error(e.to_string())).await;
                    return;
                }
            };

            // 5. Process the stream
            let mut assistant_text = String::new();
            let mut tool_calls: Vec<ToolCall> = Vec::new();
            let mut current_tool_id = String::new();
            let mut current_tool_name = String::new();
            let mut current_tool_input = String::new();

            let mut stream = std::pin::pin!(stream);

            while let Some(event) = stream.next().await {
                match event {
                    StreamEvent::TextDelta(text) => {
                        assistant_text.push_str(&text);
                        let _ = self.event_tx.send(AgentEvent::TextDelta(text)).await;
                    }
                    StreamEvent::ToolUseStart { id, name } => {
                        current_tool_id = id.clone();
                        current_tool_name = name.clone();
                        current_tool_input.clear();
                        let _ = self
                            .event_tx
                            .send(AgentEvent::ToolStart {
                                id: id.clone(),
                                name: name.clone(),
                            })
                            .await;
                    }
                    StreamEvent::ToolUseInput(json) => {
                        current_tool_input.push_str(&json);
                    }
                    StreamEvent::ToolUseEnd => {
                        let input: serde_json::Value =
                            serde_json::from_str(&current_tool_input).unwrap_or_default();
                        tool_calls.push(ToolCall {
                            id: current_tool_id.clone(),
                            name: current_tool_name.clone(),
                            input,
                        });
                    }
                    StreamEvent::Usage(usage) => {
                        session.total_input_tokens += usage.input_tokens;
                        session.total_output_tokens += usage.output_tokens;
                        let _ = self.event_tx.send(AgentEvent::Usage(usage)).await;
                    }
                    StreamEvent::Done => break,
                    StreamEvent::Error(e) => {
                        let _ = self.event_tx.send(AgentEvent::Error(e)).await;
                        return;
                    }
                }
            }

            // 6. Build assistant message with all content blocks
            let mut content = Vec::new();
            if !assistant_text.is_empty() {
                content.push(MessageContent::Text {
                    text: assistant_text,
                });
            }
            for tc in &tool_calls {
                content.push(MessageContent::ToolUse {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                    input: tc.input.clone(),
                });
            }

            let assistant_msg = Message {
                id: uuid::Uuid::new_v4().to_string(),
                role: forge_core::Role::Assistant,
                content,
                model: Some(self.provider.model().to_string()),
                usage: None,
            };
            session.add_message(assistant_msg);

            // 7. If no tool calls, we're done
            if tool_calls.is_empty() {
                let _ = self.event_tx.send(AgentEvent::TurnComplete).await;
                return;
            }

            // 8. Execute tool calls and add results
            let mut tool_results_content = Vec::new();
            for tc in &tool_calls {
                info!("Executing tool: {} (id: {})", tc.name, tc.id);

                let result = if let Some(tool) = forge_tools::find_tool(&tc.name) {
                    tool.execute(tc).await
                } else {
                    forge_core::ToolResult::error(
                        &tc.id,
                        format!("Unknown tool: {}", tc.name),
                    )
                };

                let _ = self
                    .event_tx
                    .send(AgentEvent::ToolResult {
                        id: tc.id.clone(),
                        name: tc.name.clone(),
                        output: result.content.clone(),
                        is_error: result.is_error,
                    })
                    .await;

                tool_results_content.push(MessageContent::ToolResult {
                    tool_use_id: result.tool_use_id,
                    content: result.content,
                    is_error: result.is_error,
                });
            }

            // Add tool results as a user message (Anthropic convention)
            let tool_result_msg = Message {
                id: uuid::Uuid::new_v4().to_string(),
                role: forge_core::Role::User,
                content: tool_results_content,
                model: None,
                usage: None,
            };
            session.add_message(tool_result_msg);

            // Loop back for the next LLM call with tool results
            // Clear memory context for subsequent iterations
            memory_context.clear();
        }
    }
}
