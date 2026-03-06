//! Task tool: run a sub-agent in a child session.

use anyhow::Result;

use crate::tool::{Tool, ToolContext, ToolOutput};

pub struct TaskTool;

#[async_trait::async_trait]
impl Tool for TaskTool {
    fn id(&self) -> &str {
        "task"
    }

    fn description(&self) -> &str {
        "Launch a sub-agent to handle a task autonomously. Provide a description and prompt for the task. The sub-agent runs in a child session and returns its results. Use this for delegating complex sub-tasks that can be handled independently."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["description", "prompt"],
            "properties": {
                "description": {
                    "type": "string",
                    "description": "Short description of the task (3-5 words)"
                },
                "prompt": {
                    "type": "string",
                    "description": "Detailed instructions for the sub-agent"
                },
                "agent": {
                    "type": "string",
                    "description": "Agent to use (default: general)",
                    "default": "general"
                }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let description = params["description"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'description' parameter"))?;
        let prompt = params["prompt"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'prompt' parameter"))?;
        let agent_name = params["agent"].as_str().unwrap_or("general");

        let runner = ctx.agent_runner.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "task tool requires agent_runner in context (not available in this execution mode)"
            )
        })?;

        let output = runner
            .run_sub_agent(prompt, agent_name, &ctx.session_id, ctx.cancel.clone())
            .await?;

        Ok(ToolOutput {
            title: format!("Task: {description}"),
            output,
            metadata: serde_json::json!({
                "description": description,
                "agent": agent_name,
                "status": "completed",
            }),
        })
    }
}
