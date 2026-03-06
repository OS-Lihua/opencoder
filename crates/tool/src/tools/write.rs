//! Write tool: create or overwrite files.
//!
//! Mirrors `src/tool/write.ts` from the original OpenCode.

use anyhow::Result;
use serde::Deserialize;

use crate::tool::{Tool, ToolContext, ToolOutput};

pub struct WriteTool;

#[derive(Deserialize)]
struct Params {
    file_path: String,
    content: String,
}

#[async_trait::async_trait]
impl Tool for WriteTool {
    fn id(&self) -> &str {
        "write"
    }

    fn description(&self) -> &str {
        "Create or overwrite a file with the given content. Use this for creating new files or completely replacing file contents. For partial modifications, prefer the edit tool."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Absolute path to the file to write"
                },
                "content": {
                    "type": "string",
                    "description": "The content to write to the file"
                }
            },
            "required": ["file_path", "content"]
        })
    }

    async fn execute(&self, params: serde_json::Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let params: Params = serde_json::from_value(params)?;
        let path = std::path::Path::new(&params.file_path);

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::write(path, &params.content)?;

        let line_count = params.content.lines().count();
        let byte_count = params.content.len();

        Ok(ToolOutput {
            title: format!("Write {}", params.file_path),
            output: format!(
                "Successfully wrote {} lines ({} bytes) to {}",
                line_count, byte_count, params.file_path
            ),
            metadata: serde_json::json!({
                "file_path": params.file_path,
                "lines": line_count,
                "bytes": byte_count,
            }),
        })
    }
}
