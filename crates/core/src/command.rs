//! Command system for built-in and custom slash commands.

use std::collections::HashMap;

use crate::config::CommandConfig;

/// A command definition.
#[derive(Debug, Clone)]
pub struct Command {
    pub name: String,
    pub description: String,
    pub template: String,
    pub agent: Option<String>,
}

/// Built-in commands.
fn builtin_commands() -> Vec<Command> {
    vec![
        Command {
            name: "init".to_string(),
            description: "Generate an AGENTS.md file for this project".to_string(),
            template: "Analyze this project's codebase and generate an AGENTS.md file that describes the project structure, coding conventions, build/test commands, and important patterns. Write it to the project root.".to_string(),
            agent: None,
        },
        Command {
            name: "review".to_string(),
            description: "Review recent code changes".to_string(),
            template: "Review the recent code changes (git diff HEAD). Look for bugs, security issues, performance problems, and style inconsistencies. Provide specific, actionable feedback. $ARGUMENTS".to_string(),
            agent: Some("plan".to_string()),
        },
    ]
}

/// Command registry.
pub struct CommandRegistry {
    commands: HashMap<String, Command>,
}

impl CommandRegistry {
    /// Create a registry with built-in commands.
    pub fn new() -> Self {
        let mut commands = HashMap::new();
        for cmd in builtin_commands() {
            commands.insert(cmd.name.clone(), cmd);
        }
        Self { commands }
    }

    /// Load from config, merging with builtins.
    pub fn load(config_commands: &Option<HashMap<String, CommandConfig>>) -> Self {
        let mut reg = Self::new();

        if let Some(cmds) = config_commands {
            for (name, cfg) in cmds {
                reg.commands.insert(
                    name.clone(),
                    Command {
                        name: name.clone(),
                        description: cfg.description.clone().unwrap_or_default(),
                        template: cfg.template.clone(),
                        agent: cfg.agent.clone(),
                    },
                );
            }
        }

        reg
    }

    pub fn get(&self, name: &str) -> Option<&Command> {
        self.commands.get(name)
    }

    pub fn list(&self) -> Vec<&Command> {
        let mut cmds: Vec<_> = self.commands.values().collect();
        cmds.sort_by_key(|c| &c.name);
        cmds
    }

    /// Expand a command template with arguments.
    pub fn expand(&self, name: &str, arguments: &str) -> Option<String> {
        let cmd = self.commands.get(name)?;
        let expanded = cmd
            .template
            .replace("$ARGUMENTS", arguments)
            .replace("$1", arguments);
        Some(expanded)
    }
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_commands_exist() {
        let reg = CommandRegistry::new();
        assert!(reg.get("init").is_some());
        assert!(reg.get("review").is_some());
    }

    #[test]
    fn expand_template() {
        let reg = CommandRegistry::new();
        let expanded = reg.expand("review", "src/main.rs").unwrap();
        assert!(expanded.contains("src/main.rs"));
    }
}
