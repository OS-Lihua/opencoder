//! Configuration system with multi-level resolution.
//!
//! Mirrors `src/config/config.ts` from the original OpenCode.
//! Loads config from: managed → global → project → .opencode → env → inline

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::flag;

/// Top-level configuration structure.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct Config {
    #[serde(rename = "$schema")]
    pub schema: Option<String>,

    pub log_level: Option<String>,
    pub username: Option<String>,
    pub enterprise: Option<Enterprise>,

    /// Default model in "provider/model" format.
    pub model: Option<String>,
    /// Small model for lightweight tasks.
    pub small_model: Option<String>,
    /// Default agent name.
    pub default_agent: Option<String>,

    pub disabled_providers: Option<Vec<String>>,
    pub enabled_providers: Option<Vec<String>>,

    pub provider: Option<HashMap<String, ProviderConfig>>,
    pub agent: Option<HashMap<String, AgentConfig>>,
    pub command: Option<HashMap<String, CommandConfig>>,
    pub skills: Option<SkillsConfig>,
    pub permission: Option<serde_json::Value>,
    pub share: Option<ShareMode>,
    pub autoupdate: Option<serde_json::Value>,

    pub mcp: Option<HashMap<String, serde_json::Value>>,
    pub formatter: Option<serde_json::Value>,
    pub lsp: Option<serde_json::Value>,
    pub plugin: Option<Vec<String>>,
    pub instructions: Option<Vec<String>>,
    pub snapshot: Option<bool>,
    pub compaction: Option<CompactionConfig>,
    pub watcher: Option<WatcherConfig>,
    pub server: Option<ServerConfig>,
    pub experimental: Option<ExperimentalConfig>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Enterprise {
    pub url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderConfig {
    pub whitelist: Option<Vec<String>>,
    pub blacklist: Option<Vec<String>>,
    pub models: Option<HashMap<String, serde_json::Value>>,
    pub options: Option<ProviderOptions>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderOptions {
    #[serde(rename = "apiKey")]
    pub api_key: Option<String>,
    #[serde(rename = "baseURL")]
    pub base_url: Option<String>,
    pub timeout: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentConfig {
    pub model: Option<String>,
    pub variant: Option<String>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub prompt: Option<String>,
    pub disable: Option<bool>,
    pub description: Option<String>,
    pub mode: Option<String>,
    pub hidden: Option<bool>,
    pub options: Option<HashMap<String, serde_json::Value>>,
    pub color: Option<String>,
    pub steps: Option<u32>,
    pub permission: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CommandConfig {
    pub template: String,
    pub description: Option<String>,
    pub agent: Option<String>,
    pub model: Option<String>,
    pub subtask: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SkillsConfig {
    pub paths: Option<Vec<String>>,
    pub urls: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShareMode {
    Manual,
    Auto,
    Disabled,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CompactionConfig {
    pub auto: Option<bool>,
    pub prune: Option<bool>,
    pub reserved: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WatcherConfig {
    pub ignore: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub port: Option<u16>,
    pub hostname: Option<String>,
    pub mdns: Option<bool>,
    pub mdns_domain: Option<String>,
    pub cors: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ExperimentalConfig {
    pub disable_paste_summary: Option<bool>,
    pub batch_tool: Option<bool>,
    pub open_telemetry: Option<bool>,
    pub primary_tools: Option<Vec<String>>,
    pub continue_loop_on_deny: Option<bool>,
    pub mcp_timeout: Option<u64>,
}

impl Config {
    /// Load configuration from all sources, merged in priority order.
    pub fn load(project_dir: &Path) -> anyhow::Result<Self> {
        let mut config = Config::default();

        // 1. Global config (~/.config/opencode/opencode.json)
        let global_path = crate::global::paths().config.join("opencode.json");
        if let Some(c) = Self::load_file(&global_path)? {
            config = Self::merge(config, c);
        }
        // Also try .jsonc
        let global_jsonc = crate::global::paths().config.join("opencode.jsonc");
        if let Some(c) = Self::load_file(&global_jsonc)? {
            config = Self::merge(config, c);
        }

        // 2. Custom config from env
        if let Some(path) = flag::config_path()
            && let Some(c) = Self::load_file(Path::new(&path))?
        {
            config = Self::merge(config, c);
        }

        // 3. Project config (walk up from project_dir)
        for dir in Self::ancestors(project_dir) {
            let json = dir.join("opencode.json");
            let jsonc = dir.join("opencode.jsonc");
            if let Some(c) = Self::load_file(&jsonc)? {
                config = Self::merge(config, c);
                break;
            }
            if let Some(c) = Self::load_file(&json)? {
                config = Self::merge(config, c);
                break;
            }
        }

        // 4. .opencode directory config
        let opencode_dir = project_dir.join(".opencode");
        if opencode_dir.is_dir() {
            let json = opencode_dir.join("opencode.json");
            let jsonc = opencode_dir.join("opencode.jsonc");
            if let Some(c) = Self::load_file(&jsonc)? {
                config = Self::merge(config, c);
            } else if let Some(c) = Self::load_file(&json)? {
                config = Self::merge(config, c);
            }
        }

        // 5. Inline config from env
        if let Some(content) = flag::config_content()
            && let Ok(c) = serde_json::from_str::<Config>(&content)
        {
            config = Self::merge(config, c);
        }

        // 6. Managed config (enterprise, highest priority)
        if let Some(dir) = flag::managed_config_dir() {
            let json = PathBuf::from(&dir).join("opencode.json");
            if let Some(c) = Self::load_file(&json)? {
                config = Self::merge(config, c);
            }
        }

        Ok(config)
    }

    /// Load a single config file, returning None if it doesn't exist.
    fn load_file(path: &Path) -> anyhow::Result<Option<Config>> {
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(path)?;

        // Strip JSONC comments (simple: // and /* */)
        let stripped = strip_jsonc_comments(&content);

        match serde_json::from_str::<Config>(&stripped) {
            Ok(c) => {
                debug!("loaded config from {}", path.display());
                Ok(Some(c))
            }
            Err(e) => {
                warn!("invalid config at {}: {e}", path.display());
                Ok(None)
            }
        }
    }

    /// Merge two configs. `other` takes precedence over `self`.
    fn merge(mut base: Config, other: Config) -> Config {
        macro_rules! merge_opt {
            ($field:ident) => {
                if other.$field.is_some() {
                    base.$field = other.$field;
                }
            };
        }
        merge_opt!(schema);
        merge_opt!(log_level);
        merge_opt!(username);
        merge_opt!(enterprise);
        merge_opt!(model);
        merge_opt!(small_model);
        merge_opt!(default_agent);
        merge_opt!(disabled_providers);
        merge_opt!(enabled_providers);
        merge_opt!(provider);
        merge_opt!(agent);
        merge_opt!(command);
        merge_opt!(skills);
        merge_opt!(permission);
        merge_opt!(share);
        merge_opt!(autoupdate);
        merge_opt!(mcp);
        merge_opt!(formatter);
        merge_opt!(lsp);
        merge_opt!(snapshot);
        merge_opt!(compaction);
        merge_opt!(watcher);
        merge_opt!(server);
        merge_opt!(experimental);

        // Arrays are concatenated (plugin, instructions)
        if let Some(plugins) = other.plugin {
            let mut existing = base.plugin.unwrap_or_default();
            existing.extend(plugins);
            base.plugin = Some(existing);
        }
        if let Some(instructions) = other.instructions {
            let mut existing = base.instructions.unwrap_or_default();
            existing.extend(instructions);
            base.instructions = Some(existing);
        }

        base
    }

    /// Walk directory ancestors.
    fn ancestors(path: &Path) -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        let mut current = path.to_path_buf();
        loop {
            dirs.push(current.clone());
            if !current.pop() {
                break;
            }
        }
        dirs
    }
}

/// Minimal JSONC comment stripping.
fn strip_jsonc_comments(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    let mut escape = false;

    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }

        if c == '"' {
            in_string = true;
            out.push(c);
            continue;
        }

        if c == '/'
            && let Some(&next) = chars.peek()
        {
            if next == '/' {
                // Line comment: skip until newline
                for nc in chars.by_ref() {
                    if nc == '\n' {
                        out.push('\n');
                        break;
                    }
                }
                continue;
            } else if next == '*' {
                // Block comment: skip until */
                chars.next(); // consume *
                loop {
                    match chars.next() {
                        Some('*') => {
                            if chars.peek() == Some(&'/') {
                                chars.next();
                                break;
                            }
                        }
                        Some('\n') => out.push('\n'),
                        None => break,
                        _ => {}
                    }
                }
                continue;
            }
        }

        out.push(c);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_comments() {
        let input = r#"{
  // line comment
  "key": "value", /* block */
  "num": 42
}"#;
        let stripped = strip_jsonc_comments(input);
        let parsed: serde_json::Value = serde_json::from_str(&stripped).unwrap();
        assert_eq!(parsed["key"], "value");
        assert_eq!(parsed["num"], 42);
    }

    #[test]
    fn default_config() {
        let cfg = Config::default();
        assert!(cfg.model.is_none());
        assert!(cfg.agent.is_none());
    }
}
