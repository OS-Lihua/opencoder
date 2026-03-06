//! Core Provider trait and request/response types.
//!
//! Defines the unified interface that all LLM providers must implement.

use std::collections::HashMap;
use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

/// A tool definition that can be sent to an LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    /// JSON Schema describing the tool's parameters.
    pub parameters: serde_json::Value,
}

/// The role of a chat message participant.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// Content within a chat message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    Text {
        text: String,
    },
    Image {
        /// Base64-encoded image data or URL.
        data: String,
        media_type: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(default)]
        is_error: bool,
    },
}

/// A single message in a chat conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: Vec<ContentPart>,
}

impl ChatMessage {
    /// Create a simple text message with the given role.
    pub fn text(role: Role, text: impl Into<String>) -> Self {
        Self {
            role,
            content: vec![ContentPart::Text { text: text.into() }],
        }
    }

    /// Create a tool result message.
    pub fn tool_result(
        tool_use_id: impl Into<String>,
        content: impl Into<String>,
        is_error: bool,
    ) -> Self {
        Self {
            role: Role::Tool,
            content: vec![ContentPart::ToolResult {
                tool_use_id: tool_use_id.into(),
                content: content.into(),
                is_error,
            }],
        }
    }

    /// Get the text content of this message, if it contains a single text part.
    pub fn text_content(&self) -> Option<&str> {
        for part in &self.content {
            if let ContentPart::Text { text } = part {
                return Some(text);
            }
        }
        None
    }
}

/// A tool call produced by an LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Token usage information.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_read_tokens: u64,
    #[serde(default)]
    pub cache_creation_tokens: u64,
}

/// Why the model stopped generating.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    ToolUse,
    MaxTokens,
    ContentFilter,
    Error,
}

/// A complete (non-streaming) chat response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub usage: Usage,
    pub finish_reason: FinishReason,
}

/// Events emitted during streaming.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// A chunk of generated text.
    TextDelta(String),
    /// A chunk of reasoning/thinking text.
    ReasoningDelta(String),
    /// The start of a tool call.
    ToolCallStart {
        index: usize,
        id: String,
        name: String,
    },
    /// A chunk of tool call arguments (JSON fragment).
    ToolCallDelta {
        index: usize,
        arguments_delta: String,
    },
    /// The end of a tool call.
    ToolCallEnd { index: usize },
    /// A generation step finished, with usage info.
    StepFinish {
        finish_reason: FinishReason,
        usage: Usage,
    },
    /// An error occurred during streaming.
    Error(String),
}

/// A request to a chat LLM.
#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<ToolDefinition>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub max_tokens: Option<u64>,
    /// Provider-specific options (e.g., reasoning budget, beta flags).
    pub provider_options: HashMap<String, serde_json::Value>,
}

impl ChatRequest {
    /// Create a new ChatRequest with required fields and sensible defaults.
    pub fn new(model: impl Into<String>, messages: Vec<ChatMessage>) -> Self {
        Self {
            model: model.into(),
            messages,
            tools: Vec::new(),
            temperature: None,
            top_p: None,
            max_tokens: None,
            provider_options: HashMap::new(),
        }
    }
}

/// The core trait that all LLM providers must implement.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// A unique identifier for this provider (e.g., "anthropic", "openai").
    fn id(&self) -> &str;

    /// A human-readable name for this provider.
    fn name(&self) -> &str;

    /// Send a non-streaming chat request.
    async fn chat(
        &self,
        request: ChatRequest,
        cancel: CancellationToken,
    ) -> anyhow::Result<ChatResponse>;

    /// Send a streaming chat request, returning a stream of events.
    async fn stream(
        &self,
        request: ChatRequest,
        cancel: CancellationToken,
    ) -> anyhow::Result<Pin<Box<dyn Stream<Item = anyhow::Result<StreamEvent>> + Send>>>;
}
