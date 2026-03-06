//! Edit tool: make targeted text replacements in files.
//!
//! Mirrors `src/tool/edit.ts` from the original OpenCode.
//! Supports exact string replacement with multiple fallback strategies.

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::tool::{Tool, ToolContext, ToolOutput};

pub struct EditTool;

#[derive(Deserialize)]
struct Params {
    file_path: String,
    old_string: String,
    new_string: String,
    #[serde(default)]
    replace_all: bool,
}

#[async_trait::async_trait]
impl Tool for EditTool {
    fn id(&self) -> &str {
        "edit"
    }

    fn description(&self) -> &str {
        "Make targeted text replacements in files. Specify the exact text to find (old_string) and what to replace it with (new_string). The old_string must uniquely match a section of the file unless replace_all is true."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Absolute path to the file to edit"
                },
                "old_string": {
                    "type": "string",
                    "description": "The exact text to find and replace"
                },
                "new_string": {
                    "type": "string",
                    "description": "The replacement text"
                },
                "replace_all": {
                    "type": "boolean",
                    "description": "Replace all occurrences (default: false)",
                    "default": false
                }
            },
            "required": ["file_path", "old_string", "new_string"]
        })
    }

    async fn execute(&self, params: serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let params: Params = serde_json::from_value(params)?;
        let path = std::path::Path::new(&params.file_path);

        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", params.file_path))?;

        let (new_content, count) = if params.replace_all {
            let count = content.matches(&params.old_string).count();
            (
                content.replace(&params.old_string, &params.new_string),
                count,
            )
        } else {
            // Try exact match first
            match try_replace(&content, &params.old_string, &params.new_string) {
                Some(result) => (result, 1),
                None => {
                    // Fallback: line-trimmed matching
                    match try_replace_trimmed(&content, &params.old_string, &params.new_string) {
                        Some(result) => (result, 1),
                        None => {
                            // Fallback: whitespace-normalized matching
                            match try_replace_normalized(
                                &content,
                                &params.old_string,
                                &params.new_string,
                            ) {
                                Some(result) => (result, 1),
                                None => {
                                    let occurrences = content.matches(&params.old_string).count();
                                    if occurrences == 0 {
                                        anyhow::bail!(
                                            "old_string not found in {}. Make sure it matches exactly.",
                                            params.file_path
                                        );
                                    } else {
                                        anyhow::bail!(
                                            "old_string found {} times in {}. Use replace_all=true or provide more context to make it unique.",
                                            occurrences,
                                            params.file_path
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        };

        if count == 0 {
            anyhow::bail!(
                "old_string not found in {}. Make sure it matches exactly.",
                params.file_path
            );
        }

        std::fs::write(path, &new_content)?;

        Ok(ToolOutput {
            title: format!("Edit {}", params.file_path),
            output: format!("Replaced {} occurrence(s) in {}", count, params.file_path),
            metadata: serde_json::json!({
                "file_path": params.file_path,
                "replacements": count,
            }),
        })
    }
}

/// Strategy 1: exact match, unique occurrence.
fn try_replace(content: &str, old: &str, new: &str) -> Option<String> {
    let count = content.matches(old).count();
    if count == 1 {
        Some(content.replacen(old, new, 1))
    } else {
        None
    }
}

/// Strategy 2: line-trimmed matching.
/// Trim each line before matching.
fn try_replace_trimmed(content: &str, old: &str, new: &str) -> Option<String> {
    let old_lines: Vec<&str> = old.lines().map(|l| l.trim()).collect();
    let content_lines: Vec<&str> = content.lines().collect();

    if old_lines.is_empty() {
        return None;
    }

    let mut match_start = None;
    'outer: for i in 0..content_lines.len() {
        if i + old_lines.len() > content_lines.len() {
            break;
        }
        for (j, old_line) in old_lines.iter().enumerate() {
            if content_lines[i + j].trim() != *old_line {
                continue 'outer;
            }
        }
        if match_start.is_some() {
            return None; // Multiple matches
        }
        match_start = Some(i);
    }

    let start = match_start?;
    let mut result_lines: Vec<String> = Vec::new();
    result_lines.extend(content_lines[..start].iter().map(|l| l.to_string()));

    // Detect indentation from the first matched line
    let indent = content_lines[start]
        .chars()
        .take_while(|c| c.is_whitespace())
        .collect::<String>();

    for new_line in new.lines() {
        if new_line.trim().is_empty() {
            result_lines.push(String::new());
        } else {
            result_lines.push(format!("{indent}{}", new_line.trim()));
        }
    }

    result_lines.extend(
        content_lines[start + old_lines.len()..]
            .iter()
            .map(|l| l.to_string()),
    );

    let mut result = result_lines.join("\n");
    if content.ends_with('\n') {
        result.push('\n');
    }
    Some(result)
}

/// Strategy 3: whitespace-normalized matching.
fn try_replace_normalized(content: &str, old: &str, new: &str) -> Option<String> {
    let normalize = |s: &str| -> String { s.split_whitespace().collect::<Vec<_>>().join(" ") };

    let norm_content = normalize(content);
    let norm_old = normalize(old);

    if norm_content.matches(&norm_old).count() != 1 {
        return None;
    }

    // Find the position in normalized space, then map back
    let norm_pos = norm_content.find(&norm_old)?;

    // This is a rough heuristic — find the approximate position in original
    let mut char_count = 0;
    let mut orig_start = 0;
    for (i, c) in content.chars().enumerate() {
        if !c.is_whitespace() {
            if char_count == count_non_ws(&norm_content[..norm_pos]) {
                orig_start = i;
                break;
            }
            char_count += 1;
        }
    }

    // Find end position similarly
    let mut char_count = 0;
    let target = count_non_ws(&norm_old);
    let mut orig_end = content.len();
    for (i, c) in content[orig_start..].chars().enumerate() {
        if !c.is_whitespace() {
            char_count += 1;
            if char_count == target {
                // Find end of this word
                orig_end = orig_start + i + c.len_utf8();
                // Include trailing whitespace
                for ch in content[orig_end..].chars() {
                    if ch == '\n' {
                        orig_end += 1;
                        break;
                    }
                    if ch.is_whitespace() {
                        orig_end += ch.len_utf8();
                    } else {
                        break;
                    }
                }
                break;
            }
        }
    }

    let mut result = String::new();
    result.push_str(&content[..orig_start]);
    result.push_str(new);
    result.push_str(&content[orig_end..]);
    Some(result)
}

fn count_non_ws(s: &str) -> usize {
    s.chars().filter(|c| !c.is_whitespace()).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_replace() {
        let content = "hello world\nfoo bar\nbaz";
        let result = try_replace(content, "foo bar", "foo baz").unwrap();
        assert_eq!(result, "hello world\nfoo baz\nbaz");
    }

    #[test]
    fn exact_replace_not_found() {
        let content = "hello world";
        assert!(try_replace(content, "not here", "x").is_none());
    }

    #[test]
    fn exact_replace_multiple() {
        let content = "foo foo foo";
        assert!(try_replace(content, "foo", "bar").is_none()); // 3 occurrences
    }

    #[test]
    fn trimmed_replace() {
        let content = "  hello world  \n  foo bar  \n  baz  \n";
        let result = try_replace_trimmed(content, "foo bar", "foo baz").unwrap();
        assert!(result.contains("foo baz"));
    }

    #[test]
    fn trimmed_replace_preserves_indent() {
        let content = "    if true {\n        do_thing();\n    }\n";
        let result = try_replace_trimmed(content, "do_thing();", "do_other();").unwrap();
        assert!(result.contains("    do_other();") || result.contains("        do_other();"));
    }
}
