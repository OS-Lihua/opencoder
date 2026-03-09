//! Model Context Protocol (MCP) client.
//!
//! Mirrors `src/mcp/` from the original OpenCode.
//! Supports stdio and HTTP/SSE transports for connecting to MCP servers.

pub mod client;
pub mod manager;
pub mod protocol;
pub mod transport;

pub use client::{McpClient, McpServerConfig};
pub use manager::McpManager;
