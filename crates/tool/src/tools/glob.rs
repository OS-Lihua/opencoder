//! Glob tool - find files by pattern.
//!
//! Mirrors `src/tool/glob.ts` from the original OpenCode.

use std::path::{Path, PathBuf};

use crate::tool::{Tool, ToolContext, ToolOutput};

const MAX_RESULTS: usize = 100;

pub struct GlobTool;

#[async_trait::async_trait]
impl Tool for GlobTool {
    fn id(&self) -> &str {
        "glob"
    }

    fn description(&self) -> &str {
        include_str!("../../descriptions/glob.txt")
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["pattern"],
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Glob pattern to match (e.g. '**/*.rs', 'src/**/*.ts')"
                },
                "path": {
                    "type": "string",
                    "description": "Directory to search in (default: current directory)"
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

        // Build glob matcher
        let glob = globset::GlobBuilder::new(pattern)
            .literal_separator(false)
            .build()
            .map_err(|e| anyhow::anyhow!("invalid glob pattern: {e}"))?
            .compile_matcher();

        // Walk the directory and collect matches
        let mut matches: Vec<(PathBuf, std::time::SystemTime)> = Vec::new();
        collect_matches(&search_dir, &search_dir, &glob, &mut matches).await;

        // Sort by modification time (newest first)
        matches.sort_by(|a, b| b.1.cmp(&a.1));

        let total = matches.len();
        let truncated = total > MAX_RESULTS;
        let display: Vec<String> = matches
            .into_iter()
            .take(MAX_RESULTS)
            .map(|(p, _)| p.to_string_lossy().to_string())
            .collect();

        let mut output = display.join("\n");
        if truncated {
            output.push_str(&format!(
                "\n\n[Showing {MAX_RESULTS} of {total} matches. Use a more specific pattern.]"
            ));
        }

        let title = search_dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string());

        Ok(ToolOutput {
            title,
            output,
            metadata: serde_json::json!({
                "count": display.len(),
                "total": total,
                "truncated": truncated,
            }),
        })
    }
}

async fn collect_matches(
    root: &Path,
    dir: &Path,
    glob: &globset::GlobMatcher,
    matches: &mut Vec<(PathBuf, std::time::SystemTime)>,
) {
    let Ok(mut entries) = tokio::fs::read_dir(dir).await else {
        return;
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        // Skip hidden dirs and common ignores
        if name.starts_with('.') || name == "node_modules" || name == "target" {
            continue;
        }

        if let Ok(ft) = entry.file_type().await {
            if ft.is_dir() {
                Box::pin(collect_matches(root, &path, glob, matches)).await;
            } else if ft.is_file() {
                // Match against relative path
                if let Ok(rel) = path.strip_prefix(root)
                    && glob.is_match(rel)
                {
                    let mtime = entry
                        .metadata()
                        .await
                        .and_then(|m| m.modified())
                        .unwrap_or(std::time::UNIX_EPOCH);
                    matches.push((path, mtime));
                }
            }
        }
    }
}
