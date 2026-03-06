# OpenCoder

An AI-powered coding agent for the terminal, written in Rust.

OpenCoder is a Rust rewrite of [OpenCode](https://github.com/opencode-ai/opencode). It provides a terminal UI, a headless HTTP API, and a non-interactive CLI mode for running AI-assisted coding tasks.

## Features

- **Terminal UI** — Full TUI built with [ratatui](https://github.com/ratatui/ratatui), with session management, real-time streaming, and tool execution display
- **13 Built-in Tools** — bash, read, write, edit, glob, grep, apply_patch, multiedit, question, webfetch, todo, task
- **7 Agents** — build (primary), plan (read-only), general, explore, compaction, title, summary
- **Multi-provider LLM** — Anthropic, OpenAI, and OpenAI-compatible providers (Groq, OpenRouter, Together, Fireworks, DeepSeek, Mistral, xAI, etc.)
- **HTTP API Server** — Headless mode with REST API for sessions, messages, projects, and config
- **Context Compaction** — Automatic conversation summarization when approaching token limits
- **Session Persistence** — SQLite-backed storage for sessions, messages, and tool execution history
- **Auto-formatting** — Detects and runs formatters (rustfmt, prettier, gofmt, black, etc.) on edited files
- **Instruction Files** — Reads CLAUDE.md / AGENTS.md / CONTEXT.md from project directories

## Quick Start

### Prerequisites

- Rust 1.87+ (edition 2024, uses let-chains)
- An API key for at least one LLM provider

### Install

```bash
# From source
cargo install --path crates/cli

# Or build manually
cargo build --release
# Binary at: target/release/opencoder
```

### Set up API key

```bash
# Pick one (or more)
export ANTHROPIC_API_KEY=sk-ant-...
export OPENAI_API_KEY=sk-...
export GROQ_API_KEY=gsk_...
```

### Run

```bash
# Launch the TUI (default)
opencoder

# Non-interactive single prompt
opencoder run "implement a fibonacci function in src/lib.rs"

# Specify model and agent
opencoder run "review this code" --model anthropic/claude-sonnet-4-20250514 --agent plan

# Start headless API server
opencoder serve --port 4096

# Other commands
opencoder models      # list configured models
opencoder sessions    # list sessions for current project
opencoder project     # show project info
opencoder version     # show version
```

## Configuration

OpenCoder reads config from `.opencode/config.json` in your project directory or `~/.config/opencoder/config.json` globally.

```jsonc
{
  "model": "anthropic/claude-sonnet-4-20250514",
  "small_model": "anthropic/claude-haiku-4-20250514",
  "instructions": ["Always write tests for new code"],
  "compaction": {
    "auto": true,
    "reserved": 20000
  }
}
```

### Model format

Models are specified as `provider/model-id`:

| Provider | Example |
|----------|---------|
| Anthropic | `anthropic/claude-sonnet-4-20250514` |
| OpenAI | `openai/gpt-4o` |
| Groq | `groq/llama-3.1-70b-versatile` |
| OpenRouter | `openrouter/anthropic/claude-3.5-sonnet` |
| DeepSeek | `deepseek/deepseek-chat` |
| Mistral | `mistral/mistral-large-latest` |
| xAI | `xai/grok-2` |

## Architecture

OpenCoder is a Cargo workspace with 15 crates:

```
crates/
├── core/       # Config, SQLite storage, event bus, ID generation
├── provider/   # LlmProvider trait, Anthropic/OpenAI, SSE streaming
├── tool/       # Tool trait, 13 built-in tools, registry
├── session/    # Session/message management, stream processor, compaction
├── agent/      # Agent definitions, agent loop, permission system
├── server/     # axum HTTP API
├── cli/        # clap CLI + ratatui TUI
├── file/       # File detection, listing, watching, auto-formatting
├── patch/      # Patch parsing and application
├── project/    # Project CRUD and discovery
├── lsp/        # LSP client integration
├── mcp/        # Model Context Protocol client
├── snapshot/   # Git-backed file snapshots
├── shell/      # Shell detection and process management
└── pty/        # Pseudo-terminal session management
```

### Data flow

```
User Input → Agent Loop → LLM Provider (streaming)
  → Stream Processor → Parts stored in SQLite
  → Tool Execution → Results fed back to LLM
  → Bus Events → TUI / API clients updated in real-time
```

## Development

```bash
# Run tests
cargo test

# Type check
cargo check

# Lint
cargo clippy --all-targets -- -D warnings

# Format
cargo fmt --all

# Run in dev mode (TUI)
cargo run -p opencoder-cli

# Run a single crate's tests
cargo test -p opencoder-tool

# Build release binary
./scripts/build.sh --release
```

See the [Makefile](Makefile) for additional shortcuts.

## API Server

When running in headless mode (`opencoder serve`), the following endpoints are available:

| Method | Path | Description |
|--------|------|-------------|
| GET | `/session` | List sessions |
| POST | `/session` | Create session |
| GET | `/session/:id` | Get session |
| DELETE | `/session/:id` | Delete session |
| GET | `/session/:id/messages` | Get messages |
| POST | `/session/:id/messages` | Send message (triggers agent loop) |
| POST | `/session/:id/share` | Share session |
| DELETE | `/session/:id/share` | Unshare session |
| POST | `/session/:id/fork` | Fork session |
| GET | `/project` | List projects |
| GET | `/config` | Get config |
| GET | `/provider` | List providers |
| GET | `/health` | Health check |
| GET | `/events` | SSE event stream |

## License

MIT
