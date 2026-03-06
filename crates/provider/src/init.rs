//! Provider initialization utilities.
//!
//! Factory function to build an LlmProvider from a "provider/model" string.

use std::sync::Arc;

use anyhow::{Result, bail};

use crate::provider::LlmProvider;
use crate::providers::anthropic::AnthropicProvider;
use crate::providers::openai::OpenAiProvider;

/// Known provider ID → environment variable name mappings.
const PROVIDER_ENV_KEYS: &[(&str, &[&str])] = &[
    ("anthropic", &["ANTHROPIC_API_KEY"]),
    ("openai", &["OPENAI_API_KEY"]),
    ("google", &["GOOGLE_API_KEY", "GEMINI_API_KEY"]),
    ("groq", &["GROQ_API_KEY"]),
    ("openrouter", &["OPENROUTER_API_KEY"]),
    ("together", &["TOGETHER_API_KEY"]),
    ("fireworks", &["FIREWORKS_API_KEY"]),
    ("deepseek", &["DEEPSEEK_API_KEY"]),
    ("mistral", &["MISTRAL_API_KEY"]),
    ("xai", &["XAI_API_KEY"]),
    ("azure", &["AZURE_OPENAI_API_KEY"]),
    ("bedrock", &["AWS_ACCESS_KEY_ID"]),
    ("copilot", &["GITHUB_TOKEN"]),
];

/// Known provider ID → base URL mappings for OpenAI-compatible providers.
const OPENAI_COMPATIBLE: &[(&str, &str, &str)] = &[
    ("groq", "https://api.groq.com/openai", "Groq"),
    ("openrouter", "https://openrouter.ai/api", "OpenRouter"),
    ("together", "https://api.together.xyz", "Together"),
    ("fireworks", "https://api.fireworks.ai/inference", "Fireworks"),
    ("deepseek", "https://api.deepseek.com", "DeepSeek"),
    ("mistral", "https://api.mistral.ai", "Mistral"),
    ("xai", "https://api.x.ai", "xAI"),
];

/// Parse a model string into (provider_id, model_id).
///
/// Formats:
///   "anthropic/claude-opus-4-6" → ("anthropic", "claude-opus-4-6")
///   "openai/gpt-4o"            → ("openai", "gpt-4o")
///   "claude-opus-4-6"          → ("anthropic", "claude-opus-4-6") (inferred)
///   "gpt-4o"                   → ("openai", "gpt-4o") (inferred)
pub fn parse_model_str(model_str: &str) -> (String, String) {
    if let Some((provider, model)) = model_str.split_once('/') {
        return (provider.to_string(), model.to_string());
    }

    // Infer provider from model name prefix
    let model_lower = model_str.to_lowercase();
    let provider = if model_lower.starts_with("claude") || model_lower.starts_with("anthropic") {
        "anthropic"
    } else if model_lower.starts_with("gpt")
        || model_lower.starts_with("o1")
        || model_lower.starts_with("o3")
        || model_lower.starts_with("o4")
    {
        "openai"
    } else if model_lower.starts_with("gemini") || model_lower.starts_with("gemma") {
        "google"
    } else if model_lower.starts_with("deepseek") {
        "deepseek"
    } else if model_lower.starts_with("mistral") || model_lower.starts_with("codestral") {
        "mistral"
    } else if model_lower.starts_with("grok") {
        "xai"
    } else if model_lower.starts_with("llama") || model_lower.starts_with("meta") {
        "groq"
    } else {
        "anthropic" // default fallback
    };

    (provider.to_string(), model_str.to_string())
}

/// Look up the API key for a provider from environment variables.
fn find_api_key(provider_id: &str) -> Option<String> {
    for &(id, keys) in PROVIDER_ENV_KEYS {
        if id == provider_id {
            for key in keys {
                if let Ok(val) = std::env::var(key) {
                    if !val.is_empty() {
                        return Some(val);
                    }
                }
            }
        }
    }
    None
}

/// Build an LlmProvider from a model string and optional config overrides.
///
/// The model_str is in "provider/model" format (e.g., "anthropic/claude-opus-4-6").
/// API keys are read from environment variables by default.
pub fn build_provider(model_str: &str) -> Result<(Arc<dyn LlmProvider>, String)> {
    let (provider_id, model_id) = parse_model_str(model_str);

    let api_key = find_api_key(&provider_id)
        .ok_or_else(|| {
            let env_keys: Vec<_> = PROVIDER_ENV_KEYS
                .iter()
                .filter(|(id, _)| *id == provider_id)
                .flat_map(|(_, keys)| keys.iter())
                .collect();
            let keys_str = if env_keys.is_empty() {
                format!("{}_API_KEY", provider_id.to_uppercase())
            } else {
                env_keys.iter().map(|k| k.to_string()).collect::<Vec<_>>().join(" or ")
            };
            anyhow::anyhow!(
                "No API key found for provider '{}'. Set {}",
                provider_id,
                keys_str
            )
        })?;

    let provider: Arc<dyn LlmProvider> = match provider_id.as_str() {
        "anthropic" => Arc::new(AnthropicProvider::new(api_key)),
        "openai" => Arc::new(OpenAiProvider::new(api_key)),
        other => {
            // Check if it's an OpenAI-compatible provider
            if let Some(&(_, base_url, name)) = OPENAI_COMPATIBLE.iter().find(|(id, _, _)| *id == other) {
                Arc::new(OpenAiProvider::new_compatible(api_key, base_url, other, name))
            } else {
                bail!(
                    "Unknown provider '{}'. Supported: anthropic, openai, groq, openrouter, together, fireworks, deepseek, mistral, xai",
                    other
                );
            }
        }
    };

    Ok((provider, model_id))
}

/// Build provider with optional config overrides (base_url, api_key from config).
pub fn build_provider_with_config(
    model_str: &str,
    config: &opencoder_core::config::Config,
) -> Result<(Arc<dyn LlmProvider>, String)> {
    let (provider_id, model_id) = parse_model_str(model_str);

    // Check config for provider-specific overrides
    let provider_cfg = config
        .provider
        .as_ref()
        .and_then(|p| p.get(&provider_id));

    // API key: config override > env var
    let api_key = provider_cfg
        .and_then(|c| c.options.as_ref())
        .and_then(|o| o.api_key.clone())
        .or_else(|| find_api_key(&provider_id))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No API key for provider '{}'. Set the corresponding env var or add to config.",
                provider_id
            )
        })?;

    // Base URL: config override > default
    let base_url_override = provider_cfg
        .and_then(|c| c.options.as_ref())
        .and_then(|o| o.base_url.clone());

    let provider: Arc<dyn LlmProvider> = match provider_id.as_str() {
        "anthropic" => {
            let mut p = AnthropicProvider::new(api_key);
            if let Some(url) = base_url_override {
                p = p.with_base_url(url);
            }
            Arc::new(p)
        }
        "openai" => {
            let mut p = OpenAiProvider::new(api_key);
            if let Some(url) = base_url_override {
                p = p.with_base_url(url);
            }
            Arc::new(p)
        }
        other => {
            if let Some(&(_, default_url, name)) = OPENAI_COMPATIBLE.iter().find(|(id, _, _)| *id == other) {
                let url = base_url_override.unwrap_or_else(|| default_url.to_string());
                Arc::new(OpenAiProvider::new_compatible(api_key, url, other, name))
            } else {
                // Treat as generic OpenAI-compatible
                let url = base_url_override
                    .ok_or_else(|| anyhow::anyhow!("Unknown provider '{}' requires a base_url in config", other))?;
                Arc::new(OpenAiProvider::new_compatible(api_key, url, other, other))
            }
        }
    };

    Ok((provider, model_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_provider_model() {
        let (p, m) = parse_model_str("anthropic/claude-opus-4-6");
        assert_eq!(p, "anthropic");
        assert_eq!(m, "claude-opus-4-6");
    }

    #[test]
    fn parse_model_only() {
        let (p, m) = parse_model_str("claude-opus-4-6");
        assert_eq!(p, "anthropic");
        assert_eq!(m, "claude-opus-4-6");
    }

    #[test]
    fn parse_openai_model() {
        let (p, m) = parse_model_str("gpt-4o");
        assert_eq!(p, "openai");
        assert_eq!(m, "gpt-4o");
    }

    #[test]
    fn parse_with_slash() {
        let (p, m) = parse_model_str("groq/llama-3.1-70b");
        assert_eq!(p, "groq");
        assert_eq!(m, "llama-3.1-70b");
    }

    #[test]
    fn infer_deepseek() {
        let (p, _) = parse_model_str("deepseek-coder");
        assert_eq!(p, "deepseek");
    }

    #[test]
    fn infer_gemini() {
        let (p, _) = parse_model_str("gemini-2.0-flash");
        assert_eq!(p, "google");
    }
}
