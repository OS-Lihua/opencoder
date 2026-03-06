//! Bash tool - execute shell commands.
//!
//! Mirrors `src/tool/bash.ts` from the original OpenCode.

use std::process::Stdio;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use crate::tool::{Tool, ToolContext, ToolOutput};
use crate::truncation;

const DEFAULT_TIMEOUT_MS: u64 = 120_000; // 2 minutes

pub struct BashTool;

#[async_trait::async_trait]
impl Tool for BashTool {
    fn id(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        include_str!("../../descriptions/bash.txt")
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["command", "description"],
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The bash command to execute"
                },
                "timeout": {
                    "type": "integer",
                    "description": "Timeout in milliseconds (default 120000)"
                },
                "description": {
                    "type": "string",
                    "description": "Clear, concise description of what this command does"
                }
            }
        })
    }

    async fn execute(
        &self,
        params: serde_json::Value,
        ctx: &ToolContext,
    ) -> anyhow::Result<ToolOutput> {
        let command = params["command"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'command' parameter"))?;
        let description = params["description"].as_str().unwrap_or("shell command");
        let timeout_ms = params["timeout"].as_u64().unwrap_or(DEFAULT_TIMEOUT_MS);

        // Determine shell
        let shell = if cfg!(target_os = "windows") {
            "cmd"
        } else {
            // Prefer zsh, fallback to bash
            if std::path::Path::new("/bin/zsh").exists() {
                "/bin/zsh"
            } else {
                "/bin/bash"
            }
        };

        let shell_arg = if cfg!(target_os = "windows") {
            "/C"
        } else {
            "-c"
        };

        let mut child = Command::new(shell)
            .arg(shell_arg)
            .arg(command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;

        // Read output with timeout
        let timeout = tokio::time::Duration::from_millis(timeout_ms);
        let cancel = ctx.cancel.clone();

        let result = tokio::select! {
            result = async {
                let mut stdout = String::new();
                let mut stderr = String::new();

                if let Some(ref mut out) = child.stdout {
                    out.read_to_string(&mut stdout).await.ok();
                }
                if let Some(ref mut err) = child.stderr {
                    err.read_to_string(&mut stderr).await.ok();
                }

                let status = child.wait().await?;
                Ok::<_, anyhow::Error>((stdout, stderr, status.code()))
            } => result,
            _ = tokio::time::sleep(timeout) => {
                child.kill().await.ok();
                Err(anyhow::anyhow!("command timed out after {}ms", timeout_ms))
            },
            _ = cancel.cancelled() => {
                child.kill().await.ok();
                Err(anyhow::anyhow!("command cancelled"))
            },
        };

        match result {
            Ok((stdout, stderr, exit_code)) => {
                let mut output = stdout;
                if !stderr.is_empty() {
                    if !output.is_empty() {
                        output.push('\n');
                    }
                    output.push_str(&stderr);
                }

                let truncated = truncation::truncate_default(&output);

                let exit_str = exit_code
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "signal".to_string());

                if exit_code.unwrap_or(-1) != 0 {
                    let meta = format!(
                        "\n<bash_metadata>\nexit_code: {exit_str}\n</bash_metadata>"
                    );
                    Ok(ToolOutput {
                        title: description.to_string(),
                        output: format!("{}{meta}", truncated.content),
                        metadata: serde_json::json!({
                            "exit_code": exit_code,
                            "description": description,
                        }),
                    })
                } else {
                    Ok(ToolOutput {
                        title: description.to_string(),
                        output: truncated.content,
                        metadata: serde_json::json!({
                            "exit_code": 0,
                            "description": description,
                        }),
                    })
                }
            }
            Err(e) => Ok(ToolOutput {
                title: description.to_string(),
                output: format!("Error: {e}"),
                metadata: serde_json::json!({ "error": e.to_string() }),
            }),
        }
    }
}
