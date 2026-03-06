//! Skill discovery and registry.
//!
//! Skills are Markdown files with optional YAML frontmatter that provide
//! reusable prompts/instructions.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A discovered skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub file_path: PathBuf,
    pub content: String,
}

/// Directories to search for skills.
const SKILL_DIRS: &[&str] = &[
    ".claude/skills",
    ".agents/skills",
    ".opencode/skills",
];

/// Discover skills in the project directory.
pub fn discover(project_dir: &Path) -> Vec<Skill> {
    let mut skills = Vec::new();

    for dir_name in SKILL_DIRS {
        let dir = project_dir.join(dir_name);
        if !dir.is_dir() {
            continue;
        }
        scan_skill_dir(&dir, &mut skills);
    }

    skills
}

fn scan_skill_dir(dir: &Path, skills: &mut Vec<Skill>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_skill_dir(&path, skills);
            continue;
        }
        if path.extension().is_some_and(|ext| ext == "md") {
            if let Some(skill) = parse_skill_file(&path) {
                skills.push(skill);
            }
        }
    }
}

fn parse_skill_file(path: &Path) -> Option<Skill> {
    let content = std::fs::read_to_string(path).ok()?;

    let (name, description, body) = parse_frontmatter(&content);

    let name = name.unwrap_or_else(|| {
        path.file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "unnamed".to_string())
    });

    Some(Skill {
        name,
        description: description.unwrap_or_default(),
        file_path: path.to_path_buf(),
        content: body.to_string(),
    })
}

/// Parse YAML frontmatter from a Markdown file.
/// Returns (name, description, body).
fn parse_frontmatter(content: &str) -> (Option<String>, Option<String>, &str) {
    if !content.starts_with("---") {
        return (None, None, content);
    }

    let rest = &content[3..];
    let Some(end_idx) = rest.find("---") else {
        return (None, None, content);
    };

    let frontmatter = &rest[..end_idx];
    let body = &rest[end_idx + 3..].trim_start();

    let mut name = None;
    let mut description = None;

    for line in frontmatter.lines() {
        let line = line.trim();
        if let Some(val) = line.strip_prefix("name:") {
            name = Some(val.trim().trim_matches('"').to_string());
        } else if let Some(val) = line.strip_prefix("description:") {
            description = Some(val.trim().trim_matches('"').to_string());
        }
    }

    (name, description, body)
}

/// Skill registry.
pub struct SkillRegistry {
    pub skills: Vec<Skill>,
}

impl SkillRegistry {
    pub fn load(project_dir: &Path) -> Self {
        Self {
            skills: discover(project_dir),
        }
    }

    pub fn get(&self, name: &str) -> Option<&Skill> {
        self.skills.iter().find(|s| s.name == name)
    }

    pub fn list(&self) -> &[Skill] {
        &self.skills
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_frontmatter_basic() {
        let content = "---\nname: test\ndescription: a test skill\n---\nBody content here.";
        let (name, desc, body) = parse_frontmatter(content);
        assert_eq!(name.as_deref(), Some("test"));
        assert_eq!(desc.as_deref(), Some("a test skill"));
        assert_eq!(body, "Body content here.");
    }

    #[test]
    fn parse_no_frontmatter() {
        let content = "Just plain markdown.";
        let (name, desc, body) = parse_frontmatter(content);
        assert!(name.is_none());
        assert!(desc.is_none());
        assert_eq!(body, "Just plain markdown.");
    }
}
