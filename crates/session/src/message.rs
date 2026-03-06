//! Message and Part type system for sessions.
//!
//! Mirrors `src/session/message-v2.ts` from the original OpenCode.
//! Uses Rust enums for tagged unions — much safer than TS discriminated unions.

use serde::{Deserialize, Serialize};

/// A message in a session conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role")]
pub enum Message {
    #[serde(rename = "user")]
    User(UserMessage),
    #[serde(rename = "assistant")]
    Assistant(AssistantMessage),
}

impl Message {
    pub fn role(&self) -> &str {
        match self {
            Message::User(_) => "user",
            Message::Assistant(_) => "assistant",
        }
    }
}

/// A user message containing text and optional images.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMessage {
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub images: Vec<ImageContent>,
}

/// An assistant response message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMessage {
    /// The model used (e.g., "anthropic/claude-3-5-sonnet").
    #[serde(default)]
    pub model: String,
    /// The agent that produced this message.
    #[serde(default)]
    pub agent: String,
    /// System prompt at the time of generation.
    #[serde(default)]
    pub system: String,
}

/// An image attached to a user message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageContent {
    pub url: String,
    #[serde(default)]
    pub media_type: String,
}

/// All possible part types that can be attached to a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Part {
    #[serde(rename = "text")]
    Text(TextPart),
    #[serde(rename = "reasoning")]
    Reasoning(ReasoningPart),
    #[serde(rename = "snapshot")]
    Snapshot(SnapshotPart),
    #[serde(rename = "patch")]
    Patch(PatchPart),
    #[serde(rename = "file")]
    File(FilePart),
    #[serde(rename = "tool")]
    Tool(ToolPart),
    #[serde(rename = "step-start")]
    StepStart(StepStartPart),
    #[serde(rename = "step-finish")]
    StepFinish(StepFinishPart),
    #[serde(rename = "agent")]
    Agent(AgentPart),
    #[serde(rename = "retry")]
    Retry(RetryPart),
    #[serde(rename = "compaction")]
    Compaction(CompactionPart),
    #[serde(rename = "subtask")]
    Subtask(SubtaskPart),
}

impl Part {
    pub fn type_name(&self) -> &str {
        match self {
            Part::Text(_) => "text",
            Part::Reasoning(_) => "reasoning",
            Part::Snapshot(_) => "snapshot",
            Part::Patch(_) => "patch",
            Part::File(_) => "file",
            Part::Tool(_) => "tool",
            Part::StepStart(_) => "step-start",
            Part::StepFinish(_) => "step-finish",
            Part::Agent(_) => "agent",
            Part::Retry(_) => "retry",
            Part::Compaction(_) => "compaction",
            Part::Subtask(_) => "subtask",
        }
    }
}

/// Text content from the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextPart {
    #[serde(default)]
    pub content: String,
}

/// Reasoning/thinking content (extended thinking).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningPart {
    #[serde(default)]
    pub content: String,
    /// Opaque reasoning token for multi-turn reasoning.
    #[serde(default)]
    pub reasoning: Option<String>,
}

/// A snapshot of file state (git tree hash).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotPart {
    pub hash: String,
}

/// A unified diff patch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchPart {
    pub file_path: String,
    pub content: String,
    #[serde(default)]
    pub additions: i64,
    #[serde(default)]
    pub deletions: i64,
    #[serde(default)]
    pub is_new: bool,
    #[serde(default)]
    pub is_deleted: bool,
}

/// A file reference attached to a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilePart {
    pub file_path: String,
    pub media_type: String,
    /// Base64-encoded content or URL.
    #[serde(default)]
    pub content: Option<String>,
}

/// A tool invocation with its lifecycle state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPart {
    /// The tool call ID from the LLM.
    pub call_id: String,
    /// Tool name (e.g., "bash", "read").
    pub tool: String,
    /// The current state of this tool invocation.
    pub state: ToolState,
}

/// Tool execution lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum ToolState {
    #[serde(rename = "pending")]
    Pending {
        input: serde_json::Value,
        #[serde(default)]
        raw: Option<String>,
    },
    #[serde(rename = "running")]
    Running {
        input: serde_json::Value,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        metadata: Option<serde_json::Value>,
        time_start: i64,
    },
    #[serde(rename = "completed")]
    Completed {
        input: serde_json::Value,
        output: String,
        title: String,
        #[serde(default)]
        metadata: serde_json::Value,
        time_start: i64,
        time_end: i64,
        #[serde(default)]
        attachments: Option<Vec<FilePart>>,
    },
    #[serde(rename = "error")]
    Error {
        input: serde_json::Value,
        error: String,
        #[serde(default)]
        metadata: Option<serde_json::Value>,
        time_start: i64,
        time_end: i64,
    },
}

/// Marks the start of an LLM generation step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepStartPart {
    #[serde(default)]
    pub step_index: u32,
}

/// Marks the end of an LLM generation step with usage info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepFinishPart {
    #[serde(default)]
    pub step_index: u32,
    pub finish_reason: String,
    #[serde(default)]
    pub usage: UsageInfo,
}

/// Token usage tracking for a step.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageInfo {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub reasoning_tokens: u64,
    #[serde(default)]
    pub cache_read_tokens: u64,
    #[serde(default)]
    pub cache_creation_tokens: u64,
}

/// An agent switch marker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPart {
    pub agent: String,
}

/// A retry marker with delay info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPart {
    pub attempt: u32,
    #[serde(default)]
    pub error: String,
    /// Next retry timestamp (ms since epoch).
    #[serde(default)]
    pub next: i64,
}

/// Context compaction marker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionPart {
    /// Summary of compacted content.
    #[serde(default)]
    pub summary: String,
    /// Number of messages compacted.
    #[serde(default)]
    pub compacted_count: u32,
}

/// A subtask (child session) reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtaskPart {
    pub session_id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub summary: Option<String>,
}

/// A message with its associated parts (for API responses).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageWithParts {
    pub id: String,
    pub session_id: String,
    pub message: Message,
    pub parts: Vec<PartWithId>,
    pub time_created: i64,
    pub time_updated: i64,
}

/// A part with its database ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartWithId {
    pub id: String,
    pub message_id: String,
    pub session_id: String,
    pub part: Part,
    pub time_created: i64,
    pub time_updated: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_user_message() {
        let msg = Message::User(UserMessage {
            content: "hello".into(),
            images: vec![],
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"role\":\"user\""));
        let parsed: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.role(), "user");
    }

    #[test]
    fn serialize_assistant_message() {
        let msg = Message::Assistant(AssistantMessage {
            model: "anthropic/claude-3-5-sonnet".into(),
            agent: "build".into(),
            system: "You are a coding agent.".into(),
        });
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"role\":\"assistant\""));
    }

    #[test]
    fn serialize_text_part() {
        let part = Part::Text(TextPart {
            content: "Hello world".into(),
        });
        let json = serde_json::to_string(&part).unwrap();
        assert!(json.contains("\"type\":\"text\""));
        let parsed: Part = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.type_name(), "text");
    }

    #[test]
    fn serialize_tool_part_pending() {
        let part = Part::Tool(ToolPart {
            call_id: "call_123".into(),
            tool: "bash".into(),
            state: ToolState::Pending {
                input: serde_json::json!({"command": "ls"}),
                raw: None,
            },
        });
        let json = serde_json::to_string(&part).unwrap();
        assert!(json.contains("\"status\":\"pending\""));
    }

    #[test]
    fn serialize_tool_part_completed() {
        let part = Part::Tool(ToolPart {
            call_id: "call_456".into(),
            tool: "read".into(),
            state: ToolState::Completed {
                input: serde_json::json!({"file_path": "/tmp/test.txt"}),
                output: "file contents".into(),
                title: "Read /tmp/test.txt".into(),
                metadata: serde_json::json!({}),
                time_start: 1000,
                time_end: 1050,
                attachments: None,
            },
        });
        let json = serde_json::to_string(&part).unwrap();
        assert!(json.contains("\"status\":\"completed\""));
        let parsed: Part = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.type_name(), "tool");
    }

    #[test]
    fn roundtrip_all_part_types() {
        let parts = vec![
            Part::Text(TextPart { content: "hi".into() }),
            Part::Reasoning(ReasoningPart { content: "thinking...".into(), reasoning: None }),
            Part::Snapshot(SnapshotPart { hash: "abc123".into() }),
            Part::Patch(PatchPart {
                file_path: "/a.rs".into(),
                content: "+line".into(),
                additions: 1,
                deletions: 0,
                is_new: false,
                is_deleted: false,
            }),
            Part::File(FilePart {
                file_path: "/img.png".into(),
                media_type: "image/png".into(),
                content: None,
            }),
            Part::StepStart(StepStartPart { step_index: 0 }),
            Part::StepFinish(StepFinishPart {
                step_index: 0,
                finish_reason: "stop".into(),
                usage: UsageInfo::default(),
            }),
            Part::Agent(AgentPart { agent: "build".into() }),
            Part::Retry(RetryPart { attempt: 1, error: "rate limited".into(), next: 5000 }),
            Part::Compaction(CompactionPart { summary: "summarized".into(), compacted_count: 10 }),
            Part::Subtask(SubtaskPart {
                session_id: "ses_abc".into(),
                title: "subtask".into(),
                summary: None,
            }),
        ];

        for part in parts {
            let json = serde_json::to_string(&part).unwrap();
            let parsed: Part = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed.type_name(), part.type_name());
        }
    }
}
