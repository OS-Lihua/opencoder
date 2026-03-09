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
use opencoder_core::storage::Database;
use opencoder_provider::models_db::ModelsDb;
use opencoder_provider::provider::{
    ChatMessage, ChatRequest, ContentPart, FinishReason, LlmProvider, Role, ToolDefinition,
};
use opencoder_session::compaction;
use opencoder_session::message::{
    AssistantMessage, Message, Part, ToolPart, ToolState, UserMessage,
};
use opencoder_session::processor::{PendingToolInfo, StreamProcessor, ToolResultInfo};
use opencoder_session::session::SessionService;
use opencoder_snapshot::SnapshotStore;
use opencoder_tool::tool::Tool;

use opencoder_tool::tool::AgentRunner;

use crate::agent::{AgentDef, AgentRegistry, PermissionAction, PermissionRule};
use crate::permission::{self, Ruleset};

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
    pub db: Arc<Database>,
    pub snapshot_store: Option<Arc<SnapshotStore>>,
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
    pub db: Arc<Database>,
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
            db: self.db.clone(),
            snapshot_store: None,
        };

        run(
            loop_config,
            prompt,
            self.session_svc.clone(),
            self.registry.clone(),
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
    registry: Arc<AgentRegistry>,
    tools: HashMap<String, Arc<dyn Tool>>,
    bus: &Bus,
) -> Result<()> {
    let agent_def = registry
        .get(&config.agent_name)
        .ok_or_else(|| anyhow::anyhow!("unknown agent: {}", config.agent_name))?
        .clone();

    // Filter tools by agent permissions
    let available_tools = filter_tools(&tools, &agent_def);

    // Create sub-agent runner for tools that need to spawn sub-agents
    let sub_agent_runner: Arc<dyn AgentRunner> = Arc::new(SubAgentRunner {
        session_svc: session_svc.clone(),
        registry: registry.clone(),
        tools: tools.clone(),
        provider: config.provider.clone(),
        bus: bus.clone(),
        project_dir: config.project_dir.clone(),
        config: config.config.clone(),
        model: config.model.clone(),
        db: config.db.clone(),
    });

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

    // Session-scoped permission rules (for "Always Allow")
    let mut session_rules: Ruleset = Vec::new();

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
            &agent_def,
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

        // Track snapshot before streaming
        let snapshot_hash = if let Some(ref store) = config.snapshot_store {
            let s = store.clone();
            tokio::task::spawn_blocking(move || s.track().ok())
                .await
                .ok()
                .flatten()
        } else {
            None
        };

        // Stream LLM response
        let stream = config
            .provider
            .stream(request, config.cancel.clone())
            .await?;

        let processor = StreamProcessor::new(
            session_svc.clone(),
            available_tools.clone(),
            Some(bus.clone()),
            Some(config.db.clone()),
            Some(config.project_dir.clone()),
            Some(sub_agent_runner.clone()),
        );
        let result = processor
            .process(
                &config.session_id,
                &msg_id,
                &config.agent_name,
                stream,
                config.cancel.clone(),
                snapshot_hash,
            )
            .await?;

        // If there are tool calls, check permissions then execute
        if result.has_tool_calls && !result.pending_tools.is_empty() {
            let default_rules = permission::default_rules();
            // Snapshot session_rules for evaluation; check_permissions may add new rules
            let session_rules_snapshot = session_rules.clone();
            let rulesets: Vec<&Ruleset> = vec![
                &default_rules,
                &agent_def.permission_rules,
                &session_rules_snapshot,
            ];

            // Partition tools into allowed and those needing permission
            let (allowed, denied_results) = check_permissions(
                result.pending_tools,
                &rulesets,
                &config.session_id,
                bus,
                &session_svc,
                &config.cancel,
                &mut session_rules,
            )
            .await;

            let mut tool_results: Vec<ToolResultInfo> = denied_results;

            if !allowed.is_empty() {
                let mut executed = processor
                    .execute_tools(
                        &config.session_id,
                        &msg_id,
                        &config.agent_name,
                        allowed,
                        config.cancel.clone(),
                    )
                    .await?;
                tool_results.append(&mut executed);
            }

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

    // Auto-generate title for build agent if still default
    if config.agent_name == "build"
        && let Ok(session) = session_svc.get(&config.session_id)
        && session.title == "New Session"
    {
        let svc = session_svc.clone();
        let provider = config.provider.clone();
        let model = config.model.clone();
        let sid = config.session_id.clone();
        tokio::spawn(async move {
            if let Err(e) = generate_title(&sid, &svc, &provider, &model).await {
                warn!("title generation failed: {e}");
            }
        });
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

/// Check permissions for a set of pending tool calls.
///
/// Returns (allowed_tools, denied_results): tools that passed permission checks,
/// and synthetic error results for tools that were denied.
async fn check_permissions(
    pending: Vec<PendingToolInfo>,
    rulesets: &[&Ruleset],
    session_id: &str,
    bus: &Bus,
    session_svc: &SessionService,
    cancel: &CancellationToken,
    session_rules: &mut Ruleset,
) -> (Vec<PendingToolInfo>, Vec<ToolResultInfo>) {
    let mut allowed = Vec::new();
    let mut denied_results = Vec::new();

    for info in pending {
        let pattern = extract_permission_pattern(&info.name, &info.arguments_json);
        let action = permission::evaluate(&info.name, &pattern, rulesets);

        match action {
            PermissionAction::Allow => {
                allowed.push(info);
            }
            PermissionAction::Deny => {
                // Update part to error state
                if let Some(ref pid) = info.part_id {
                    let input: serde_json::Value =
                        serde_json::from_str(&info.arguments_json).unwrap_or_default();
                    let now = chrono::Utc::now().timestamp_millis();
                    let part = opencoder_session::message::Part::Tool(ToolPart {
                        call_id: info.call_id.clone(),
                        tool: info.name.clone(),
                        state: ToolState::Error {
                            input,
                            error: "Permission denied".to_string(),
                            metadata: None,
                            time_start: now,
                            time_end: now,
                        },
                    });
                    let _ = session_svc.update_part(pid, &part);
                }
                denied_results.push(ToolResultInfo {
                    call_id: info.call_id,
                    tool_name: info.name,
                    output: "Permission denied by policy.".to_string(),
                    is_error: true,
                });
            }
            PermissionAction::Ask => {
                let perm_id = Identifier::create(Prefix::Permission);
                let sid: opencoder_core::id::SessionId = session_id
                    .parse()
                    .unwrap_or_else(|_| Identifier::create(Prefix::Session));

                bus.publish(Event::PermissionAsked {
                    id: perm_id.clone(),
                    session_id: sid.clone(),
                    tool_name: info.name.clone(),
                    description: pattern.clone(),
                });

                // Wait for reply
                let mut rx = bus.subscribe();
                let timeout = tokio::time::Duration::from_secs(300);

                let reply = tokio::select! {
                    _ = tokio::time::sleep(timeout) => "deny".to_string(),
                    _ = cancel.cancelled() => "deny".to_string(),
                    reply = async {
                        loop {
                            match rx.recv().await {
                                Ok(Event::PermissionReplied { request_id, reply, .. })
                                    if request_id == perm_id =>
                                {
                                    break reply;
                                }
                                Err(_) => break "deny".to_string(),
                                _ => continue,
                            }
                        }
                    } => reply,
                };

                match reply.as_str() {
                    "allow" => {
                        allowed.push(info);
                    }
                    "always" => {
                        // Persist as a session-scoped rule for future calls
                        session_rules.push(PermissionRule {
                            tool: info.name.clone(),
                            pattern: None,
                            action: PermissionAction::Allow,
                        });
                        allowed.push(info);
                    }
                    _ => {
                        // Denied
                        if let Some(ref pid) = info.part_id {
                            let input: serde_json::Value =
                                serde_json::from_str(&info.arguments_json).unwrap_or_default();
                            let now = chrono::Utc::now().timestamp_millis();
                            let part = opencoder_session::message::Part::Tool(ToolPart {
                                call_id: info.call_id.clone(),
                                tool: info.name.clone(),
                                state: ToolState::Error {
                                    input,
                                    error: "Permission denied by user".to_string(),
                                    metadata: None,
                                    time_start: now,
                                    time_end: now,
                                },
                            });
                            let _ = session_svc.update_part(pid, &part);
                        }
                        denied_results.push(ToolResultInfo {
                            call_id: info.call_id,
                            tool_name: info.name,
                            output: "Permission denied by user.".to_string(),
                            is_error: true,
                        });
                    }
                }
            }
        }
    }

    (allowed, denied_results)
}

/// Extract a pattern string from tool arguments for permission matching.
fn extract_permission_pattern(tool_name: &str, arguments_json: &str) -> String {
    let parsed: serde_json::Value = serde_json::from_str(arguments_json).unwrap_or_default();
    match tool_name {
        "bash" => parsed["command"].as_str().unwrap_or("*").to_string(),
        "write" | "edit" | "read" => parsed["file_path"].as_str().unwrap_or("*").to_string(),
        _ => "*".to_string(),
    }
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

/// Generate a title for a session using the title agent's system prompt.
async fn generate_title(
    session_id: &str,
    session_svc: &SessionService,
    provider: &Arc<dyn LlmProvider>,
    model: &str,
) -> Result<()> {
    let messages = session_svc.messages(session_id)?;

    // Collect conversation context (first ~1000 chars)
    let mut context = String::new();
    for msg in &messages {
        match &msg.message {
            Message::User(u) => {
                context.push_str("User: ");
                context.push_str(&u.content);
                context.push('\n');
            }
            Message::Assistant(_) => {
                for p in &msg.parts {
                    if let Part::Text(t) = &p.part {
                        context.push_str("Assistant: ");
                        context.push_str(&t.content);
                        context.push('\n');
                    }
                }
            }
        }
        if context.len() > 1000 {
            break;
        }
    }

    if context.is_empty() {
        return Ok(());
    }

    let system_prompt = include_str!("prompts/title.txt");
    let request = ChatRequest::new(
        model.to_string(),
        vec![
            ChatMessage::text(Role::System, system_prompt),
            ChatMessage::text(Role::User, &context),
        ],
    );

    let cancel = CancellationToken::new();
    let response = provider.chat(request, cancel).await?;
    let title = response.content.trim().to_string();

    if !title.is_empty() {
        session_svc.set_title(session_id, &title)?;
    }

    Ok(())
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
