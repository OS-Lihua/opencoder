//! Tool registry for looking up tools by ID.
//!
//! Mirrors `src/tool/registry.ts` from the original OpenCode.

use std::collections::HashMap;
use std::sync::Arc;

use crate::tool::Tool;

/// Registry that holds all available tools.
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool. Replaces any existing tool with the same ID.
    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.id().to_string(), tool);
    }

    /// Look up a tool by ID.
    pub fn get(&self, id: &str) -> Option<&Arc<dyn Tool>> {
        self.tools.get(id)
    }

    /// List all registered tool IDs.
    pub fn list(&self) -> Vec<&str> {
        self.tools.keys().map(|s| s.as_str()).collect()
    }

    /// Get all tools as a map (for passing to LLM).
    pub fn all(&self) -> &HashMap<String, Arc<dyn Tool>> {
        &self.tools
    }

    /// Number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Create a registry with all built-in tools.
    pub fn with_builtins() -> Self {
        let mut reg = Self::new();
        reg.register(Arc::new(crate::tools::bash::BashTool));
        reg.register(Arc::new(crate::tools::read::ReadTool));
        reg.register(Arc::new(crate::tools::glob::GlobTool));
        reg.register(Arc::new(crate::tools::grep::GrepTool));
        reg.register(Arc::new(crate::tools::write::WriteTool));
        reg.register(Arc::new(crate::tools::edit::EditTool));
        reg.register(Arc::new(crate::tools::apply_patch::ApplyPatchTool));
        reg.register(Arc::new(crate::tools::question::QuestionTool));
        reg.register(Arc::new(crate::tools::webfetch::WebFetchTool));
        reg.register(Arc::new(crate::tools::todo::TodoWriteTool));
        reg.register(Arc::new(crate::tools::todo::TodoReadTool));
        reg.register(Arc::new(crate::tools::multiedit::MultiEditTool));
        reg.register(Arc::new(crate::tools::task::TaskTool));
        reg
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_registry() {
        let reg = ToolRegistry::with_builtins();
        assert!(reg.get("bash").is_some());
        assert!(reg.get("read").is_some());
        assert!(reg.get("glob").is_some());
        assert!(reg.get("grep").is_some());
        assert!(reg.get("write").is_some());
        assert!(reg.get("edit").is_some());
        assert!(reg.get("nonexistent").is_none());
        assert_eq!(reg.len(), 13);
    }
}
