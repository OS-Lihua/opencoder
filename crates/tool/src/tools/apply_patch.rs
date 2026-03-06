//! Apply patch tool: apply structured patches to modify files.
//!
//! Uses the opencoder-patch crate for parsing and applying.

use std::path::Path;

use anyhow::Result;

use crate::tool::{Tool, ToolContext, ToolOutput};

pub struct ApplyPatchTool;

#[async_trait::async_trait]
impl Tool for ApplyPatchTool {
    fn id(&self) -> &str {
        "apply_patch"
    }

    fn description(&self) -> &str {
        include_str!("../../descriptions/apply_patch.txt")
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["patch"],
            "properties": {
                "patch": {
                    "type": "string",
                    "description": "The patch text in '*** Begin Patch / *** End Patch' format"
                }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let patch_text = params["patch"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'patch' parameter"))?;

        let project_dir = ctx.project_dir.as_deref().unwrap_or_else(|| Path::new("."));

        let patch = opencoder_patch::parse_patch(patch_text)?;

        let results = opencoder_patch::apply_patch(&patch, |path| {
            let full_path = project_dir.join(path);
            // Security check: prevent path traversal
            let canonical = full_path.canonicalize().ok();
            if let Some(ref canon) = canonical {
                let proj_canon = project_dir.canonicalize().unwrap_or_else(|_| project_dir.to_path_buf());
                if !canon.starts_with(&proj_canon) {
                    return Err(anyhow::anyhow!("path traversal detected: {}", path));
                }
            }

            if full_path.exists() {
                Ok(Some(std::fs::read_to_string(&full_path)?))
            } else {
                Ok(None)
            }
        })?;

        let mut additions = 0usize;
        let mut deletions = 0usize;
        let mut files_changed = Vec::new();

        for (path, content) in &results {
            let full_path = project_dir.join(path);
            match content {
                Some(new_content) => {
                    if let Some(parent) = full_path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(&full_path, new_content)?;
                    files_changed.push(path.clone());
                }
                None => {
                    // File deletion
                    if full_path.exists() {
                        std::fs::remove_file(&full_path)?;
                    }
                    files_changed.push(format!("{path} (deleted)"));
                }
            }
        }

        // Count stats from the patch
        for file_patch in &patch.files {
            for hunk in &file_patch.hunks {
                for line in &hunk.lines {
                    match line {
                        opencoder_patch::DiffLine::Add(_) => additions += 1,
                        opencoder_patch::DiffLine::Remove(_) => deletions += 1,
                        opencoder_patch::DiffLine::Context(_) => {}
                    }
                }
            }
        }

        let summary = format!(
            "Applied patch to {} file(s): +{} -{}\nFiles: {}",
            files_changed.len(),
            additions,
            deletions,
            files_changed.join(", ")
        );

        Ok(ToolOutput {
            title: format!("Patch ({} files)", files_changed.len()),
            output: summary,
            metadata: serde_json::json!({
                "files": files_changed,
                "additions": additions,
                "deletions": deletions,
            }),
        })
    }
}
