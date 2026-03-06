//! MCP client: manages connections to MCP servers.
//!
//! Handles initialization, tool listing, tool calling, and lifecycle.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::protocol::{InitializeResult, McpPrompt, McpResource, McpToolDef, ServerCapabilities};
use crate::transport::{HttpTransport, StdioTransport, Transport};

/// Configuration for an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// Command to run (for stdio transport).
    #[serde(default)]
    pub command: Option<String>,
    /// Arguments for the command.
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment variables.
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// URL for HTTP transport.
    #[serde(default)]
    pub url: Option<String>,
    /// HTTP headers.
    #[serde(default)]
    pub headers: HashMap<String, String>,
}

/// Connection status of an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum McpStatus {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

/// An active MCP client connected to a server.
pub struct McpClient {
    name: String,
    transport: Arc<dyn Transport>,
    capabilities: ServerCapabilities,
    tools: Vec<McpToolDef>,
    resources: Vec<McpResource>,
    prompts: Vec<McpPrompt>,
    status: McpStatus,
}

impl McpClient {
    /// Connect to an MCP server using the given config.
    pub async fn connect(name: &str, config: &McpServerConfig) -> Result<Self> {
        let transport: Arc<dyn Transport> = if let Some(ref command) = config.command {
            let (transport, _child) =
                StdioTransport::spawn(command, &config.args, &config.env).await?;
            Arc::new(transport)
        } else if let Some(ref url) = config.url {
            Arc::new(HttpTransport::new(url, config.headers.clone()))
        } else {
            anyhow::bail!("MCP server config must have either 'command' or 'url'");
        };

        let mut client = Self {
            name: name.to_string(),
            transport,
            capabilities: ServerCapabilities::default(),
            tools: Vec::new(),
            resources: Vec::new(),
            prompts: Vec::new(),
            status: McpStatus::Connecting,
        };

        // Initialize
        match client.initialize().await {
            Ok(_) => {
                client.status = McpStatus::Connected;
                info!(name, "MCP server connected");
            }
            Err(e) => {
                client.status = McpStatus::Error(e.to_string());
                warn!(name, error = %e, "MCP server initialization failed");
                return Err(e);
            }
        }

        Ok(client)
    }

    async fn initialize(&mut self) -> Result<()> {
        let params = serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "opencoder",
                "version": env!("CARGO_PKG_VERSION")
            }
        });

        let result = self.transport.request("initialize", Some(params)).await?;
        let init: InitializeResult =
            serde_json::from_value(result).context("failed to parse initialize result")?;

        self.capabilities = init.capabilities;

        if let Some(ref info) = init.server_info {
            debug!(
                name = %self.name,
                server = %info.name,
                version = info.version.as_deref().unwrap_or("unknown"),
                "MCP server info"
            );
        }

        // Send initialized notification
        self.transport.notify("notifications/initialized", None).await?;

        // Fetch tools if supported
        if self.capabilities.tools.is_some() {
            self.refresh_tools().await?;
        }

        // Fetch resources if supported
        if self.capabilities.resources.is_some() {
            self.refresh_resources().await?;
        }

        // Fetch prompts if supported
        if self.capabilities.prompts.is_some() {
            self.refresh_prompts().await?;
        }

        Ok(())
    }

    /// Refresh the list of available tools.
    pub async fn refresh_tools(&mut self) -> Result<()> {
        let result = self.transport.request("tools/list", None).await?;
        let tools_result: ToolsListResult =
            serde_json::from_value(result).unwrap_or(ToolsListResult { tools: Vec::new() });
        self.tools = tools_result.tools;
        debug!(name = %self.name, count = self.tools.len(), "MCP tools refreshed");
        Ok(())
    }

    /// Refresh the list of available resources.
    pub async fn refresh_resources(&mut self) -> Result<()> {
        let result = self.transport.request("resources/list", None).await?;
        let resources_result: ResourcesListResult = serde_json::from_value(result)
            .unwrap_or(ResourcesListResult {
                resources: Vec::new(),
            });
        self.resources = resources_result.resources;
        Ok(())
    }

    /// Refresh the list of available prompts.
    pub async fn refresh_prompts(&mut self) -> Result<()> {
        let result = self.transport.request("prompts/list", None).await?;
        let prompts_result: PromptsListResult =
            serde_json::from_value(result).unwrap_or(PromptsListResult { prompts: Vec::new() });
        self.prompts = prompts_result.prompts;
        Ok(())
    }

    /// Call an MCP tool.
    pub async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let params = serde_json::json!({
            "name": name,
            "arguments": arguments,
        });
        self.transport.request("tools/call", Some(params)).await
    }

    /// Read an MCP resource.
    pub async fn read_resource(&self, uri: &str) -> Result<serde_json::Value> {
        let params = serde_json::json!({"uri": uri});
        self.transport.request("resources/read", Some(params)).await
    }

    /// Get a prompt.
    pub async fn get_prompt(
        &self,
        name: &str,
        arguments: Option<HashMap<String, String>>,
    ) -> Result<serde_json::Value> {
        let params = serde_json::json!({
            "name": name,
            "arguments": arguments.unwrap_or_default(),
        });
        self.transport.request("prompts/get", Some(params)).await
    }

    /// Get the list of tools.
    pub fn tools(&self) -> &[McpToolDef] {
        &self.tools
    }

    /// Get the list of resources.
    pub fn resources(&self) -> &[McpResource] {
        &self.resources
    }

    /// Get the list of prompts.
    pub fn prompts(&self) -> &[McpPrompt] {
        &self.prompts
    }

    /// Get the connection status.
    pub fn status(&self) -> &McpStatus {
        &self.status
    }

    /// Get the server name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Disconnect from the server.
    pub async fn disconnect(&mut self) -> Result<()> {
        self.transport.close().await?;
        self.status = McpStatus::Disconnected;
        Ok(())
    }
}

#[derive(Deserialize)]
struct ToolsListResult {
    tools: Vec<McpToolDef>,
}

#[derive(Deserialize)]
struct ResourcesListResult {
    resources: Vec<McpResource>,
}

#[derive(Deserialize)]
struct PromptsListResult {
    prompts: Vec<McpPrompt>,
}

/// Convert MCP tools to internal tool definitions.
pub fn mcp_tools_to_tool_defs(
    server_name: &str,
    tools: &[McpToolDef],
) -> Vec<opencoder_provider::ToolDefinition> {
    tools
        .iter()
        .map(|t| opencoder_provider::ToolDefinition {
            name: format!("mcp__{}__{}", server_name, t.name),
            description: t.description.clone().unwrap_or_default(),
            parameters: t.input_schema.clone().unwrap_or(serde_json::json!({"type": "object"})),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_tool_name_conversion() {
        let tools = vec![McpToolDef {
            name: "search".to_string(),
            description: Some("Search the web".to_string()),
            input_schema: Some(serde_json::json!({"type": "object"})),
        }];

        let defs = mcp_tools_to_tool_defs("exa", &tools);
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "mcp__exa__search");
        assert_eq!(defs[0].description, "Search the web");
    }

    #[test]
    fn server_config_deserialize() {
        let json = r#"{"command": "npx", "args": ["-y", "@modelcontextprotocol/server-filesystem"]}"#;
        let config: McpServerConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.command.as_deref(), Some("npx"));
        assert_eq!(config.args.len(), 2);
    }

    #[test]
    fn status_variants() {
        assert_eq!(McpStatus::Connected, McpStatus::Connected);
        assert_ne!(McpStatus::Connected, McpStatus::Disconnected);
    }
}
