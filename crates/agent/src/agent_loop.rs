//! Agent loop: LLM → tool execution → loop.
//!
//! Mirrors `src/session/prompt.ts` (loop + step) from the original OpenCode.
//! Orchestrates the conversation between the LLM and tools.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use opencoder_core::bus::{Bus, Event, SessionStatusInfo};
use opencoder_core::config::Config;
use opencoder_core::id::{Identifier, Prefix};
use opencoder_provider::models_db::ModelsDb;
use opencoder_provider::provider::{
    ChatMessage, ChatRequest, ContentPart, FinishReason, LlmProvider, Role, ToolDefinition,
};
use opencoder_session::compaction;
use opencoder_session::message::{AssistantMessage, Message, Part, UserMessage};
use opencoder_session::processor::{StreamProcessor, ToolResultInfo};
use opencoder_session::session::SessionService;
use opencoder_tool::tool::Tool;

use opencoder_tool::tool::AgentRunner;

use crate::agent::{AgentDef, AgentRegistry};

/// Configuration for an agent loop run.
pub struct AgentLoopConfig {
    pub session_id: String,
    pub project_id: String,
    pub agent_name: String,
    pub model: String,
    pub provider: Arc<dyn LlmProvider>,
    pub cancel: CancellationToken,
    pub project_dir: std::path::PathBuf,
    pub config: Config,
}

/// An AgentRunner implementation that can spawn sub-agent loops.
pub struct SubAgentRunner {
    pub session_svc: Arc<SessionService>,
    pub registry: Arc<AgentRegistry>,
    pub tools: HashMap<String, Arc<dyn Tool>>,
    pub provider: Arc<dyn LlmProvider>,
    pub bus: Bus,
    pub project_dir: std::path::PathBuf,
    pub config: Config,
    pub model: String,
}

#[async_trait::async_trait]
impl AgentRunner for SubAgentRunner {
    async fn run_sub_agent(
        &self,
        prompt: &str,
        agent_name: &str,
        parent_session_id: &str,
        cancel: CancellationToken,
    ) -> Result<String> {
        // Create a child session
        let project_dir_str = self.project_dir.to_string_lossy().to_string();
        let parent_session = self.session_svc.get(parent_session_id)?;

        let child_session =
            self.session_svc
                .create(&parent_session.project_id, &project_dir_str, None)?;

        let loop_config = AgentLoopConfig {
            session_id: child_session.id.clone(),
            project_id: parent_session.project_id.clone(),
            agent_name: agent_name.to_string(),
            model: self.model.clone(),
            provider: self.provider.clone(),
            cancel,
            project_dir: self.project_dir.clone(),
            config: self.config.clone(),
        };

        run(
            loop_config,
            prompt,
            self.session_svc.clone(),
            &self.registry,
            self.tools.clone(),
            &self.bus,
        )
        .await?;

        // Collect the final text output from the child session
        let messages = self.session_svc.messages(&child_session.id)?;
        let mut output = String::new();
        for msg in messages.iter().rev() {
            if let Message::Assistant(_) = &msg.message {
                for part in &msg.parts {
                    if let Part::Text(text) = &part.part
                        && !text.content.is_empty()
                    {
                        output = text.content.clone();
                        break;
                    }
                }
                if !output.is_empty() {
                    break;
                }
            }
        }

        if output.is_empty() {
            output = "[Sub-agent completed with no text output]".to_string();
        }

        Ok(output)
    }
}

/// Run the agent loop for a user message.
///
/// Flow:
/// 1. Add user message to session
/// 2. Create assistant message
/// 3. Stream LLM response, creating parts
/// 4. If tool calls → execute tools → add tool results → loop back to step 3
/// 5. If no tool calls (or max steps reached) → return
pub async fn run(
    config: AgentLoopConfig,
    user_content: &str,
    session_svc: Arc<SessionService>,
    registry: &AgentRegistry,
    tools: HashMap<String, Arc<dyn Tool>>,
    bus: &Bus,
) -> Result<()> {
    let agent_def = registry
        .get(&config.agent_name)
        .ok_or_else(|| anyhow::anyhow!("unknown agent: {}", config.agent_name))?;

    // Filter tools by agent permissions
    let available_tools = filter_tools(&tools, agent_def);

    // Add user message
    let user_msg = Message::User(UserMessage {
        content: user_content.to_string(),
        images: vec![],
    });
    session_svc.add_message(&config.session_id, &user_msg)?;

    // Publish busy status
    bus.publish(Event::SessionStatus {
        session_id: config
            .session_id
            .parse()
            .unwrap_or_else(|_| Identifier::create(Prefix::Session)),
        status: SessionStatusInfo::Busy,
    });

    let mut step = 0u32;
    let max_steps = agent_def.max_steps;

    loop {
        if config.cancel.is_cancelled() {
            info!("agent loop cancelled");
            break;
        }

        if step >= max_steps {
            warn!("max steps ({max_steps}) reached, stopping");
            break;
        }

        step += 1;
        debug!(step, agent = %config.agent_name, "agent step");

        // Create assistant message
        let assistant_msg = Message::Assistant(AssistantMessage {
            model: config.model.clone(),
            agent: config.agent_name.clone(),
            system: agent_def.system_prompt.clone(),
        });
        let msg_id = session_svc.add_message(&config.session_id, &assistant_msg)?;

        // Build messages for LLM
        let messages = build_llm_messages(
            &session_svc,
            &config.session_id,
            agent_def,
            &config.project_dir,
            &config.config,
        )?;

        // Build tool definitions
        let tool_defs: Vec<ToolDefinition> = available_tools
            .values()
            .map(|tool| ToolDefinition {
                name: tool.id().to_string(),
                description: tool.description().to_string(),
                parameters: tool.parameters_schema(),
            })
            .collect();

        // Create LLM request
        let mut request = ChatRequest::new(config.model.clone(), messages);
        request.tools = tool_defs;
        request.temperature = agent_def.temperature;
        request.top_p = agent_def.top_p;

        // Stream LLM response
        let stream = config
            .provider
            .stream(request, config.cancel.clone())
            .await?;

        let processor = StreamProcessor::new(session_svc.clone(), available_tools.clone());
        let result = processor
            .process(
                &config.session_id,
                &msg_id,
                &config.agent_name,
                stream,
                config.cancel.clone(),
            )
            .await?;

        // If there are tool calls, execute them and loop
        if result.has_tool_calls && !result.pending_tools.is_empty() {
            let tool_results = processor
                .execute_tools(
                    &config.session_id,
                    &msg_id,
                    &config.agent_name,
                    result.pending_tools,
                    config.cancel.clone(),
                )
                .await?;

            // Add tool result messages for the next LLM call
            add_tool_result_messages(&session_svc, &config.session_id, &tool_results)?;

            // Check for context overflow and run compaction if needed
            maybe_compact(&config, &session_svc, &result.usage.input_tokens).await;

            // Check finish reason — if tool_use, continue the loop
            if result.finish_reason == FinishReason::ToolUse {
                continue;
            }
        }

        // No tool calls or non-tool finish reason → done
        break;
    }

    // Publish idle status
    bus.publish(Event::SessionStatus {
        session_id: config
            .session_id
            .parse()
            .unwrap_or_else(|_| Identifier::create(Prefix::Session)),
        status: SessionStatusInfo::Idle,
    });

    Ok(())
}

/// Filter tools based on agent permissions.
fn filter_tools(
    tools: &HashMap<String, Arc<dyn Tool>>,
    agent_def: &AgentDef,
) -> HashMap<String, Arc<dyn Tool>> {
    tools
        .iter()
        .filter(|(name, _)| agent_def.can_use_tool(name))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// Build the LLM message history from session messages.
fn build_llm_messages(
    session_svc: &SessionService,
    session_id: &str,
    agent_def: &AgentDef,
    project_dir: &std::path::Path,
    config: &Config,
) -> Result<Vec<ChatMessage>> {
    let session_messages = session_svc.messages(session_id)?;
    let mut llm_messages = Vec::new();

    // Build system prompt with environment info, instruction files, etc.
    let system_parts =
        opencoder_session::system_prompt::build(&agent_def.system_prompt, project_dir, config);
    let system_text = system_parts.join("\n\n");
    llm_messages.push(ChatMessage::text(Role::System, &system_text));

    for msg_with_parts in &session_messages {
        match &msg_with_parts.message {
            Message::User(user) => {
                llm_messages.push(ChatMessage::text(Role::User, &user.content));
            }
            Message::Assistant(_) => {
                // Collect content parts from the assistant's parts
                let mut content_parts = Vec::new();

                for part_with_id in &msg_with_parts.parts {
                    match &part_with_id.part {
                        Part::Text(text) => {
                            if !text.content.is_empty() {
                                content_parts.push(ContentPart::Text {
                                    text: text.content.clone(),
                                });
                            }
                        }
                        Part::Tool(tool) => {
                            // Add tool_use content part
                            let input = match &tool.state {
                                opencoder_session::message::ToolState::Pending {
                                    input, ..
                                } => input.clone(),
                                opencoder_session::message::ToolState::Running {
                                    input, ..
                                } => input.clone(),
                                opencoder_session::message::ToolState::Completed {
                                    input, ..
                                } => input.clone(),
                                opencoder_session::message::ToolState::Error { input, .. } => {
                                    input.clone()
                                }
                            };
                            content_parts.push(ContentPart::ToolUse {
                                id: tool.call_id.clone(),
                                name: tool.tool.clone(),
                                input,
                            });
                        }
                        _ => {}
                    }
                }

                if !content_parts.is_empty() {
                    llm_messages.push(ChatMessage {
                        role: Role::Assistant,
                        content: content_parts,
                    });
                }

                // Add tool results as separate messages
                for part_with_id in &msg_with_parts.parts {
                    if let Part::Tool(tool) = &part_with_id.part {
                        match &tool.state {
                            opencoder_session::message::ToolState::Completed { output, .. } => {
                                llm_messages.push(ChatMessage::tool_result(
                                    &tool.call_id,
                                    output,
                                    false,
                                ));
                            }
                            opencoder_session::message::ToolState::Error { error, .. } => {
                                llm_messages.push(ChatMessage::tool_result(
                                    &tool.call_id,
                                    error,
                                    true,
                                ));
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    Ok(llm_messages)
}

/// Check for context overflow and run compaction if needed.
async fn maybe_compact(config: &AgentLoopConfig, session_svc: &SessionService, input_tokens: &u64) {
    // Only compact if auto-compaction is enabled (default: true)
    let auto = config
        .config
        .compaction
        .as_ref()
        .and_then(|c| c.auto)
        .unwrap_or(true);
    if !auto {
        return;
    }

    // Look up model context limit from ModelsDb
    let (provider_id, model_id) = opencoder_provider::init::parse_model_str(&config.model);
    let models_db = ModelsDb::load();
    let model_info = models_db.get(&provider_id, &model_id);

    let context_limit = model_info
        .as_ref()
        .and_then(|m| m.context_length)
        .unwrap_or(200_000);
    let max_output = model_info
        .as_ref()
        .and_then(|m| m.max_output)
        .unwrap_or(8_192);

    if compaction::is_overflow(*input_tokens, context_limit, max_output, &config.config) {
        info!(
            input_tokens,
            context_limit, "context overflow detected, running compaction"
        );
        if let Err(e) = compaction::process(
            &config.session_id,
            session_svc,
            &config.provider,
            &config.model,
        )
        .await
        {
            warn!("compaction failed: {e}");
        }
    }
}

/// Add tool result messages to the session for the next LLM call.
fn add_tool_result_messages(
    _session_svc: &SessionService,
    _session_id: &str,
    results: &[ToolResultInfo],
) -> Result<()> {
    for result in results {
        // Tool results are stored as user-role messages with tool_result content
        // They're already captured in the part state, so we just need them
        // in the message history for the next LLM call.
        // The build_llm_messages function reads tool state from parts.
        debug!(
            tool = %result.tool_name,
            call_id = %result.call_id,
            is_error = result.is_error,
            "tool result recorded"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::builtin_agents;

    #[test]
    fn filter_tools_for_plan_agent() {
        let agents = builtin_agents();
        let plan = &agents["plan"];

        let mut tools: HashMap<String, Arc<dyn Tool>> = HashMap::new();
        tools.insert("read".into(), Arc::new(MockTool("read")));
        tools.insert("bash".into(), Arc::new(MockTool("bash")));
        tools.insert("write".into(), Arc::new(MockTool("write")));
        tools.insert("glob".into(), Arc::new(MockTool("glob")));

        let filtered = filter_tools(&tools, plan);
        assert!(filtered.contains_key("read"));
        assert!(filtered.contains_key("glob"));
        assert!(!filtered.contains_key("bash"));
        assert!(!filtered.contains_key("write"));
    }

    struct MockTool(&'static str);

    #[async_trait::async_trait]
    impl Tool for MockTool {
        fn id(&self) -> &str {
            self.0
        }
        fn description(&self) -> &str {
            "mock"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        async fn execute(
            &self,
            _params: serde_json::Value,
            _ctx: &opencoder_tool::tool::ToolContext,
        ) -> anyhow::Result<opencoder_tool::tool::ToolOutput> {
            Ok(opencoder_tool::tool::ToolOutput {
                title: "mock".into(),
                output: "mock output".into(),
                metadata: serde_json::json!({}),
            })
        }
    }
}
