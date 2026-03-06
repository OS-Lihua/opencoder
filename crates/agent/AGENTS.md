# AGENT CRATE

Agent definitions, orchestration loop, and permission system. The brain that ties LLM streaming + tool execution + session persistence together.

## FILES

| File | Purpose |
|------|---------|
| `src/agent.rs` | `AgentDef` struct, `AgentRegistry`, 7 built-in agent definitions |
| `src/agent_loop.rs` | `run()` — core loop: user message → LLM stream → tool execution → loop. `AgentLoopConfig`. `SubAgentRunner` impl |
| `src/permission.rs` | Permission evaluation: last-match-wins with wildcard patterns. Default rules (read=Allow, write=Ask) |
| `src/prompts/*.txt` | System prompts loaded at compile time via `include_str!()` |
| `src/lib.rs` | Re-exports: `AgentDef`, `AgentRegistry`, `PermissionAction`, `PermissionRule` |

## 7 BUILT-IN AGENTS

| Agent | Model | Tools | Max Steps | Purpose |
|-------|-------|-------|-----------|---------|
| **build** | default | All | 200 | Primary coding agent |
| **plan** | default | read, glob, grep, question | 200 | Read-only analysis |
| **general** | default | All | 100 | Sub-agent for task tool |
| **explore** | small | read, glob, grep | 50 | Fast codebase search |
| **compaction** | small | None | 1 | Conversation summarizer (hidden) |
| **title** | small | None | 1 | Session title generator (hidden) |
| **summary** | small | None | 1 | Summary generator (hidden) |

## AGENT LOOP

```
run(config, user_content) → Result<()>
  1. add_message(User) to session
  2. create assistant message with agent metadata
  3. build_llm_messages() + system_prompt::build()
  4. provider.stream() → processor.process()
  5. if has_tool_calls:
       filter by permissions → execute_tools()
       maybe_compact() if context overflow
       → goto 3
  6. if no tool calls or max_steps reached → return
```

- `filter_tools()` applies agent's allowed/denied tools list
- `SubAgentRunner` implements `AgentRunner` trait (from tool crate) for task tool delegation
- Step counter prevents infinite loops
- `FinishReason::ToolUse` → continue, `Stop` → exit

## PERMISSION SYSTEM

**Evaluation**: Last-match-wins across rulesets.

```rust
PermissionRule { tool: String, pattern: Option<String>, action: PermissionAction }
// pattern: wildcard matching on tool arguments (e.g. "ls *")
// action: Allow, Deny, Ask

// Defaults: read/glob/grep → Allow. write/edit/bash → Ask.
```

Multiple rulesets merged (later overrides earlier): system defaults → agent rules → user config.

## WHEN MODIFYING

- Adding an agent → add to `builtin_agents()` in `agent.rs`, create system prompt in `prompts/`
- Changing tool filtering → `filter_tools()` in `agent_loop.rs`
- Permission defaults → `default_rules()` in `permission.rs`
- Agent config overrides → user's `config.agent` map merges with built-in defs in `AgentRegistry::with_config()`
