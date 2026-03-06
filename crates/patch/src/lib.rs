//! Unified diff patch parsing and application.
//!
//! Mirrors `src/patch/` from the original OpenCode.
//! Supports the OpenCode patch format:
//!   *** Begin Patch
//!   *** Add File: path
//!   *** Update File: path
//!   *** Delete File: path
//!   *** End Patch

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A parsed patch containing changes for multiple files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Patch {
    pub files: Vec<FilePatch>,
}

/// A patch for a single file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilePatch {
    pub path: String,
    pub action: PatchAction,
    pub hunks: Vec<Hunk>,
    /// Full content for Add operations, diff content for Update.
    pub content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PatchAction {
    Add,
    Update,
    Delete,
}

/// A hunk within a unified diff.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hunk {
    pub old_start: u32,
    pub old_count: u32,
    pub new_start: u32,
    pub new_count: u32,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DiffLine {
    Context(String),
    Add(String),
    Remove(String),
}

/// Parse the OpenCode patch format.
pub fn parse_patch(text: &str) -> anyhow::Result<Patch> {
    let mut files = Vec::new();
    let mut current_path: Option<String> = None;
    let mut current_action: Option<PatchAction> = None;
    let mut current_content = String::new();
    let mut in_patch = false;

    for line in text.lines() {
        if line.starts_with("*** Begin Patch") {
            in_patch = true;
            continue;
        }
        if line.starts_with("*** End Patch") {
            flush_file(
                &mut files,
                &mut current_path,
                &mut current_action,
                &mut current_content,
            );
            break;
        }
        if !in_patch {
            continue;
        }

        if let Some(path) = line.strip_prefix("*** Add File: ") {
            flush_file(
                &mut files,
                &mut current_path,
                &mut current_action,
                &mut current_content,
            );
            current_path = Some(path.trim().to_string());
            current_action = Some(PatchAction::Add);
        } else if let Some(path) = line.strip_prefix("*** Update File: ") {
            flush_file(
                &mut files,
                &mut current_path,
                &mut current_action,
                &mut current_content,
            );
            current_path = Some(path.trim().to_string());
            current_action = Some(PatchAction::Update);
        } else if let Some(path) = line.strip_prefix("*** Delete File: ") {
            flush_file(
                &mut files,
                &mut current_path,
                &mut current_action,
                &mut current_content,
            );
            current_path = Some(path.trim().to_string());
            current_action = Some(PatchAction::Delete);
        } else {
            current_content.push_str(line);
            current_content.push('\n');
        }
    }

    Ok(Patch { files })
}

fn flush_file(
    files: &mut Vec<FilePatch>,
    path: &mut Option<String>,
    action: &mut Option<PatchAction>,
    content: &mut String,
) {
    if let (Some(p), Some(a)) = (path.take(), action.take()) {
        files.push(FilePatch {
            path: p,
            action: a,
            hunks: Vec::new(),
            content: if content.is_empty() {
                None
            } else {
                Some(std::mem::take(content))
            },
        });
    }
    content.clear();
}

/// Apply a patch to a set of files.
/// Returns a map of file path → new content (None = deleted).
pub fn apply_patch(
    patch: &Patch,
    read_file: impl Fn(&str) -> anyhow::Result<Option<String>>,
) -> anyhow::Result<HashMap<String, Option<String>>> {
    let mut results = HashMap::new();

    for fp in &patch.files {
        match fp.action {
            PatchAction::Add => {
                results.insert(
                    fp.path.clone(),
                    Some(fp.content.clone().unwrap_or_default()),
                );
            }
            PatchAction::Delete => {
                results.insert(fp.path.clone(), None);
            }
            PatchAction::Update => {
                let existing = read_file(&fp.path)?
                    .ok_or_else(|| anyhow::anyhow!("file not found: {}", fp.path))?;
                if let Some(ref patch_content) = fp.content {
                    let new_content = apply_hunks_simple(&existing, patch_content);
                    results.insert(fp.path.clone(), Some(new_content));
                } else {
                    results.insert(fp.path.clone(), Some(existing));
                }
            }
        }
    }

    Ok(results)
}

/// Simple hunk application: parse +/- lines from patch content.
fn apply_hunks_simple(original: &str, patch_content: &str) -> String {
    let orig_lines: Vec<&str> = original.lines().collect();
    let mut result = Vec::new();
    let mut orig_idx = 0;

    for line in patch_content.lines() {
        if let Some(added) = line.strip_prefix('+') {
            result.push(added.to_string());
        } else if line.starts_with('-') {
            orig_idx += 1;
        } else if let Some(ctx) = line.strip_prefix(' ') {
            result.push(ctx.to_string());
            orig_idx += 1;
        } else if line.starts_with("@@") {
            // Hunk header — skip
        } else if orig_idx < orig_lines.len() {
            result.push(orig_lines[orig_idx].to_string());
            orig_idx += 1;
        }
    }

    while orig_idx < orig_lines.len() {
        result.push(orig_lines[orig_idx].to_string());
        orig_idx += 1;
    }

    let mut output = result.join("\n");
    if original.ends_with('\n') && !output.ends_with('\n') {
        output.push('\n');
    }
    output
}

/// Generate a unified diff between two strings.
pub fn diff(old: &str, new: &str, path: &str) -> String {
    use similar::TextDiff;

    let text_diff = TextDiff::from_lines(old, new);
    let mut output = format!("--- a/{path}\n+++ b/{path}\n");

    for hunk in text_diff.unified_diff().context_radius(3).iter_hunks() {
        output.push_str(&format!("{hunk}"));
    }

    output
}

/// Count additions and deletions in a diff.
pub fn diff_stats(diff_text: &str) -> (usize, usize) {
    let mut additions = 0;
    let mut deletions = 0;
    for line in diff_text.lines() {
        if line.starts_with('+') && !line.starts_with("+++") {
            additions += 1;
        } else if line.starts_with('-') && !line.starts_with("---") {
            deletions += 1;
        }
    }
    (additions, deletions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_add_file() {
        let text = "*** Begin Patch\n*** Add File: src/new.rs\nfn main() {\n    println!(\"hello\");\n}\n*** End Patch";
        let patch = parse_patch(text).unwrap();
        assert_eq!(patch.files.len(), 1);
        assert_eq!(patch.files[0].path, "src/new.rs");
        assert_eq!(patch.files[0].action, PatchAction::Add);
        assert!(patch.files[0].content.as_ref().unwrap().contains("fn main"));
    }

    #[test]
    fn parse_delete_file() {
        let text = "*** Begin Patch\n*** Delete File: old.rs\n*** End Patch";
        let patch = parse_patch(text).unwrap();
        assert_eq!(patch.files.len(), 1);
        assert_eq!(patch.files[0].action, PatchAction::Delete);
    }

    #[test]
    fn parse_multiple_files() {
        let text = "*** Begin Patch\n*** Add File: a.rs\ncontent a\n*** Update File: b.rs\n unchanged\n-old\n+new\n*** Delete File: c.rs\n*** End Patch";
        let patch = parse_patch(text).unwrap();
        assert_eq!(patch.files.len(), 3);
        assert_eq!(patch.files[0].action, PatchAction::Add);
        assert_eq!(patch.files[1].action, PatchAction::Update);
        assert_eq!(patch.files[2].action, PatchAction::Delete);
    }

    #[test]
    fn apply_add_delete() {
        let text = "*** Begin Patch\n*** Add File: new.txt\nhello world\n*** Delete File: old.txt\n*** End Patch";
        let patch = parse_patch(text).unwrap();
        let results = apply_patch(&patch, |_| Ok(None)).unwrap();
        assert!(results.get("new.txt").unwrap().is_some());
        assert!(results.get("old.txt").unwrap().is_none());
    }

    #[test]
    fn diff_generation() {
        let old = "line1\nline2\nline3\n";
        let new = "line1\nmodified\nline3\n";
        let d = diff(old, new, "test.txt");
        assert!(d.contains("--- a/test.txt"));
        assert!(d.contains("+++ b/test.txt"));
        assert!(d.contains("-line2"));
        assert!(d.contains("+modified"));
    }

    #[test]
    fn diff_stats_counting() {
        let d = "--- a/f\n+++ b/f\n-old\n+new1\n+new2\n context\n";
        let (add, del) = diff_stats(d);
        assert_eq!(add, 2);
        assert_eq!(del, 1);
    }

    #[test]
    fn apply_update_hunks() {
        let original = "line1\nline2\nline3\n";
        let patch_text = "*** Begin Patch\n*** Update File: test.txt\n line1\n-line2\n+modified\n line3\n*** End Patch";
        let patch = parse_patch(patch_text).unwrap();
        let results = apply_patch(&patch, |_| Ok(Some(original.to_string()))).unwrap();
        let new_content = results.get("test.txt").unwrap().as_ref().unwrap();
        assert!(new_content.contains("modified"));
        assert!(!new_content.contains("line2"));
    }
}
