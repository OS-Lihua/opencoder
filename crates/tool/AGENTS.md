# TOOL CRATE

13 built-in tools + `Tool` trait + `ToolRegistry`. Also defines `AgentRunner` trait to break circular dependency with agent crate.

## FILES

| File | Purpose |
|------|---------|
| `src/tool.rs` | `Tool` trait (`id`, `description`, `parameters_schema`, `execute`). `ToolContext` (session/message/agent/cancel + optional bus/db/project_dir/agent_runner). `ToolOutput`. `AgentRunner` trait |
| `src/registry.rs` | `ToolRegistry` — HashMap lookup. `with_builtins()` registers all 13 |
| `src/truncation.rs` | Output truncation utilities (line/byte limits) |
| `src/tools/*.rs` | One file per tool implementation |

## 13 BUILT-IN TOOLS

| Tool | File | Purpose | Key Params |
|------|------|---------|------------|
| `bash` | `bash.rs` | Shell command execution | command, timeout (120s default) |
| `read` | `read.rs` | Read files with line numbers | file_path, offset, limit (2000 lines) |
| `write` | `write.rs` | Create/overwrite files | file_path, content |
| `edit` | `edit.rs` | Targeted text replacement | file_path, old_string, new_string, replace_all |
| `glob` | `glob.rs` | Find files by pattern | pattern, path (100 results max) |
| `grep` | `grep.rs` | Regex content search | pattern, path, include (uses rg if available) |
| `apply_patch` | `apply_patch.rs` | Apply structured patches | patch (Begin/End Patch format) |
| `question` | `question.rs` | Ask user with choices | question, options (5min timeout, Bus events) |
| `webfetch` | `webfetch.rs` | Fetch URL content | url, format (text/markdown/html), timeout |
| `todo_write` | `todo.rs` | Write session todo list | todos (array of {content, status, priority}) |
| `todo_read` | `todo.rs` | Read session todo list | (none) |
| `multiedit` | `multiedit.rs` | Batch edits across files | edits (array of edit operations) |
| `task` | `task.rs` | Launch sub-agent | description, prompt, agent (default: "general") |

## TOOL TRAIT

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn id(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> serde_json::Value;  // JSON Schema
    async fn execute(&self, ctx: ToolContext, args: serde_json::Value) -> Result<ToolOutput>;
}
```

## ToolContext INJECTION

```rust
pub struct ToolContext {
    pub session_id: String,
    pub message_id: String,
    pub agent: String,
    pub call_id: String,
    pub cancel: CancellationToken,
    pub bus: Option<Bus>,
    pub db: Option<Arc<Database>>,
    pub project_dir: Option<PathBuf>,
    pub agent_runner: Option<Arc<dyn AgentRunner>>,
}
```

Optional fields enable tools to work in isolated contexts (testing) or full runtime.

## WHEN MODIFYING

- Adding a new tool → create `src/tools/new_tool.rs`, implement `Tool` trait, add `mod new_tool` to `src/tools/mod.rs`, register in `with_builtins()` in `registry.rs`
- Edit tool fallback chain: exact match → line-trimmed → whitespace-normalized
- `AgentRunner` trait lives here (not in agent crate) to avoid circular deps
- `task` tool uses `AgentRunner` to spawn sub-agents in child sessions
