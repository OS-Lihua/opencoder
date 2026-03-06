//! Todo tools: manage a todo list for the current session.
//!
//! TodoWriteTool replaces the session's todo list.
//! TodoReadTool reads the current todo list.

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::tool::{Tool, ToolContext, ToolOutput};

/// A single todo item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub content: String,
    #[serde(default = "default_status")]
    pub status: String,
    #[serde(default)]
    pub priority: Option<String>,
}

fn default_status() -> String {
    "pending".to_string()
}

pub struct TodoWriteTool;

#[async_trait::async_trait]
impl Tool for TodoWriteTool {
    fn id(&self) -> &str {
        "todowrite"
    }

    fn description(&self) -> &str {
        "Write or update the session's todo list. Provide the complete list of todos. Each todo has content (description), status (pending/in_progress/completed), and optional priority (high/medium/low). This replaces the entire todo list."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["todos"],
            "properties": {
                "todos": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "required": ["content", "status"],
                        "properties": {
                            "content": { "type": "string", "description": "Todo description" },
                            "status": {
                                "type": "string",
                                "enum": ["pending", "in_progress", "completed"],
                                "description": "Todo status"
                            },
                            "priority": {
                                "type": "string",
                                "enum": ["high", "medium", "low"],
                                "description": "Optional priority"
                            }
                        }
                    },
                    "description": "The complete todo list"
                }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let todos: Vec<TodoItem> = serde_json::from_value(params["todos"].clone())
            .map_err(|e| anyhow::anyhow!("invalid todos format: {e}"))?;

        // Store todos in the database if available
        if let Some(db) = &ctx.db {
            db.use_conn(|conn| {
                // Create table if not exists
                conn.execute_batch(
                    "CREATE TABLE IF NOT EXISTS todo (
                        id INTEGER PRIMARY KEY AUTOINCREMENT,
                        session_id TEXT NOT NULL,
                        content TEXT NOT NULL,
                        status TEXT NOT NULL DEFAULT 'pending',
                        priority TEXT,
                        position INTEGER NOT NULL DEFAULT 0
                    )"
                )?;

                // Clear existing todos for this session
                conn.execute(
                    "DELETE FROM todo WHERE session_id = ?1",
                    [&ctx.session_id],
                )?;

                // Insert new todos
                for (i, todo) in todos.iter().enumerate() {
                    conn.execute(
                        "INSERT INTO todo (session_id, content, status, priority, position) VALUES (?1, ?2, ?3, ?4, ?5)",
                        rusqlite::params![ctx.session_id, todo.content, todo.status, todo.priority, i],
                    )?;
                }

                Ok(())
            })?;

            // Publish event if bus available
            if let Some(bus) = &ctx.bus {
                let session_id = ctx.session_id.parse().unwrap_or_else(|_| {
                    opencoder_core::id::Identifier::create(opencoder_core::id::Prefix::Session)
                });
                bus.publish(opencoder_core::bus::Event::TodoUpdated { session_id });
            }
        }

        let pending = todos.iter().filter(|t| t.status != "completed").count();
        let completed = todos.iter().filter(|t| t.status == "completed").count();

        let summary: Vec<String> = todos
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let icon = match t.status.as_str() {
                    "completed" => "[x]",
                    "in_progress" => "[~]",
                    _ => "[ ]",
                };
                let priority_str = t
                    .priority
                    .as_deref()
                    .map(|p| format!(" ({p})"))
                    .unwrap_or_default();
                format!("{}. {} {}{}", i + 1, icon, t.content, priority_str)
            })
            .collect();

        Ok(ToolOutput {
            title: format!("Todos ({pending} pending, {completed} done)"),
            output: summary.join("\n"),
            metadata: serde_json::json!({
                "total": todos.len(),
                "pending": pending,
                "completed": completed,
            }),
        })
    }
}

pub struct TodoReadTool;

#[async_trait::async_trait]
impl Tool for TodoReadTool {
    fn id(&self) -> &str {
        "todoread"
    }

    fn description(&self) -> &str {
        "Read the current session's todo list. Returns all todos with their status and priority."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn execute(&self, _params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput> {
        if let Some(db) = &ctx.db {
            let todos = db.use_conn(|conn| {
                // Check if table exists
                let table_exists: bool = conn.query_row(
                    "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='todo'",
                    [],
                    |row| row.get(0),
                )?;

                if !table_exists {
                    return Ok(Vec::new());
                }

                let mut stmt = conn.prepare(
                    "SELECT content, status, priority FROM todo WHERE session_id = ?1 ORDER BY position"
                )?;

                let todos: Vec<TodoItem> = stmt.query_map([&ctx.session_id], |row| {
                    Ok(TodoItem {
                        content: row.get(0)?,
                        status: row.get(1)?,
                        priority: row.get(2)?,
                    })
                })?.filter_map(|r| r.ok()).collect();

                Ok(todos)
            })?;

            if todos.is_empty() {
                return Ok(ToolOutput {
                    title: "Todos (empty)".to_string(),
                    output: "No todos found for this session.".to_string(),
                    metadata: serde_json::json!({ "total": 0 }),
                });
            }

            let output: Vec<String> = todos
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    let icon = match t.status.as_str() {
                        "completed" => "[x]",
                        "in_progress" => "[~]",
                        _ => "[ ]",
                    };
                    let priority_str = t
                        .priority
                        .as_deref()
                        .map(|p| format!(" ({p})"))
                        .unwrap_or_default();
                    format!("{}. {} {}{}", i + 1, icon, t.content, priority_str)
                })
                .collect();

            return Ok(ToolOutput {
                title: format!("Todos ({} items)", todos.len()),
                output: output.join("\n"),
                metadata: serde_json::json!({
                    "total": todos.len(),
                    "todos": todos,
                }),
            });
        }

        Ok(ToolOutput {
            title: "Todos".to_string(),
            output: "Todo storage not available (no database).".to_string(),
            metadata: serde_json::json!({ "error": "no_db" }),
        })
    }
}
