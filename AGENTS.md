# PROJECT KNOWLEDGE BASE

**Generated:** 2026-03-06 · **Commit:** e60ae6d · **Branch:** master

## OVERVIEW

OpenCoder — AI coding agent in Rust. Cargo workspace (15 crates, ~14.8k LOC) providing CLI/TUI, HTTP API, multi-provider LLM abstraction, 13 tools, 7 agents, and SQLite-backed session persistence. Rust rewrite of TypeScript [OpenCode](https://github.com/opencode-ai/opencode).

## STRUCTURE

```
opencoder/
├── crates/
│   ├── core/       # Foundation: config, SQLite, event bus, IDs, env, flags
│   ├── provider/   # LlmProvider trait + Anthropic/OpenAI impls + SSE streaming
│   ├── tool/       # Tool trait + 13 built-in tools + registry
│   ├── session/    # Session/message CRUD, stream processor, compaction
│   ├── agent/      # 7 agent defs, agent loop orchestration, permissions
│   ├── server/     # axum REST API (headless mode)
│   ├── cli/        # clap CLI + ratatui TUI (entry point)
│   ├── file/       # File detection, listing, watcher, auto-formatting
│   ├── patch/      # Unified diff parsing and application
│   ├── project/    # Project CRUD and discovery
│   ├── lsp/        # LSP client integration
│   ├── mcp/        # Model Context Protocol client
│   ├── snapshot/   # Git-backed file snapshots
│   ├── shell/      # Shell detection and process management
│   └── pty/        # Pseudo-terminal session management
├── scripts/        # build.sh, version-bump.sh, download_models.sh
├── Makefile        # make test/lint/fmt/build/dev/serve/run/release-*
└── CLAUDE.md       # AI assistant guidance (architecture reference)
```

## ARCHITECTURAL LAYERS

```
PRESENTATION    cli (TUI/CLI entry)  ·  server (axum REST API)
                            ↑
ORCHESTRATION   agent (agent loop, permissions)  ·  session (messages, streaming, compaction)
                            ↑
EXECUTION       tool (13 tools, registry)  ·  provider (LLM abstraction)  ·  mcp
                            ↑
UTILITY         file · patch · lsp · snapshot · project · shell · pty
                            ↑
FOUNDATION      core (config, SQLite, bus, IDs, env, flags, skills, commands)
```

Strict unidirectional dependencies. No circular deps. `AgentRunner` trait in tool crate breaks the tool↔agent cycle.

## WHERE TO LOOK

| Task | Location | Notes |
|------|----------|-------|
| Add new tool | `crates/tool/src/tools/` + register in `registry.rs` | Implement `Tool` trait |
| Add LLM provider | `crates/provider/src/providers/` + factory in `init.rs` | Implement `LlmProvider` trait; OpenAI-compatible → use `new_compatible()` |
| Add/modify agent | `crates/agent/src/agent.rs` | Define `AgentDef` with tools/permissions/prompt |
| Agent system prompts | `crates/agent/src/prompts/*.txt` | Loaded at compile time |
| Change agent loop | `crates/agent/src/agent_loop.rs` | Core orchestration: LLM stream → tool exec → loop |
| Stream processing | `crates/session/src/processor.rs` | StreamEvent → Part conversion + tool execution |
| Message/part types | `crates/session/src/message.rs` | 11 Part types, ToolState lifecycle |
| Context compaction | `crates/session/src/compaction.rs` | Overflow detection, pruning, summarization |
| System prompt build | `crates/session/src/system_prompt.rs` | Walks dirs for CLAUDE.md/AGENTS.md/CONTEXT.md |
| Config structure | `crates/core/src/config/mod.rs` | Multi-level merge: global → env → project → .opencode/ |
| Event bus | `crates/core/src/bus/mod.rs` | ~20 Event variants, Tokio broadcast |
| Database/schema | `crates/core/src/storage/` | SQLite WAL, 7+ tables, migration system |
| ID generation | `crates/core/src/id/mod.rs` | Prefix + base62 timestamp + counter |
| Feature flags | `crates/core/src/flag/mod.rs` | 30+ `OPENCODE_*` env var flags |
| HTTP API routes | `crates/server/src/routes/` | Session, project, config, provider, SSE events |
| TUI screens | `crates/cli/src/tui/screens/` | Home (session list), Session (conversation) |
| CLI commands | `crates/cli/src/main.rs` | serve, run, models, sessions, project, version |
| Dependency policy | `deny.toml` | License allowlist, vulnerability deny, no wildcards |
| CI pipeline | `.github/workflows/ci.yml` | fmt, clippy(3 OS), test(3 OS), MSRV 1.85, coverage |
| Release process | `.github/workflows/release.yml` | 6-target build, checksums, crates.io publish |

## DATA FLOW

```
User Input → AgentLoopConfig → agent_loop::run()
  → SessionService.add_message(User)
  → system_prompt::build() — walks dirs for instruction files
  → LlmProvider.stream() → StreamProcessor.process()
    → TextDelta/ToolCallDelta → Part stored in SQLite
    → PartDelta events on Bus → TUI/API clients update
  → StreamProcessor.execute_tools() — pending ToolParts
    → Tool.execute(ToolContext) → ToolOutput
    → Part updated: Pending → Running → Completed/Error
  → maybe_compact() if input_tokens >= context_limit - reserved
  → loop if finish_reason == ToolUse
```

## CONVENTIONS

- **Model strings**: `"provider/model"` format (e.g. `"anthropic/claude-sonnet-4-20250514"`). Parsed by `provider/src/init.rs`.
- **Error handling**: `anyhow::Result<T>` everywhere. `thiserror` only for domain errors needing matching (`IdError`, `NotFoundError`). Always chain `.context()`.
- **IDs**: Sortable with prefix. `descending()` for sessions (newest first), `ascending()` for messages/parts.
- **Bus pattern**: `Database.effect()` defers bus publishes until after transaction commit.
- **Tests**: All inline `#[cfg(test)] mod tests`. No integration test directory. Standard assertions only.
- **Config**: JSONC support (comments allowed). Resolution: global → `OPENCODE_CONFIG` → project walk-up → `.opencode/` → `OPENCODE_CONFIG_CONTENT`.
- **Formatting**: Rust defaults (no rustfmt.toml). `cargo fmt --all` enforced in CI.
- **Linting**: `cargo clippy --all-targets -- -D warnings` (all warnings are errors).
- **Edition**: 2024, MSRV 1.85.0.
- **Tool context injection**: Tools receive `ToolContext` with optional `bus`, `db`, `project_dir`, `agent_runner` — avoids hard dependencies on runtime services.
- **Leaf crate independence**: file, patch, lsp, snapshot, project, shell, pty depend only on core — usable standalone.

## ANTI-PATTERNS

- **No circular crate dependencies** — `AgentRunner` trait in tool crate is the boundary.
- **No `unwrap()` in non-test code** — use `?` with `.context()`.
- **No wildcard dependencies** — deny.toml enforces.
- **No unlicensed crates** — allowlist: MIT, Apache-2.0, BSD, ISC, Zlib, CC0, OpenSSL, BSL.
- **No synchronous blocking** — all I/O is async/tokio.
- **No version mismatches** — workspace-level versioning, release-please auto-bumps all crates.

## COMMANDS

```bash
# Development
make dev                    # TUI mode
make serve                  # HTTP API (port 4096)
make run PROMPT='...'       # Single prompt
cargo test                  # All 144 tests
cargo test -p opencoder-tool  # Single crate

# Quality
make lint                   # clippy + fmt check
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
cargo deny check            # License + security audit

# Release
make release-patch          # version bump + commit + tag + push
./scripts/build.sh --target aarch64-apple-darwin --release
./scripts/download_models.sh  # Update embedded model DB before release
```

## CRATE PUBLISH ORDER

Crates.io publish must follow dependency order with 20s propagation delay:
```
core → patch → shell → pty → provider → file → lsp → mcp → snapshot → project → tool → session → agent → server → cli
```

## NOTES

- `cli` depends on `server` (for `opencoder serve` mode) — they share the same backend.
- Tool tool arguments accumulate as JSON string across `ToolCallDelta` events, parsed only at `ToolCallEnd`.
- Compaction keeps last 2 user turns untouched; truncates tool outputs >40k chars in older messages.
- System prompt walks up from project_dir to filesystem root collecting CLAUDE.md/AGENTS.md/CONTEXT.md.
- Nightly builds run security audit (rustsec) + dependency freshness (cargo-outdated) daily.
- Cache version ("14") — cache wiped on version mismatch to prevent stale data.
