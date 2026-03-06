//! Feature flags via environment variables.
//!
//! Mirrors `src/flag/flag.ts` from the original OpenCode.
//! All flags are read from `OPENCODE_*` environment variables at runtime.


/// Read a boolean flag from environment. Truthy: "1", "true", "yes".
pub fn get_bool(key: &str) -> bool {
    std::env::var(key)
        .map(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

/// Read a string flag from environment.
pub fn get_string(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|s| !s.is_empty())
}

/// Read a numeric flag from environment.
pub fn get_i64(key: &str) -> Option<i64> {
    std::env::var(key).ok()?.parse().ok()
}

// ---- Feature flag accessors ----
// Each mirrors a flag from the original OpenCode

pub fn disable_autoupdate() -> bool {
    get_bool("OPENCODE_DISABLE_AUTOUPDATE")
}

pub fn disable_share() -> bool {
    get_bool("OPENCODE_DISABLE_SHARE")
}

pub fn disable_mcp() -> bool {
    get_bool("OPENCODE_DISABLE_MCP")
}

pub fn disable_lsp() -> bool {
    get_bool("OPENCODE_DISABLE_LSP")
}

pub fn disable_format() -> bool {
    get_bool("OPENCODE_DISABLE_FORMAT")
}

pub fn disable_watcher() -> bool {
    get_bool("OPENCODE_DISABLE_WATCHER")
}

pub fn disable_snapshot() -> bool {
    get_bool("OPENCODE_DISABLE_SNAPSHOT")
}

pub fn disable_instructions() -> bool {
    get_bool("OPENCODE_DISABLE_INSTRUCTIONS")
}

pub fn disable_compaction() -> bool {
    get_bool("OPENCODE_DISABLE_COMPACTION")
}

pub fn disable_auto_compaction() -> bool {
    get_bool("OPENCODE_DISABLE_AUTO_COMPACTION")
}

pub fn disable_prune() -> bool {
    get_bool("OPENCODE_DISABLE_PRUNE")
}

pub fn disable_question() -> bool {
    get_bool("OPENCODE_DISABLE_QUESTION")
}

pub fn enable_exa() -> bool {
    get_bool("OPENCODE_ENABLE_EXA")
}

pub fn enable_experimental() -> bool {
    get_bool("OPENCODE_ENABLE_EXPERIMENTAL")
}

pub fn enable_lsp_tool() -> bool {
    get_bool("OPENCODE_ENABLE_LSP_TOOL")
}

pub fn enable_batch_tool() -> bool {
    get_bool("OPENCODE_ENABLE_BATCH_TOOL")
}

pub fn experimental_output_token_max() -> Option<i64> {
    get_i64("OPENCODE_EXPERIMENTAL_OUTPUT_TOKEN_MAX")
}

pub fn experimental_bash_default_timeout_ms() -> Option<i64> {
    get_i64("OPENCODE_EXPERIMENTAL_BASH_DEFAULT_TIMEOUT_MS")
}

pub fn config_path() -> Option<String> {
    get_string("OPENCODE_CONFIG")
}

pub fn config_content() -> Option<String> {
    get_string("OPENCODE_CONFIG_CONTENT")
}

pub fn tui_config_path() -> Option<String> {
    get_string("OPENCODE_TUI_CONFIG")
}

pub fn models_url() -> Option<String> {
    get_string("OPENCODE_MODELS_URL")
}

pub fn models_path() -> Option<String> {
    get_string("OPENCODE_MODELS_PATH")
}

/// The `OPENCODE_CLIENT` env var (e.g. "app", "cli", "desktop").
pub fn client() -> Option<String> {
    get_string("OPENCODE_CLIENT")
}

/// Whether running as an agent (non-interactive).
pub fn is_agent() -> bool {
    get_bool("OPENCODE_AGENT")
}

/// Custom DB path.
pub fn db_path() -> Option<String> {
    get_string("OPENCODE_DB_PATH")
}

/// Custom API base URL for provider overrides.
pub fn api_base_url() -> Option<String> {
    get_string("OPENCODE_API_BASE_URL")
}

/// Managed config directory (enterprise).
pub fn managed_config_dir() -> Option<String> {
    get_string("OPENCODE_MANAGED_CONFIG_DIR")
}
