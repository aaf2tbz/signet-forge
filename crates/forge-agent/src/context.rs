use forge_core::Message;

/// Manages context window and handles compaction
pub struct ContextManager {
    /// Maximum tokens before triggering compaction
    max_tokens: usize,
    /// Threshold percentage to trigger compaction (0.0 - 1.0)
    compact_threshold: f64,
}

impl ContextManager {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            max_tokens,
            compact_threshold: 0.9,
        }
    }

    /// Check if we should compact the context
    pub fn should_compact(&self, current_tokens: usize) -> bool {
        current_tokens as f64 > self.max_tokens as f64 * self.compact_threshold
    }

    /// Estimate token count for messages (rough heuristic: ~4 chars per token)
    pub fn estimate_tokens(messages: &[Message]) -> usize {
        messages
            .iter()
            .map(|m| m.text().len() / 4)
            .sum()
    }

    pub fn max_tokens(&self) -> usize {
        self.max_tokens
    }
}
