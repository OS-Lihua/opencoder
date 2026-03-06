//! Multi-edit tool: make multiple targeted edits across files in one call.

use anyhow::Result;
use serde::Deserialize;

use crate::tool::{Tool, ToolContext, ToolOutput};
use crate::tools::edit::EditTool;

#[derive(Deserialize)]
struct EditItem {
    file_path: String,
    old_string: String,
    new_string: String,
    #[serde(default)]
    replace_all: bool,
}

pub struct MultiEditTool;

#[async_trait::async_trait]
impl Tool for MultiEditTool {
    fn id(&self) -> &str {
        "multiedit"
    }

    fn description(&self) -> &str {
        include_str!("../../descriptions/multiedit.txt")
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["edits"],
            "properties": {
                "edits": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "required": ["file_path", "old_string", "new_string"],
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
                        }
                    },
                    "description": "List of edits to apply"
                }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let edits: Vec<EditItem> = serde_json::from_value(params["edits"].clone())
            .map_err(|e| anyhow::anyhow!("invalid edits format: {e}"))?;

        if edits.is_empty() {
            return Ok(ToolOutput {
                title: "MultiEdit (0 edits)".to_string(),
                output: "No edits provided.".to_string(),
                metadata: serde_json::json!({ "edits": 0 }),
            });
        }

        let edit_tool = EditTool;
        let mut results = Vec::new();
        let mut errors = Vec::new();

        for (i, edit) in edits.iter().enumerate() {
            let edit_params = serde_json::json!({
                "file_path": edit.file_path,
                "old_string": edit.old_string,
                "new_string": edit.new_string,
                "replace_all": edit.replace_all,
            });

            match edit_tool.execute(edit_params, ctx).await {
                Ok(output) => {
                    results.push(format!("{}. {}: {}", i + 1, edit.file_path, output.output));
                }
                Err(e) => {
                    errors.push(format!("{}. {}: Error: {}", i + 1, edit.file_path, e));
                }
            }
        }

        let mut output = String::new();
        if !results.is_empty() {
            output.push_str(&results.join("\n"));
        }
        if !errors.is_empty() {
            if !output.is_empty() {
                output.push_str("\n\nErrors:\n");
            }
            output.push_str(&errors.join("\n"));
        }

        Ok(ToolOutput {
            title: format!("MultiEdit ({} ok, {} errors)", results.len(), errors.len()),
            output,
            metadata: serde_json::json!({
                "total": edits.len(),
                "success": results.len(),
                "errors": errors.len(),
            }),
        })
    }
}
