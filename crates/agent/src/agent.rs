//! Agent definitions and registry.
//!
//! Mirrors `src/agent/agent.ts` from the original OpenCode.
//! Defines the built-in agents: build, plan, general, explore, compaction, title, summary.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// An agent definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDef {
    pub name: String,
    pub description: String,
    /// The default model for this agent (e.g., "anthropic/claude-sonnet-4-20250514").
    pub model: Option<String>,
    /// Whether to use the small model.
    pub small: bool,
    /// Temperature override.
    pub temperature: Option<f64>,
    /// Top-p override.
    pub top_p: Option<f64>,
    /// System prompt for this agent.
    pub system_prompt: String,
    /// Whether this agent is hidden from the UI.
    pub hidden: bool,
    /// Color hint for the UI.
    pub color: Option<String>,
    /// Maximum tool execution steps before stopping.
    pub max_steps: u32,
    /// Tools this agent is allowed to use (empty = all).
    pub allowed_tools: Vec<String>,
    /// Tools this agent is denied.
    pub denied_tools: Vec<String>,
    /// Permission rules specific to this agent.
    pub permission_rules: Vec<PermissionRule>,
}

/// A permission rule for tool access.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRule {
    /// The tool name pattern (supports wildcards).
    pub tool: String,
    /// The argument pattern (supports wildcards).
    #[serde(default)]
    pub pattern: Option<String>,
    /// The action: allow, deny, or ask.
    pub action: PermissionAction,
}

/// What to do when a permission rule matches.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PermissionAction {
    Allow,
    Deny,
    Ask,
}

impl AgentDef {
    /// Whether this agent can use a given tool.
    pub fn can_use_tool(&self, tool_name: &str) -> bool {
        if !self.denied_tools.is_empty() && self.denied_tools.iter().any(|t| t == tool_name) {
            return false;
        }
        if !self.allowed_tools.is_empty() {
            return self.allowed_tools.iter().any(|t| t == tool_name);
        }
        true
    }
}

/// Built-in agent definitions.
pub fn builtin_agents() -> HashMap<String, AgentDef> {
    let mut agents = HashMap::new();

    agents.insert(
        "build".to_string(),
        AgentDef {
            name: "build".to_string(),
            description: "Primary coding agent with full permissions.".to_string(),
            model: None,
            small: false,
            temperature: None,
            top_p: None,
            system_prompt: include_str!("prompts/build.txt").to_string(),
            hidden: false,
            color: Some("blue".to_string()),
            max_steps: 200,
            allowed_tools: vec![],
            denied_tools: vec![],
            permission_rules: vec![
                PermissionRule {
                    tool: "read".to_string(),
                    pattern: None,
                    action: PermissionAction::Allow,
                },
                PermissionRule {
                    tool: "glob".to_string(),
                    pattern: None,
                    action: PermissionAction::Allow,
                },
                PermissionRule {
                    tool: "grep".to_string(),
                    pattern: None,
                    action: PermissionAction::Allow,
                },
                PermissionRule {
                    tool: "question".to_string(),
                    pattern: None,
                    action: PermissionAction::Allow,
                },
            ],
        },
    );

    agents.insert(
        "plan".to_string(),
        AgentDef {
            name: "plan".to_string(),
            description: "Read-only planning agent for analyzing code.".to_string(),
            model: None,
            small: false,
            temperature: None,
            top_p: None,
            system_prompt: include_str!("prompts/plan.txt").to_string(),
            hidden: false,
            color: Some("yellow".to_string()),
            max_steps: 200,
            allowed_tools: vec![
                "read".to_string(),
                "glob".to_string(),
                "grep".to_string(),
                "question".to_string(),
            ],
            denied_tools: vec!["bash".to_string(), "write".to_string(), "edit".to_string()],
            permission_rules: vec![
                PermissionRule {
                    tool: "read".to_string(),
                    pattern: None,
                    action: PermissionAction::Allow,
                },
                PermissionRule {
                    tool: "glob".to_string(),
                    pattern: None,
                    action: PermissionAction::Allow,
                },
                PermissionRule {
                    tool: "grep".to_string(),
                    pattern: None,
                    action: PermissionAction::Allow,
                },
            ],
        },
    );

    agents.insert(
        "general".to_string(),
        AgentDef {
            name: "general".to_string(),
            description: "General-purpose sub-agent for delegated tasks.".to_string(),
            model: None,
            small: false,
            temperature: None,
            top_p: None,
            system_prompt: include_str!("prompts/general.txt").to_string(),
            hidden: false,
            color: Some("green".to_string()),
            max_steps: 100,
            allowed_tools: vec![],
            denied_tools: vec![],
            permission_rules: vec![],
        },
    );

    agents.insert(
        "explore".to_string(),
        AgentDef {
            name: "explore".to_string(),
            description: "Fast codebase exploration agent.".to_string(),
            model: None,
            small: true,
            temperature: None,
            top_p: None,
            system_prompt: include_str!("prompts/explore.txt").to_string(),
            hidden: false,
            color: Some("cyan".to_string()),
            max_steps: 50,
            allowed_tools: vec!["read".to_string(), "glob".to_string(), "grep".to_string()],
            denied_tools: vec!["bash".to_string(), "write".to_string(), "edit".to_string()],
            permission_rules: vec![
                PermissionRule {
                    tool: "read".to_string(),
                    pattern: None,
                    action: PermissionAction::Allow,
                },
                PermissionRule {
                    tool: "glob".to_string(),
                    pattern: None,
                    action: PermissionAction::Allow,
                },
                PermissionRule {
                    tool: "grep".to_string(),
                    pattern: None,
                    action: PermissionAction::Allow,
                },
            ],
        },
    );

    agents.insert(
        "compaction".to_string(),
        AgentDef {
            name: "compaction".to_string(),
            description: "Summarizes conversation for context compaction.".to_string(),
            model: None,
            small: true,
            temperature: Some(0.0),
            top_p: None,
            system_prompt: include_str!("prompts/compaction.txt").to_string(),
            hidden: true,
            color: None,
            max_steps: 1,
            allowed_tools: vec![],
            denied_tools: vec![],
            permission_rules: vec![],
        },
    );

    agents.insert(
        "title".to_string(),
        AgentDef {
            name: "title".to_string(),
            description: "Generates session titles.".to_string(),
            model: None,
            small: true,
            temperature: Some(0.0),
            top_p: None,
            system_prompt: include_str!("prompts/title.txt").to_string(),
            hidden: true,
            color: None,
            max_steps: 1,
            allowed_tools: vec![],
            denied_tools: vec![],
            permission_rules: vec![],
        },
    );

    agents.insert(
        "summary".to_string(),
        AgentDef {
            name: "summary".to_string(),
            description: "Generates conversation summaries.".to_string(),
            model: None,
            small: true,
            temperature: Some(0.0),
            top_p: None,
            system_prompt: include_str!("prompts/summary.txt").to_string(),
            hidden: true,
            color: None,
            max_steps: 1,
            allowed_tools: vec![],
            denied_tools: vec![],
            permission_rules: vec![],
        },
    );

    agents
}

/// Agent registry.
pub struct AgentRegistry {
    agents: HashMap<String, AgentDef>,
}

impl AgentRegistry {
    /// Create a new registry with built-in agents.
    pub fn new() -> Self {
        Self {
            agents: builtin_agents(),
        }
    }

    /// Create a registry with both builtins and custom agents from config.
    pub fn with_config(custom: HashMap<String, opencoder_core::config::AgentConfig>) -> Self {
        let mut agents = builtin_agents();

        for (name, cfg) in custom {
            if let Some(agent) = agents.get_mut(&name) {
                // Merge config into existing agent
                if let Some(model) = cfg.model {
                    agent.model = Some(model);
                }
                if let Some(temp) = cfg.temperature {
                    agent.temperature = Some(temp);
                }
                if let Some(top_p) = cfg.top_p {
                    agent.top_p = Some(top_p);
                }
                if let Some(prompt) = cfg.prompt {
                    agent.system_prompt = prompt;
                }
                if let Some(hidden) = cfg.hidden {
                    agent.hidden = hidden;
                }
                if let Some(color) = cfg.color {
                    agent.color = Some(color);
                }
                if let Some(steps) = cfg.steps {
                    agent.max_steps = steps;
                }
                if let Some(desc) = cfg.description {
                    agent.description = desc;
                }
            }
        }

        Self { agents }
    }

    /// Get an agent by name.
    pub fn get(&self, name: &str) -> Option<&AgentDef> {
        self.agents.get(name)
    }

    /// List all visible agents, sorted with default first.
    pub fn list(&self) -> Vec<&AgentDef> {
        let mut agents: Vec<_> = self.agents.values().filter(|a| !a.hidden).collect();
        agents.sort_by(|a, b| {
            // "build" first, then alphabetical
            if a.name == "build" {
                std::cmp::Ordering::Less
            } else if b.name == "build" {
                std::cmp::Ordering::Greater
            } else {
                a.name.cmp(&b.name)
            }
        });
        agents
    }

    /// List all agents including hidden ones.
    pub fn list_all(&self) -> Vec<&AgentDef> {
        let mut agents: Vec<_> = self.agents.values().collect();
        agents.sort_by_key(|a| &a.name);
        agents
    }

    /// Get the default agent name.
    pub fn default_agent(&self) -> &str {
        "build"
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_agents_exist() {
        let agents = builtin_agents();
        assert!(agents.contains_key("build"));
        assert!(agents.contains_key("plan"));
        assert!(agents.contains_key("general"));
        assert!(agents.contains_key("explore"));
        assert!(agents.contains_key("compaction"));
        assert!(agents.contains_key("title"));
        assert!(agents.contains_key("summary"));
        assert_eq!(agents.len(), 7);
    }

    #[test]
    fn registry_list_visible() {
        let reg = AgentRegistry::new();
        let visible = reg.list();
        // Hidden agents: compaction, title, summary
        assert_eq!(visible.len(), 4);
        assert_eq!(visible[0].name, "build"); // default first
    }

    #[test]
    fn registry_list_all() {
        let reg = AgentRegistry::new();
        let all = reg.list_all();
        assert_eq!(all.len(), 7);
    }

    #[test]
    fn agent_can_use_tool() {
        let agents = builtin_agents();

        let build = &agents["build"];
        assert!(build.can_use_tool("bash"));
        assert!(build.can_use_tool("read"));
        assert!(build.can_use_tool("write"));

        let plan = &agents["plan"];
        assert!(plan.can_use_tool("read"));
        assert!(plan.can_use_tool("glob"));
        assert!(!plan.can_use_tool("bash")); // denied
        assert!(!plan.can_use_tool("write")); // denied
    }

    #[test]
    fn explore_agent_is_read_only() {
        let agents = builtin_agents();
        let explore = &agents["explore"];
        assert!(explore.can_use_tool("read"));
        assert!(explore.can_use_tool("glob"));
        assert!(explore.can_use_tool("grep"));
        assert!(!explore.can_use_tool("bash"));
        assert!(!explore.can_use_tool("write"));
    }

    #[test]
    fn hidden_agents() {
        let agents = builtin_agents();
        assert!(agents["compaction"].hidden);
        assert!(agents["title"].hidden);
        assert!(agents["summary"].hidden);
        assert!(!agents["build"].hidden);
    }
}
