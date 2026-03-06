//! Core Tool trait and types.
//!
//! Mirrors `src/tool/tool.ts` from the original OpenCode.

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

/// Trait for running sub-agent loops (used by the task tool).
/// Implemented in the agent crate to avoid circular dependencies.
#[async_trait::async_trait]
pub trait AgentRunner: Send + Sync {
    /// Run a sub-agent with the given prompt and return the final text output.
    async fn run_sub_agent(
        &self,
        prompt: &str,
        agent_name: &str,
        parent_session_id: &str,
        cancel: CancellationToken,
    ) -> anyhow::Result<String>;
}

/// Context passed to every tool execution.
#[derive(Clone)]
pub struct ToolContext {
    pub session_id: String,
    pub message_id: String,
    pub agent: String,
    pub call_id: String,
    pub cancel: CancellationToken,
    /// Event bus for tools that need to communicate with UI (e.g., question tool).
    pub bus: Option<Arc<opencoder_core::bus::Bus>>,
    /// Database access for tools that need persistence (e.g., todo tool).
    pub db: Option<Arc<opencoder_core::storage::Database>>,
    /// Project directory for path resolution.
    pub project_dir: Option<PathBuf>,
    /// Agent runner for sub-agent execution (e.g., task tool).
    pub agent_runner: Option<Arc<dyn AgentRunner>>,
}

/// Result of a tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    pub title: String,
    pub output: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// The core Tool trait. Every tool (bash, read, edit, etc.) implements this.
#[async_trait::async_trait]
pub trait Tool: Send + Sync {
    /// Unique identifier (e.g., "bash", "read", "edit").
    fn id(&self) -> &str;

    /// Human-readable description for the LLM.
    fn description(&self) -> &str;

    /// JSON Schema for the tool's parameters.
    fn parameters_schema(&self) -> serde_json::Value;

    /// Execute the tool with the given parameters.
    async fn execute(
        &self,
        params: serde_json::Value,
        ctx: &ToolContext,
    ) -> anyhow::Result<ToolOutput>;
}
