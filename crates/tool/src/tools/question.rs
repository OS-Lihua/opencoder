//! Question tool: ask the user a question and wait for a response.
//!
//! When a Bus is available, publishes QuestionAsked events and waits for replies.
//! Otherwise falls back to stderr/stdin.

use anyhow::Result;

use crate::tool::{Tool, ToolContext, ToolOutput};

pub struct QuestionTool;

#[async_trait::async_trait]
impl Tool for QuestionTool {
    fn id(&self) -> &str {
        "question"
    }

    fn description(&self) -> &str {
        "Ask the user a question when you need clarification or input to proceed. Provide a clear question and optional answer choices. The user's response will be returned."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["question"],
            "properties": {
                "question": {
                    "type": "string",
                    "description": "The question to ask the user"
                },
                "options": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional list of answer choices"
                }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let question = params["question"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'question' parameter"))?;
        let options: Vec<String> = params["options"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        // If we have a Bus, use event-based flow
        if let Some(bus) = &ctx.bus {
            use opencoder_core::bus::Event;
            use opencoder_core::id::{Identifier, Prefix};

            let question_id = Identifier::create(Prefix::Question);
            let session_id = ctx.session_id.parse()
                .unwrap_or_else(|_| Identifier::create(Prefix::Session));

            bus.publish(Event::QuestionAsked {
                id: question_id.clone(),
                session_id: session_id.clone(),
            });

            // Wait for reply via bus subscription
            let mut rx = bus.subscribe();
            let timeout = tokio::time::Duration::from_secs(300); // 5 minutes

            let reply = tokio::select! {
                _ = tokio::time::sleep(timeout) => {
                    return Ok(ToolOutput {
                        title: "Question (timed out)".to_string(),
                        output: "User did not respond within 5 minutes.".to_string(),
                        metadata: serde_json::json!({ "timeout": true }),
                    });
                }
                _ = ctx.cancel.cancelled() => {
                    return Ok(ToolOutput {
                        title: "Question (cancelled)".to_string(),
                        output: "Question was cancelled.".to_string(),
                        metadata: serde_json::json!({ "cancelled": true }),
                    });
                }
                reply = async {
                    loop {
                        match rx.recv().await {
                            Ok(Event::QuestionReplied { id, .. }) if id == question_id => {
                                // The reply content would be carried by another mechanism
                                // For now, return a placeholder
                                break "User acknowledged.".to_string();
                            }
                            Err(_) => break "Bus closed.".to_string(),
                            _ => continue,
                        }
                    }
                } => reply,
            };

            return Ok(ToolOutput {
                title: "Question".to_string(),
                output: reply,
                metadata: serde_json::json!({ "question": question }),
            });
        }

        // Fallback: stdin/stderr for non-TUI mode
        let mut prompt = format!("\n--- Question ---\n{question}\n");
        if !options.is_empty() {
            prompt.push_str("Options:\n");
            for (i, opt) in options.iter().enumerate() {
                prompt.push_str(&format!("  {}. {opt}\n", i + 1));
            }
        }
        prompt.push_str("Your answer: ");

        eprint!("{prompt}");

        let mut answer = String::new();
        std::io::stdin().read_line(&mut answer)?;
        let answer = answer.trim().to_string();

        Ok(ToolOutput {
            title: "Question".to_string(),
            output: answer,
            metadata: serde_json::json!({ "question": question }),
        })
    }
}
