//! Stream processor: converts LLM StreamEvents into session Parts.
//!
//! Mirrors `src/session/processor.ts` from the original OpenCode.
//! Handles the streaming loop: LLM events → part creation → tool execution → loop.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use futures::StreamExt;
use tracing::{debug, warn};

use opencoder_core::bus::Bus;
use opencoder_core::storage::Database;
use opencoder_provider::provider::{FinishReason, StreamEvent};
use opencoder_tool::tool::{AgentRunner, Tool, ToolContext, ToolOutput};

use crate::message::*;
use crate::session::SessionService;

/// Accumulated state for a single tool call being streamed.
struct PendingToolCall {
    call_id: String,
    name: String,
    arguments_json: String,
    part_id: Option<String>,
}

/// Processes a stream of LLM events, creating parts in the session.
pub struct StreamProcessor {
    session_svc: Arc<SessionService>,
    tools: HashMap<String, Arc<dyn Tool>>,
    bus: Option<Bus>,
    db: Option<Arc<Database>>,
    project_dir: Option<PathBuf>,
    agent_runner: Option<Arc<dyn AgentRunner>>,
}

impl StreamProcessor {
    pub fn new(
        session_svc: Arc<SessionService>,
        tools: HashMap<String, Arc<dyn Tool>>,
        bus: Option<Bus>,
        db: Option<Arc<Database>>,
        project_dir: Option<PathBuf>,
        agent_runner: Option<Arc<dyn AgentRunner>>,
    ) -> Self {
        Self {
            session_svc,
            tools,
            bus,
            db,
            project_dir,
            agent_runner,
        }
    }

    /// Process a stream of LLM events for a given session/message.
    /// Returns the finish reason and whether tool calls were made.
    pub async fn process(
        &self,
        session_id: &str,
        message_id: &str,
        _agent: &str,
        mut stream: std::pin::Pin<Box<dyn futures::Stream<Item = Result<StreamEvent>> + Send>>,
        cancel: tokio_util::sync::CancellationToken,
        snapshot_hash: Option<String>,
    ) -> Result<ProcessResult> {
        let mut text_content = String::new();
        let mut text_part_id: Option<String> = None;
        let mut reasoning_content = String::new();
        let mut reasoning_part_id: Option<String> = None;
        let mut pending_tools: HashMap<usize, PendingToolCall> = HashMap::new();
        let mut step_index: u32 = 0;
        let mut finish_reason = FinishReason::Stop;
        let mut usage = UsageInfo::default();
        let mut has_tool_calls = false;

        // Create step-start part
        self.session_svc.add_part(
            session_id,
            message_id,
            &Part::StepStart(StepStartPart {
                step_index,
                snapshot_hash,
            }),
        )?;

        loop {
            let event = tokio::select! {
                _ = cancel.cancelled() => {
                    debug!("stream cancelled");
                    break;
                }
                event = stream.next() => {
                    match event {
                        Some(Ok(e)) => e,
                        Some(Err(e)) => {
                            warn!("stream error: {e}");
                            return Err(e);
                        }
                        None => break,
                    }
                }
            };

            match event {
                StreamEvent::TextDelta(delta) => {
                    text_content.push_str(&delta);

                    if let Some(ref pid) = text_part_id {
                        self.session_svc
                            .publish_part_delta(session_id, message_id, pid, "content", &delta);
                    } else {
                        let part = Part::Text(TextPart {
                            content: text_content.clone(),
                        });
                        let pid = self.session_svc.add_part(session_id, message_id, &part)?;
                        text_part_id = Some(pid);
                    }
                }

                StreamEvent::ReasoningDelta(delta) => {
                    reasoning_content.push_str(&delta);

                    if let Some(ref pid) = reasoning_part_id {
                        self.session_svc
                            .publish_part_delta(session_id, message_id, pid, "content", &delta);
                    } else {
                        let part = Part::Reasoning(ReasoningPart {
                            content: reasoning_content.clone(),
                            reasoning: None,
                        });
                        let pid = self.session_svc.add_part(session_id, message_id, &part)?;
                        reasoning_part_id = Some(pid);
                    }
                }

                StreamEvent::ToolCallStart { index, id, name } => {
                    has_tool_calls = true;
                    pending_tools.insert(
                        index,
                        PendingToolCall {
                            call_id: id,
                            name,
                            arguments_json: String::new(),
                            part_id: None,
                        },
                    );
                }

                StreamEvent::ToolCallDelta {
                    index,
                    arguments_delta,
                } => {
                    if let Some(tc) = pending_tools.get_mut(&index) {
                        tc.arguments_json.push_str(&arguments_delta);

                        // Create the tool part as pending once we start getting arguments
                        if tc.part_id.is_none() {
                            let part = Part::Tool(ToolPart {
                                call_id: tc.call_id.clone(),
                                tool: tc.name.clone(),
                                state: ToolState::Pending {
                                    input: serde_json::Value::Null,
                                    raw: Some(tc.arguments_json.clone()),
                                },
                            });
                            let pid = self.session_svc.add_part(session_id, message_id, &part)?;
                            tc.part_id = Some(pid);
                        }
                    }
                }

                StreamEvent::ToolCallEnd { index } => {
                    if let Some(tc) = pending_tools.get_mut(&index) {
                        // Parse the accumulated JSON arguments
                        let input: serde_json::Value =
                            serde_json::from_str(&tc.arguments_json).unwrap_or_default();

                        // Update the part to have parsed input
                        if let Some(ref pid) = tc.part_id {
                            let part = Part::Tool(ToolPart {
                                call_id: tc.call_id.clone(),
                                tool: tc.name.clone(),
                                state: ToolState::Pending { input, raw: None },
                            });
                            self.session_svc.update_part(pid, &part)?;
                        }
                    }
                }

                StreamEvent::StepFinish {
                    finish_reason: fr,
                    usage: u,
                } => {
                    finish_reason = fr;
                    usage = UsageInfo {
                        input_tokens: u.input_tokens,
                        output_tokens: u.output_tokens,
                        reasoning_tokens: 0,
                        cache_read_tokens: u.cache_read_tokens,
                        cache_creation_tokens: u.cache_creation_tokens,
                    };

                    // Flush text part
                    if let Some(ref pid) = text_part_id {
                        let part = Part::Text(TextPart {
                            content: text_content.clone(),
                        });
                        self.session_svc.update_part(pid, &part)?;
                    }

                    // Flush reasoning part
                    if let Some(ref pid) = reasoning_part_id {
                        let part = Part::Reasoning(ReasoningPart {
                            content: reasoning_content.clone(),
                            reasoning: None,
                        });
                        self.session_svc.update_part(pid, &part)?;
                    }

                    // Create step-finish part
                    self.session_svc.add_part(
                        session_id,
                        message_id,
                        &Part::StepFinish(StepFinishPart {
                            step_index,
                            finish_reason: format!("{:?}", finish_reason).to_lowercase(),
                            usage: usage.clone(),
                        }),
                    )?;
                    step_index += 1;
                }

                StreamEvent::Error(err) => {
                    warn!("LLM stream error: {err}");
                    return Err(anyhow::anyhow!("LLM error: {err}"));
                }
            }
        }

        Ok(ProcessResult {
            finish_reason,
            has_tool_calls,
            pending_tools: pending_tools
                .into_values()
                .map(|tc| PendingToolInfo {
                    call_id: tc.call_id,
                    name: tc.name,
                    arguments_json: tc.arguments_json,
                    part_id: tc.part_id,
                })
                .collect(),
            usage,
        })
    }

    /// Execute pending tool calls and update their parts.
    pub async fn execute_tools(
        &self,
        session_id: &str,
        message_id: &str,
        agent: &str,
        tools_info: Vec<PendingToolInfo>,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<Vec<ToolResultInfo>> {
        let mut results = Vec::new();

        for info in tools_info {
            let input: serde_json::Value =
                serde_json::from_str(&info.arguments_json).unwrap_or_default();

            let time_start = Utc::now().timestamp_millis();

            // Update part to running state
            if let Some(ref pid) = info.part_id {
                let part = Part::Tool(ToolPart {
                    call_id: info.call_id.clone(),
                    tool: info.name.clone(),
                    state: ToolState::Running {
                        input: input.clone(),
                        title: None,
                        metadata: None,
                        time_start,
                    },
                });
                self.session_svc.update_part(pid, &part)?;
            }

            // Execute the tool
            let tool_result = if let Some(tool) = self.tools.get(&info.name) {
                let ctx = ToolContext {
                    session_id: session_id.to_string(),
                    message_id: message_id.to_string(),
                    agent: agent.to_string(),
                    call_id: info.call_id.clone(),
                    cancel: cancel.clone(),
                    bus: self.bus.clone().map(Arc::new),
                    db: self.db.clone(),
                    project_dir: self.project_dir.clone(),
                    agent_runner: self.agent_runner.clone(),
                };
                tool.execute(input.clone(), &ctx).await
            } else {
                Err(anyhow::anyhow!("unknown tool: {}", info.name))
            };

            let time_end = Utc::now().timestamp_millis();

            // Update part to completed/error state
            let (output_text, is_error) = match tool_result {
                Ok(ToolOutput {
                    title,
                    output,
                    metadata,
                }) => {
                    if let Some(ref pid) = info.part_id {
                        let part = Part::Tool(ToolPart {
                            call_id: info.call_id.clone(),
                            tool: info.name.clone(),
                            state: ToolState::Completed {
                                input: input.clone(),
                                output: output.clone(),
                                title,
                                metadata,
                                time_start,
                                time_end,
                                attachments: None,
                            },
                        });
                        self.session_svc.update_part(pid, &part)?;
                    }
                    (output, false)
                }
                Err(e) => {
                    let error = e.to_string();
                    if let Some(ref pid) = info.part_id {
                        let part = Part::Tool(ToolPart {
                            call_id: info.call_id.clone(),
                            tool: info.name.clone(),
                            state: ToolState::Error {
                                input: input.clone(),
                                error: error.clone(),
                                metadata: None,
                                time_start,
                                time_end,
                            },
                        });
                        self.session_svc.update_part(pid, &part)?;
                    }
                    (error, true)
                }
            };

            results.push(ToolResultInfo {
                call_id: info.call_id,
                tool_name: info.name,
                output: output_text,
                is_error,
            });
        }

        Ok(results)
    }
}

/// Result of processing a stream.
pub struct ProcessResult {
    pub finish_reason: FinishReason,
    pub has_tool_calls: bool,
    pub pending_tools: Vec<PendingToolInfo>,
    pub usage: UsageInfo,
}

/// Info about a pending tool call.
pub struct PendingToolInfo {
    pub call_id: String,
    pub name: String,
    pub arguments_json: String,
    pub part_id: Option<String>,
}

/// Result of a tool execution.
pub struct ToolResultInfo {
    pub call_id: String,
    pub tool_name: String,
    pub output: String,
    pub is_error: bool,
}
