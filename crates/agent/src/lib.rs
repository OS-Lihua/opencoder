//! Agent system: definitions, permissions, and the main agent loop.
//!
//! Mirrors `src/agent/` from the original OpenCode.

pub mod agent;
pub mod agent_loop;
pub mod permission;

pub use agent::{AgentDef, AgentRegistry, PermissionAction, PermissionRule};
