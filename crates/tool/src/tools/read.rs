//! Read file tool - read file contents with line numbers.
//!
//! Mirrors `src/tool/read.ts` from the original OpenCode.

use std::path::Path;

use crate::tool::{Tool, ToolContext, ToolOutput};

const DEFAULT_LIMIT: usize = 2000;
const MAX_LINE_LEN: usize = 2000;
const MAX_OUTPUT_BYTES: usize = 50 * 1024;

pub struct ReadTool;

/// Check if a byte slice looks like binary content.
fn is_binary(sample: &[u8]) -> bool {
    if sample.is_empty() {
        return false;
    }
    let null_count = sample.iter().filter(|&&b| b == 0).count();
    if null_count > 0 {
        return true;
    }
    let non_printable = sample
        .iter()
        .filter(|&&b| b < 0x20 && b != b'\n' && b != b'\r' && b != b'\t')
        .count();
    (non_printable as f64 / sample.len() as f64) > 0.3
}

#[async_trait::async_trait]
impl Tool for ReadTool {
    fn id(&self) -> &str {
        "read"
    }

    fn description(&self) -> &str {
        include_str!("../../descriptions/read.txt")
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["file_path"],
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Absolute path to the file to read"
                },
                "offset": {
                    "type": "integer",
                    "description": "Line number to start reading from (1-indexed)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of lines to read (default 2000)"
                }
            }
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _ctx: &ToolContext,
    ) -> anyhow::Result<ToolOutput> {
        let file_path = params["file_path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'file_path' parameter"))?;
        let offset = params["offset"].as_u64().map(|v| v as usize);
        let limit = params["limit"].as_u64().map(|v| v as usize).unwrap_or(DEFAULT_LIMIT);

        let path = Path::new(file_path);

        // Check if path is a directory
        if path.is_dir() {
            return read_directory(path).await;
        }

        // Check if file exists
        if !path.exists() {
            return Ok(ToolOutput {
                title: file_path.to_string(),
                output: format!("Error: file not found: {file_path}"),
                metadata: serde_json::json!({ "error": "not_found" }),
            });
        }

        // Read file bytes
        let bytes = tokio::fs::read(path).await?;

        // Binary detection on first 4KB
        let sample = &bytes[..bytes.len().min(4096)];
        if is_binary(sample) {
            return Ok(ToolOutput {
                title: file_path.to_string(),
                output: format!("Error: file appears to be binary ({} bytes)", bytes.len()),
                metadata: serde_json::json!({ "binary": true, "size": bytes.len() }),
            });
        }

        let content = String::from_utf8_lossy(&bytes);
        let all_lines: Vec<&str> = content.lines().collect();
        let total_lines = all_lines.len();

        // Apply offset (1-indexed)
        let start = offset.unwrap_or(1).saturating_sub(1);
        let end = (start + limit).min(total_lines);
        let selected = &all_lines[start..end];

        // Format with line numbers
        let mut output = String::new();
        let mut bytes_used = 0;
        let width = format!("{}", end).len();

        for (i, line) in selected.iter().enumerate() {
            let line_num = start + i + 1;
            let display_line = if line.len() > MAX_LINE_LEN {
                format!("{}...", &line[..MAX_LINE_LEN])
            } else {
                line.to_string()
            };

            let formatted = format!("{:>width$}\t{}\n", line_num, display_line, width = width);
            bytes_used += formatted.len();
            if bytes_used > MAX_OUTPUT_BYTES && i > 0 {
                output.push_str(&format!(
                    "\n[Output capped at {}KB]",
                    MAX_OUTPUT_BYTES / 1024
                ));
                break;
            }
            output.push_str(&formatted);
        }

        // Truncation message
        let showing = end - start;
        if showing < total_lines {
            let next_offset = end + 1;
            output.push_str(&format!(
                "\nShowing lines {}-{} of {}. Use offset={} to continue.",
                start + 1,
                end,
                total_lines,
                next_offset
            ));
        }

        // Derive a short title (relative-ish path)
        let title = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| file_path.to_string());

        Ok(ToolOutput {
            title,
            output,
            metadata: serde_json::json!({
                "lines": total_lines,
                "showing": showing,
                "truncated": showing < total_lines,
            }),
        })
    }
}

async fn read_directory(path: &Path) -> anyhow::Result<ToolOutput> {
    let mut entries = tokio::fs::read_dir(path).await?;
    let mut names = Vec::new();

    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().to_string();
        let is_dir = entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false);
        if is_dir {
            names.push(format!("{name}/"));
        } else {
            names.push(name);
        }
    }

    names.sort();
    let output = names.join("\n");
    let title = path.to_string_lossy().to_string();

    Ok(ToolOutput {
        title,
        output,
        metadata: serde_json::json!({ "entries": names.len(), "directory": true }),
    })
}
