# CORE CRATE

Foundation for all 14 other crates. Provides config, SQLite persistence, event bus, ID generation, environment, feature flags, skills, and commands.

## MODULES

| Module | File | Purpose |
|--------|------|---------|
| `bus` | `src/bus/mod.rs` | Tokio broadcast with ~20 `Event` variants. `Bus` (instance) + `GlobalBus` (cross-instance singleton) |
| `storage` | `src/storage/` | SQLite WAL wrapper. `Database.use_conn()`, `.transaction()`, `.effect()` for deferred bus publishes |
| `config` | `src/config/mod.rs` | JSONC config with hierarchical merge. Provider/agent/command overrides |
| `id` | `src/id/mod.rs` | `Identifier` — prefix + base62(timestamp) + counter. `ascending()` / `descending()` |
| `env` | `src/env/mod.rs` | `Env` — instance-scoped env overrides via DashMap. Falls back to `std::env` |
| `flag` | `src/flag/mod.rs` | 30+ `OPENCODE_*` feature flags. All runtime-checked, no compile gates |
| `skill` | `src/skill.rs` | Discovers `.md` skill files in `.opencode/skills/` with optional YAML frontmatter |
| `command` | `src/command.rs` | Built-in (`/init`, `/review`) + config-defined slash commands with `$ARGUMENTS` templates |
| `global` | `src/global/mod.rs` | XDG paths (`~/.config/opencode`, `~/.cache/opencode`). Cache version = "14" |

## KEY PATTERNS

**Transaction + Effects**: `Database.effect()` queues side-effects (bus publishes) to run only after transaction commits. Prevents events for rolled-back changes.

**Config Resolution** (lowest → highest priority):
1. `~/.config/opencode/opencode.json`
2. `OPENCODE_CONFIG` env var path
3. Walk up from project_dir to find `opencode.json`
4. `.opencode/opencode.json` in project root
5. `OPENCODE_CONFIG_CONTENT` env var (inline JSON)

**ID Prefixes**: `ses_` (session), `msg_` (message), `prt_` (part), `per_` (permission), `que_` (question), `pty_` (pty), `wrk_` (workspace), `prj_` (project).

## SCHEMA (7+ tables)

`project`, `workspace`, `session`, `message`, `part`, `todo`, `permission`, `session_share`, `control_account`. Messages/parts store typed data as JSON in `data` column. Migrations tracked in `_migrations` table, embedded at compile time.

## BUS EVENTS (key variants)

- `PartDelta { session_id, message_id, part_id, field, delta }` — streaming text to UI
- `PartUpdated` / `PartRemoved` — part lifecycle
- `PermissionAsked` / `PermissionReplied` — tool permission flow
- `QuestionAsked` / `QuestionReplied` — user Q&A flow
- `SessionStatus { session_id, status }` — Idle/Busy/Error
- `FileEdited` — post-edit notification

## WHEN MODIFYING

- Adding a new event → add variant to `Event` enum in `bus/mod.rs`, update serde tag
- Adding a table → add SQL to new migration file in `storage/migrations/`, add `Row` type to `schema.rs`
- Adding a feature flag → add accessor function to `flag/mod.rs` using `get_bool`/`get_string`
- Adding config field → extend `Config` struct, update `merge_opt!` macro usage
