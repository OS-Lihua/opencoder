//! LLM Provider abstraction layer.
//!
//! Mirrors `src/provider/` from the original OpenCode.
//! Provides a unified interface for Anthropic, OpenAI, Google, Azure, etc.

pub mod error;
pub mod init;
pub mod model;
pub mod models_db;
pub mod provider;
pub mod providers;
pub mod sse;

// Re-export key types for convenience.
pub use provider::{
    ChatMessage, ChatRequest, ChatResponse, ContentPart, FinishReason, LlmProvider, Role,
    StreamEvent, ToolCall, ToolDefinition, Usage,
};
pub use providers::anthropic::AnthropicProvider;
pub use providers::openai::OpenAiProvider;
