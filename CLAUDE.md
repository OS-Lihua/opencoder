# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

OpenCoder is an AI-powered coding agent written in Rust — a rewrite of [OpenCode](https://github.com/opencode-ai/opencode) (TypeScript/Bun). It's a Cargo workspace with 15 crates providing a CLI/TUI, HTTP API server, LLM provider abstraction, tool system, and agent loop.

## Commands

```bash
cargo test                              # all tests (144 currently)
cargo test -p opencoder-tool            # single crate tests
cargo test -p opencoder-core id::tests  # single test module

cargo check                             # type-check (use -j1 if build scripts get killed)
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check

cargo run -p opencoder-cli              # launch TUI (default)
cargo run -p opencoder-cli -- run "prompt"  # non-interactive single prompt
cargo run -p opencoder-cli -- serve     # headless HTTP API server (port 4096)
```

## Architecture

### Crate Dependency Graph (bottom-up)

```
core (config, storage/SQLite, bus, id, env, flag)
  ↑
provider (LlmProvider trait, Anthropic/OpenAI, SSE streaming, models DB)
patch (patch parsing/applying)
shell, file, lsp, mcp, snapshot, pty, project
  ↑
tool (Tool trait, 13 built-in tools, registry)
  ↑
session (SessionService, message/part types, stream processor, compaction)
  ↑
agent (AgentDef, AgentRegistry, agent_loop, permission system)
  ↑
server (axum HTTP API)
  ↑
cli (clap CLI, ratatui TUI, output streaming)
```

### Key Abstractions

**LlmProvider** (`provider/src/provider.rs`): Trait with `chat()` and `stream()` methods. Anthropic and OpenAI implementations exist; OpenAI-compatible providers (Groq, OpenRouter, etc.) reuse OpenAI with different base URLs. Factory: `provider/src/init.rs` parses `"provider/model"` strings.

**Tool** (`tool/src/tool.rs`): Trait with `id()`, `description()`, `parameters_schema()` (JSON Schema), `execute()`. Tools receive a `ToolContext` with session info, cancellation token, optional bus/db/project_dir/agent_runner. 13 tools registered in `tool/src/registry.rs`.

**AgentRunner** (`tool/src/tool.rs`): Trait for sub-agent execution, implemented as `SubAgentRunner` in `agent/src/agent_loop.rs`. Avoids circular dependency between tool and agent crates.

**Agent Loop** (`agent/src/agent_loop.rs`): The core orchestration loop — adds user message, streams LLM response via `StreamProcessor`, executes tool calls, checks for context overflow (compaction), loops until no more tool calls or max steps reached.

**StreamProcessor** (`session/src/processor.rs`): Converts `StreamEvent`s from the LLM into session `Part`s (Text, Tool, Reasoning, StepStart/StepFinish) stored in SQLite. Handles tool call argument buffering across chunks.

**Bus** (`core/src/bus/mod.rs`): Tokio broadcast channel with ~20 typed `Event` variants. Used for real-time UI updates (PartDelta for streaming text), permission ask/reply flow, session status notifications. `Database.effect()` defers bus publishes until after transaction commit.

**SessionService** (`session/src/session.rs`): CRUD for sessions, messages, and parts backed by SQLite. Messages contain typed `Part` enums serialized as JSON in the `data` column.

### Data Flow

```
User Input → AgentLoopConfig → agent_loop::run()
  → SessionService.add_message(User)
  → build system prompt (session/system_prompt.rs)
  → LlmProvider.stream() → StreamProcessor.process()
    → creates Text/Tool/Reasoning Parts in DB
    → publishes PartDelta events on Bus
  → StreamProcessor.execute_tools() for pending tool calls
  → maybe_compact() if context overflow
  → loop if finish_reason == ToolUse
```

### Model String Format

Models are specified as `"provider_id/model_id"` (e.g., `"anthropic/claude-sonnet-4-20250514"`). The `init::parse_model_str()` function splits this and infers provider from model name when no `/` is present.

### Agents

7 built-in agents defined in `agent/src/agent.rs`: **build** (primary, full tool access), **plan** (read-only), **general** (sub-agent for task tool), **explore** (fast search), **compaction**, **title**, **summary** (hidden utility agents). System prompts in `agent/src/prompts/*.txt`.

### ID System

`core/src/id/mod.rs`: Sortable IDs with 3-char prefix + base62 encoded timestamp + counter. `Identifier::descending()` produces reverse-chronological ordering for newest-first queries.
