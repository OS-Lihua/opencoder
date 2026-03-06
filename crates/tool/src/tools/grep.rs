//! Grep tool - search file contents with regex.
//!
//! Mirrors `src/tool/grep.ts` from the original OpenCode.
//! Uses `rg` (ripgrep) subprocess if available, falls back to manual search.

use std::path::PathBuf;
use std::process::Stdio;

use tokio::io::AsyncReadExt;
use tokio::process::Command;

use crate::tool::{Tool, ToolContext, ToolOutput};

const MAX_MATCHES: usize = 100;
const MAX_LINE_LEN: usize = 2000;

pub struct GrepTool;

#[async_trait::async_trait]
impl Tool for GrepTool {
    fn id(&self) -> &str {
        "grep"
    }

    fn description(&self) -> &str {
        include_str!("../../descriptions/grep.txt")
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["pattern"],
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Regex pattern to search for"
                },
                "path": {
                    "type": "string",
                    "description": "Directory to search in (default: current directory)"
                },
                "include": {
                    "type": "string",
                    "description": "Glob pattern to filter files (e.g. '*.rs', '*.ts')"
                }
            }
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        _ctx: &ToolContext,
    ) -> anyhow::Result<ToolOutput> {
        let pattern = params["pattern"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'pattern' parameter"))?;
        let search_dir = params["path"]
            .as_str()
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        let include = params["include"].as_str();

        // Try ripgrep first
        if let Some(output) = try_ripgrep(pattern, &search_dir, include).await {
            return Ok(output);
        }

        // Fallback: manual grep not implemented yet
        Ok(ToolOutput {
            title: pattern.to_string(),
            output: "ripgrep (rg) not found. Install it for grep functionality.".to_string(),
            metadata: serde_json::json!({ "error": "rg_not_found" }),
        })
    }
}

async fn try_ripgrep(
    pattern: &str,
    dir: &PathBuf,
    include: Option<&str>,
) -> Option<ToolOutput> {
    let mut cmd = Command::new("rg");
    cmd.args(["-nH", "--hidden", "--no-messages", "--field-match-separator=|"])
        .arg(pattern)
        .arg(dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    if let Some(glob) = include {
        cmd.args(["--glob", glob]);
    }

    let mut child = cmd.spawn().ok()?;

    let mut stdout = String::new();
    if let Some(ref mut out) = child.stdout {
        out.read_to_string(&mut stdout).await.ok();
    }

    let status = child.wait().await.ok()?;

    // rg exit 0 = matches, 1 = no matches, 2 = error
    if !status.success() && stdout.is_empty() {
        if status.code() == Some(1) {
            return Some(ToolOutput {
                title: pattern.to_string(),
                output: "No matches found.".to_string(),
                metadata: serde_json::json!({ "matches": 0 }),
            });
        }
        return None; // error or not found
    }

    // Parse and format results
    let lines: Vec<&str> = stdout.lines().collect();
    let total = lines.len();
    let truncated = total > MAX_MATCHES;

    let mut output = String::new();
    for line in lines.iter().take(MAX_MATCHES) {
        let display = if line.len() > MAX_LINE_LEN {
            format!("{}...", &line[..MAX_LINE_LEN])
        } else {
            line.to_string()
        };
        output.push_str(&display);
        output.push('\n');
    }

    if truncated {
        output.push_str(&format!(
            "\n[Showing {MAX_MATCHES} of {total} matches. Use a more specific pattern.]"
        ));
    }

    Some(ToolOutput {
        title: pattern.to_string(),
        output,
        metadata: serde_json::json!({
            "matches": total.min(MAX_MATCHES),
            "total": total,
            "truncated": truncated,
        }),
    })
}
