//! Permission evaluation system.
//!
//! Mirrors `src/permission/next.ts` from the original OpenCode.
//! Uses last-match-wins semantics with wildcard pattern matching.


use opencoder_core::util::wildcard;

use crate::agent::PermissionRule;

pub use crate::agent::PermissionAction;

/// A ruleset is a list of permission rules.
pub type Ruleset = Vec<PermissionRule>;

/// Merge multiple rulesets. Later rulesets have higher priority (appended last).
pub fn merge(rulesets: &[&Ruleset]) -> Ruleset {
    let mut merged = Vec::new();
    for rs in rulesets {
        merged.extend(rs.iter().cloned());
    }
    merged
}

/// Evaluate a permission request against merged rulesets.
/// Uses last-match-wins: iterates in reverse and returns the first matching rule.
/// If no rule matches, returns `Ask` as the default action.
pub fn evaluate(permission: &str, pattern: &str, rulesets: &[&Ruleset]) -> PermissionAction {
    let merged = merge(rulesets);
    // Last match wins — iterate in reverse
    for rule in merged.iter().rev() {
        let perm_matches = wildcard::matches(&rule.tool, permission);
        let pattern_matches = rule
            .pattern
            .as_ref()
            .map(|p| wildcard::matches(p, pattern))
            .unwrap_or(true);

        if perm_matches && pattern_matches {
            return rule.action.clone();
        }
    }
    PermissionAction::Ask
}

/// Get the set of tools that are denied by the given rulesets.
pub fn disabled_tools(tool_names: &[&str], rulesets: &[&Ruleset]) -> Vec<String> {
    let mut disabled = Vec::new();
    for &tool in tool_names {
        let action = evaluate(tool, "*", rulesets);
        if action == PermissionAction::Deny {
            disabled.push(tool.to_string());
        }
    }
    disabled
}

/// Default permission rules for the system.
pub fn default_rules() -> Ruleset {
    vec![
        // Read-only tools are always allowed
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
        // Write tools need permission by default
        PermissionRule {
            tool: "write".to_string(),
            pattern: None,
            action: PermissionAction::Ask,
        },
        PermissionRule {
            tool: "edit".to_string(),
            pattern: None,
            action: PermissionAction::Ask,
        },
        PermissionRule {
            tool: "bash".to_string(),
            pattern: None,
            action: PermissionAction::Ask,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_rule(tool: &str, pattern: Option<&str>, action: PermissionAction) -> PermissionRule {
        PermissionRule {
            tool: tool.to_string(),
            pattern: pattern.map(String::from),
            action,
        }
    }

    #[test]
    fn evaluate_default_ask() {
        let empty: Ruleset = vec![];
        let action = evaluate("unknown_tool", "anything", &[&empty]);
        assert_eq!(action, PermissionAction::Ask);
    }

    #[test]
    fn evaluate_allow_read() {
        let rules = default_rules();
        let action = evaluate("read", "/any/file.rs", &[&rules]);
        assert_eq!(action, PermissionAction::Allow);
    }

    #[test]
    fn evaluate_ask_bash() {
        let rules = default_rules();
        let action = evaluate("bash", "rm -rf /", &[&rules]);
        assert_eq!(action, PermissionAction::Ask);
    }

    #[test]
    fn last_match_wins() {
        let rules = vec![
            make_rule("bash", None, PermissionAction::Ask),
            make_rule("bash", Some("ls *"), PermissionAction::Allow),
        ];
        // "ls /tmp" should match the second rule (Allow)
        let action = evaluate("bash", "ls /tmp", &[&rules]);
        assert_eq!(action, PermissionAction::Allow);

        // "rm /tmp" should match only the first rule (Ask)
        let action = evaluate("bash", "rm /tmp", &[&rules]);
        assert_eq!(action, PermissionAction::Ask);
    }

    #[test]
    fn merge_rulesets() {
        let base = vec![make_rule("bash", None, PermissionAction::Ask)];
        let override_rules = vec![make_rule("bash", None, PermissionAction::Allow)];

        // Override should win (it's appended last)
        let action = evaluate("bash", "anything", &[&base, &override_rules]);
        assert_eq!(action, PermissionAction::Allow);
    }

    #[test]
    fn wildcard_tool_matching() {
        let rules = vec![make_rule("*", None, PermissionAction::Allow)];
        let action = evaluate("bash", "ls", &[&rules]);
        assert_eq!(action, PermissionAction::Allow);
    }

    #[test]
    fn disabled_tools_list() {
        let rules = vec![
            make_rule("read", None, PermissionAction::Allow),
            make_rule("bash", None, PermissionAction::Deny),
            make_rule("write", None, PermissionAction::Deny),
        ];
        let disabled = disabled_tools(&["read", "bash", "write", "glob"], &[&rules]);
        assert_eq!(disabled, vec!["bash".to_string(), "write".to_string()]);
    }
}
