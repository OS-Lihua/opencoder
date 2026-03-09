//! MCP tool wrapper: wraps an MCP tool as an internal Tool.

use std::sync::Arc;

use anyhow::Result;
use serde_json::{Value, json};

use opencoder_mcp::McpClient;
use opencoder_mcp::protocol::McpToolDef;

use crate::tool::{Tool, ToolContext, ToolOutput};

/// Wraps a single MCP tool as an internal Tool implementation.
pub struct McpToolWrapper {
    tool_id: String,
    tool_name: String,
    tool_description: String,
    schema: Value,
    client: Arc<McpClient>,
}

impl McpToolWrapper {
    pub fn new(server_name: &str, tool_def: &McpToolDef, client: Arc<McpClient>) -> Self {
        Self {
            tool_id: format!("mcp__{server_name}__{}", tool_def.name),
            tool_name: tool_def.name.clone(),
            tool_description: tool_def
                .description
                .clone()
                .unwrap_or_else(|| format!("MCP tool: {}", tool_def.name)),
            schema: tool_def
                .input_schema
                .clone()
                .unwrap_or(json!({"type": "object"})),
            client,
        }
    }
}

#[async_trait::async_trait]
impl Tool for McpToolWrapper {
    fn id(&self) -> &str {
        &self.tool_id
    }

    fn description(&self) -> &str {
        &self.tool_description
    }

    fn parameters_schema(&self) -> Value {
        self.schema.clone()
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let result = self.client.call_tool(&self.tool_name, params).await?;
        let output = serde_json::to_string_pretty(&result)?;
        Ok(ToolOutput {
            title: self.tool_id.clone(),
            output,
            metadata: json!({}),
        })
    }
}
