//! MCP server manager: connects to multiple MCP servers.

use std::collections::HashMap;
use std::sync::Arc;

use tracing::{info, warn};

use crate::client::{McpClient, McpServerConfig};

/// Manages connections to multiple MCP servers.
pub struct McpManager {
    clients: HashMap<String, Arc<McpClient>>,
}

impl McpManager {
    /// Connect to all configured MCP servers. Failures are logged but don't stop startup.
    pub async fn connect_all(configs: HashMap<String, McpServerConfig>) -> Self {
        let mut clients = HashMap::new();
        for (name, config) in configs {
            match McpClient::connect(&name, &config).await {
                Ok(client) => {
                    info!(name = %name, "MCP server connected");
                    clients.insert(name, Arc::new(client));
                }
                Err(e) => {
                    warn!(name = %name, error = %e, "MCP server connection failed");
                }
            }
        }
        Self { clients }
    }

    /// Get a client by server name.
    pub fn client(&self, server_name: &str) -> Option<&Arc<McpClient>> {
        self.clients.get(server_name)
    }

    /// Get all clients.
    pub fn clients(&self) -> &HashMap<String, Arc<McpClient>> {
        &self.clients
    }

    /// Shutdown all connected servers.
    pub async fn shutdown_all(&mut self) {
        for (name, client) in self.clients.drain() {
            // McpClient::disconnect takes &mut self, but we have Arc.
            // We can only disconnect if we have the sole reference.
            if let Some(client) = Arc::into_inner(client) {
                let mut client = client;
                if let Err(e) = client.disconnect().await {
                    warn!(name = %name, error = %e, "MCP shutdown error");
                }
            }
        }
    }
}
