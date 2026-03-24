use crate::client::{HookPayload, SignetClient};
use forge_core::ForgeError;
use tracing::debug;

/// Manages Signet session lifecycle hooks
pub struct SessionHooks {
    client: SignetClient,
    session_id: String,
    project: Option<String>,
}

impl SessionHooks {
    pub fn new(client: SignetClient, session_id: String, project: Option<String>) -> Self {
        Self {
            client,
            session_id,
            project,
        }
    }

    /// Call session-start hook — returns injected memories/context
    pub async fn session_start(&self) -> Result<String, ForgeError> {
        debug!("Calling session-start hook for session {}", self.session_id);

        let payload = HookPayload {
            harness: "forge".to_string(),
            session_id: Some(self.session_id.clone()),
            project: self.project.clone(),
            content: None,
            transcript: None,
        };

        let result = self.client.call_hook("session-start", &payload).await?;

        // The hook returns injected context as stdout or a structured response
        let context = result
            .get("stdout")
            .or_else(|| result.get("context"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        debug!(
            "Session start hook returned {} bytes of context",
            context.len()
        );
        Ok(context)
    }

    /// Call user-prompt-submit hook — returns per-prompt memory injection
    pub async fn prompt_submit(&self, user_message: &str) -> Result<String, ForgeError> {
        debug!("Calling user-prompt-submit hook");

        let payload = HookPayload {
            harness: "forge".to_string(),
            session_id: Some(self.session_id.clone()),
            project: self.project.clone(),
            content: Some(user_message.to_string()),
            transcript: None,
        };

        let result = self
            .client
            .call_hook("user-prompt-submit", &payload)
            .await?;

        let injection = result
            .get("stdout")
            .or_else(|| result.get("injection"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        Ok(injection)
    }

    /// Call pre-compaction hook — called before auto-compacting context
    pub async fn pre_compaction(&self) -> Result<String, ForgeError> {
        debug!("Calling pre-compaction hook");

        let payload = HookPayload {
            harness: "forge".to_string(),
            session_id: Some(self.session_id.clone()),
            project: self.project.clone(),
            content: None,
            transcript: None,
        };

        let result = self
            .client
            .call_hook("pre-compaction", &payload)
            .await?;

        let instructions = result
            .get("stdout")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        Ok(instructions)
    }

    /// Call session-end hook — triggers extraction pipeline
    pub async fn session_end(&self, transcript: &str) -> Result<(), ForgeError> {
        debug!(
            "Calling session-end hook for session {} ({} bytes transcript)",
            self.session_id,
            transcript.len()
        );

        let payload = HookPayload {
            harness: "forge".to_string(),
            session_id: Some(self.session_id.clone()),
            project: self.project.clone(),
            content: None,
            transcript: Some(transcript.to_string()),
        };

        self.client.call_hook("session-end", &payload).await?;
        debug!("Session end hook completed — extraction pipeline triggered");
        Ok(())
    }
}
