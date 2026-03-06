# SESSION CRATE

Session/message CRUD, LLM stream processing, context compaction, system prompt construction. The persistence and streaming hub.

## FILES

| File | Purpose |
|------|---------|
| `src/session.rs` | `SessionService` — CRUD for sessions, messages, parts. `Session` struct with metadata. Fork support |
| `src/message.rs` | `Message` enum (User/Assistant). 11 `Part` types. `ToolState` lifecycle (Pending→Running→Completed/Error). `UsageInfo` tokens |
| `src/processor.rs` | `StreamProcessor` — converts `StreamEvent` stream → session `Part`s. Executes tools. Returns `ProcessResult` |
| `src/compaction.rs` | Overflow detection (`input_tokens >= context_limit - reserved`). Pruning (truncate tool outputs >40k). Full compaction via LLM summarization |
| `src/system_prompt.rs` | Builds system prompt: agent prompt + env block + instruction files (CLAUDE.md/AGENTS.md/CONTEXT.md walk-up) + config instructions |
| `src/share.rs` | Session sharing (URL generation, secret management) |
| `src/retry.rs` | Retry logic for transient LLM failures |
| `src/lib.rs` | Re-exports: `Session`, `SessionService`, `Message`, `MessageWithParts`, `Part`, `PartWithId` |

## 11 PART TYPES

| Part | Purpose |
|------|---------|
| `Text` | LLM text output (streamed via PartDelta) |
| `Reasoning` | Extended thinking / chain-of-thought |
| `Tool` | Tool invocation with state lifecycle |
| `StepStart` / `StepFinish` | LLM generation boundaries + usage |
| `Snapshot` | Git tree hash at point in time |
| `Patch` | Unified diff |
| `File` | File reference |
| `Agent` | Agent switch marker |
| `Retry` | Retry marker |
| `Compaction` | Summarization result |
| `Subtask` | Child session reference |

## TOOL STATE LIFECYCLE

```
Pending { input, raw }
  → Running { input, title, metadata, time_start }
    → Completed { input, output, title, metadata, time_start, time_end, attachments }
    → Error { input, error, metadata, time_start, time_end }
```

## STREAM PROCESSING FLOW

```
LlmProvider.stream() → StreamProcessor.process()
  TextDelta        → create/append Text part + publish PartDelta
  ReasoningDelta   → create/append Reasoning part
  ToolCallStart    → init pending call buffer
  ToolCallDelta    → accumulate JSON arguments string
  ToolCallEnd      → parse args → create Tool part (Pending)
  StepFinish       → flush parts → StepFinish part + usage

StreamProcessor.execute_tools(pending_tools)
  → update each Tool part: Pending → Running
  → Tool.execute(ToolContext) → ToolOutput
  → update: Running → Completed/Error
```

Tool arguments accumulate as raw JSON string across deltas, parsed only at `ToolCallEnd`.

## COMPACTION

- **Trigger**: `input_tokens >= (model_context_limit - reserved)`. Default reserved = min(max_output, 20000).
- **Pruning first**: Truncates tool outputs >40k chars in older messages. Keeps last 2 user turns untouched.
- **Full compaction**: Calls LLM to summarize conversation. Adds `CompactionPart` recording summary + message count.

## SYSTEM PROMPT COMPOSITION

Order: agent prompt → `<env>` block (platform, date, git branch) → instruction files (walk up dirs: CLAUDE.md, AGENTS.md, CONTEXT.md) → config `instructions` array.

## WHEN MODIFYING

- Adding a Part type → add variant to `Part` enum in `message.rs`, update serde tag, handle in `processor.rs`
- Changing compaction → `compaction.rs`: `is_overflow()` for detection, `prune()` for truncation, `process()` for full summarization
- System prompt sources → `system_prompt.rs`: add file patterns to the walk-up loop
- IDs: sessions use `descending()` (newest first), messages/parts use `ascending()` (oldest first)
