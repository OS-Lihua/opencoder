//! LSP tool: exposes Language Server Protocol features to the LLM.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Result, bail};
use serde_json::{Value, json};

use opencoder_lsp::LspManager;

use crate::tool::{Tool, ToolContext, ToolOutput};

/// Tool that wraps an LspManager for code intelligence.
pub struct LspTool {
    manager: Arc<LspManager>,
}

impl LspTool {
    pub fn new(manager: Arc<LspManager>) -> Self {
        Self { manager }
    }
}

#[async_trait::async_trait]
impl Tool for LspTool {
    fn id(&self) -> &str {
        "lsp"
    }

    fn description(&self) -> &str {
        "Language Server Protocol operations: go to definition, find references, hover info, and document symbols. Requires a file path and operation. For definition/references/hover, also requires line and character position (0-indexed)."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["operation", "file_path"],
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["definition", "references", "hover", "symbols"],
                    "description": "The LSP operation to perform"
                },
                "file_path": {
                    "type": "string",
                    "description": "Path to the file"
                },
                "line": {
                    "type": "integer",
                    "description": "Line number (0-indexed)"
                },
                "character": {
                    "type": "integer",
                    "description": "Character offset (0-indexed)"
                }
            }
        })
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let op = params["operation"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing operation"))?;
        let file_path_str = params["file_path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing file_path"))?;

        let file_path = if file_path_str.starts_with('/') {
            PathBuf::from(file_path_str)
        } else if let Some(ref dir) = ctx.project_dir {
            dir.join(file_path_str)
        } else {
            PathBuf::from(file_path_str)
        };

        let uri = format!("file://{}", file_path.display());
        let client = self.manager.client_for_file(&file_path).await?;

        let result = match op {
            "definition" => {
                let line = params["line"].as_u64().unwrap_or(0) as u32;
                let character = params["character"].as_u64().unwrap_or(0) as u32;
                client.definition(&uri, line, character).await?
            }
            "references" => {
                let line = params["line"].as_u64().unwrap_or(0) as u32;
                let character = params["character"].as_u64().unwrap_or(0) as u32;
                client.references(&uri, line, character).await?
            }
            "hover" => {
                let line = params["line"].as_u64().unwrap_or(0) as u32;
                let character = params["character"].as_u64().unwrap_or(0) as u32;
                client.hover(&uri, line, character).await?
            }
            "symbols" => client.document_symbols(&uri).await?,
            _ => bail!("unknown LSP operation: {op}"),
        };

        let output = serde_json::to_string_pretty(&result)?;
        Ok(ToolOutput {
            title: format!("lsp {op} {file_path_str}"),
            output,
            metadata: json!({}),
        })
    }
}
