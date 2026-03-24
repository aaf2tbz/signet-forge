use forge_core::ToolPermission;

/// Manages tool permissions and approval
pub struct PermissionManager {
    /// Tools that are always auto-approved
    auto_approve: Vec<String>,
    /// Tools approved for this session
    session_approved: Vec<String>,
}

impl PermissionManager {
    pub fn new(auto_approve: Vec<String>) -> Self {
        Self {
            auto_approve,
            session_approved: Vec::new(),
        }
    }

    /// Check if a tool is approved for execution
    pub fn is_approved(&self, tool_name: &str, permission: ToolPermission) -> bool {
        match permission {
            ToolPermission::ReadOnly => true,
            ToolPermission::Write => {
                self.auto_approve.contains(&tool_name.to_string())
                    || self.session_approved.contains(&tool_name.to_string())
            }
            ToolPermission::Dangerous => false,
        }
    }

    /// Approve a tool for the remainder of this session
    pub fn approve_for_session(&mut self, tool_name: &str) {
        if !self.session_approved.contains(&tool_name.to_string()) {
            self.session_approved.push(tool_name.to_string());
        }
    }
}
