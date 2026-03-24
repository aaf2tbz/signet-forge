use crate::context::ContextManager;
use crate::permissions::{PermissionManager, PermissionRequest, PermissionResponse};
use crate::session::SharedSession;
use forge_core::{Message, MessageContent, TokenUsage, ToolCall};
use forge_provider::{CompletionOpts, Provider, StreamEvent};
use forge_signet::hooks::SessionHooks;
use forge_tools;
use futures::StreamExt;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, error, info, warn};

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
    /// Tool needs permission approval — TUI must respond via PermissionRequest channel
    ToolApproval(String, String, serde_json::Value),
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
    /// Channel for sending permission requests to the TUI
    permission_tx: mpsc::Sender<PermissionRequest>,
    permissions: Arc<Mutex<PermissionManager>>,
    context_manager: ContextManager,
    system_prompt: String,
}

impl AgentLoop {
    pub fn new(
        provider: Arc<dyn Provider>,
        hooks: Option<SessionHooks>,
        event_tx: mpsc::Sender<AgentEvent>,
        permission_tx: mpsc::Sender<PermissionRequest>,
        permissions: Arc<Mutex<PermissionManager>>,
        system_prompt: String,
    ) -> Self {
        let context_window = provider.context_window();
        Self {
            provider,
            hooks,
            event_tx,
            permission_tx,
            permissions,
            context_manager: ContextManager::new(context_window),
            system_prompt,
        }
    }

    /// Process a user message through the full agentic loop
    pub async fn process_message(&self, session: &SharedSession, user_input: &str) {
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
        {
            let mut s = session.lock().await;
            s.add_message(Message::user(user_input));
        }

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

            // Get current messages snapshot
            let messages = {
                let s = session.lock().await;
                s.messages.clone()
            };

            // 4. Call the provider
            let stream = match self.provider.complete(&messages, &tools, &opts).await {
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
                        {
                            let mut s = session.lock().await;
                            s.total_input_tokens += usage.input_tokens;
                            s.total_output_tokens += usage.output_tokens;
                        }
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
            {
                let mut s = session.lock().await;
                s.add_message(assistant_msg);
            }

            // 7. If no tool calls, we're done — check compaction first
            if tool_calls.is_empty() {
                // Check if context needs compaction
                let estimated_tokens = {
                    let s = session.lock().await;
                    ContextManager::estimate_tokens(&s.messages)
                };
                if self.context_manager.should_compact(estimated_tokens) {
                    let _ = self
                        .event_tx
                        .send(AgentEvent::Status("Compacting context...".to_string()))
                        .await;
                    if let Err(e) = self
                        .context_manager
                        .compact(session, &self.provider, self.hooks.as_ref())
                        .await
                    {
                        warn!("Context compaction failed: {e}");
                    }
                }
                let _ = self.event_tx.send(AgentEvent::TurnComplete).await;
                return;
            }

            // 8. Execute tool calls with permission checks
            let mut tool_results_content = Vec::new();
            for tc in &tool_calls {
                // Check permissions
                let tool_impl = forge_tools::find_tool(&tc.name);
                let permission_level = tool_impl
                    .as_ref()
                    .map(|t| t.permission())
                    .unwrap_or(forge_core::ToolPermission::Write);

                let approved = {
                    let perms = self.permissions.lock().await;
                    perms.is_auto_approved(&tc.name, permission_level)
                };

                if !approved {
                    // Request permission from the TUI
                    let _ = self
                        .event_tx
                        .send(AgentEvent::ToolApproval(
                            tc.id.clone(),
                            tc.name.clone(),
                            tc.input.clone(),
                        ))
                        .await;

                    let (response_tx, response_rx) = tokio::sync::oneshot::channel();
                    let _ = self
                        .permission_tx
                        .send(PermissionRequest {
                            tool_name: tc.name.clone(),
                            tool_input: tc.input.clone(),
                            response_tx,
                        })
                        .await;

                    // Wait for user response
                    let response = match response_rx.await {
                        Ok(r) => r,
                        Err(_) => {
                            // Channel closed — treat as deny
                            PermissionResponse::Deny
                        }
                    };

                    match response {
                        PermissionResponse::Allow => {
                            // Proceed with execution
                        }
                        PermissionResponse::AlwaysAllow => {
                            let mut perms = self.permissions.lock().await;
                            perms.approve_for_session(&tc.name);
                        }
                        PermissionResponse::Deny => {
                            let result = forge_core::ToolResult::error(
                                &tc.id,
                                "Permission denied by user",
                            );
                            let _ = self
                                .event_tx
                                .send(AgentEvent::ToolResult {
                                    id: tc.id.clone(),
                                    name: tc.name.clone(),
                                    output: result.content.clone(),
                                    is_error: true,
                                })
                                .await;
                            tool_results_content.push(MessageContent::ToolResult {
                                tool_use_id: result.tool_use_id,
                                content: result.content,
                                is_error: result.is_error,
                            });
                            continue;
                        }
                    }
                }

                info!("Executing tool: {} (id: {})", tc.name, tc.id);

                let result = if let Some(tool) = tool_impl {
                    tool.execute(tc).await
                } else {
                    forge_core::ToolResult::error(&tc.id, format!("Unknown tool: {}", tc.name))
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
            {
                let mut s = session.lock().await;
                s.add_message(tool_result_msg);
            }

            // Loop back for the next LLM call with tool results
            memory_context.clear();
        }
    }
}
