//! System prompt construction.
//!
//! Builds the full system prompt for an agent, including environment info,
//! instruction files (CLAUDE.md, AGENTS.md, etc.), and config instructions.

use std::path::Path;

use opencoder_core::config::Config;

/// Instruction file names to search for (in order).
const INSTRUCTION_FILES: &[&str] = &[
    "CLAUDE.md",
    "AGENTS.md",
    "CONTEXT.md",
    ".claude/instructions.md",
    ".opencode/instructions.md",
];

/// Build the complete system prompt for an agent.
///
/// Returns a Vec of system prompt strings that get concatenated into the
/// system message(s) sent to the LLM.
pub fn build(
    agent_system_prompt: &str,
    project_dir: &Path,
    config: &Config,
) -> Vec<String> {
    let mut parts = Vec::new();

    // 1. Base agent system prompt
    parts.push(agent_system_prompt.to_string());

    // 2. Environment block
    parts.push(build_env_block(project_dir));

    // 3. Instruction files (walk up from project_dir)
    for content in find_instruction_files(project_dir) {
        parts.push(content);
    }

    // 4. Custom agent prompts from .opencode/agents/
    let agents_dir = project_dir.join(".opencode").join("agents");
    if agents_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&agents_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "md") {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        if !content.trim().is_empty() {
                            parts.push(format!(
                                "<custom-agent-prompt source=\"{}\">\n{}\n</custom-agent-prompt>",
                                path.display(),
                                content.trim()
                            ));
                        }
                    }
                }
            }
        }
    }

    // 5. Config instructions
    if let Some(instructions) = &config.instructions {
        for instruction in instructions {
            if !instruction.trim().is_empty() {
                parts.push(instruction.clone());
            }
        }
    }

    parts
}

/// Build the environment info block.
fn build_env_block(project_dir: &Path) -> String {
    let platform = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    let cwd = project_dir.display();

    let mut env_parts = vec![
        format!("cwd: {cwd}"),
        format!("platform: {platform}/{arch}"),
        format!("date: {date}"),
    ];

    // Try to get git branch
    if let Ok(output) = std::process::Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(project_dir)
        .output()
    {
        if output.status.success() {
            let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !branch.is_empty() {
                env_parts.push(format!("git branch: {branch}"));
            }
        }
    }

    format!("<env>\n{}\n</env>", env_parts.join("\n"))
}

/// Search for instruction files by walking up from project_dir.
fn find_instruction_files(project_dir: &Path) -> Vec<String> {
    let mut results = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let mut dir = project_dir.to_path_buf();
    loop {
        for filename in INSTRUCTION_FILES {
            let path = dir.join(filename);
            if path.is_file() {
                let canonical = path.to_string_lossy().to_string();
                if seen.contains(&canonical) {
                    continue;
                }
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if !content.trim().is_empty() {
                        results.push(format!(
                            "<instructions source=\"{}\">\n{}\n</instructions>",
                            path.display(),
                            content.trim()
                        ));
                        seen.insert(canonical);
                    }
                }
            }
        }

        if !dir.pop() {
            break;
        }
        // Stop at filesystem root or home directory
        if dir.as_os_str().is_empty() {
            break;
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_basic_prompt() {
        let config = Config::default();
        let parts = build("You are helpful.", Path::new("/tmp"), &config);
        assert!(!parts.is_empty());
        assert_eq!(parts[0], "You are helpful.");
        // Second part should be env block
        assert!(parts[1].contains("<env>"));
        assert!(parts[1].contains("platform:"));
    }

    #[test]
    fn build_with_instructions() {
        let config = Config {
            instructions: Some(vec!["Always use Rust.".to_string()]),
            ..Default::default()
        };
        let parts = build("Agent prompt.", Path::new("/tmp"), &config);
        assert!(parts.iter().any(|p| p.contains("Always use Rust.")));
    }
}
