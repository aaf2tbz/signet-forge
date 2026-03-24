pub mod bash;
pub mod edit;
pub mod glob;
pub mod grep;
pub mod read;
pub mod write;

use async_trait::async_trait;
use forge_core::{ToolCall, ToolDefinition, ToolPermission, ToolResult};

/// Trait for built-in tool implementations
#[async_trait]
pub trait Tool: Send + Sync {
    /// Tool name (matches the name in ToolDefinition)
    fn name(&self) -> &str;

    /// Tool definition for the LLM
    fn definition(&self) -> ToolDefinition;

    /// Permission level required
    fn permission(&self) -> ToolPermission;

    /// Execute the tool with the given input
    async fn execute(&self, call: &ToolCall) -> ToolResult;
}

/// Get all built-in tool definitions
pub fn all_definitions() -> Vec<ToolDefinition> {
    all_tools().iter().map(|t| t.definition()).collect()
}

/// Get all built-in tool instances
pub fn all_tools() -> Vec<Box<dyn Tool>> {
    vec![
        Box::new(bash::BashTool),
        Box::new(read::ReadTool),
        Box::new(write::WriteTool),
        Box::new(edit::EditTool),
        Box::new(glob::GlobTool),
        Box::new(grep::GrepTool),
    ]
}

/// Find a tool by name
pub fn find_tool(name: &str) -> Option<Box<dyn Tool>> {
    all_tools().into_iter().find(|t| t.name() == name)
}
