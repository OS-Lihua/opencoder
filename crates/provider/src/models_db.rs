//! Model database: loads model metadata from models.dev API.
//!
//! Phase 1: Compatible with models.dev JSON format.
//! Phase 2 (future): Self-hosted Rust version of models.dev.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

/// Model metadata from models.dev.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub context_length: Option<u64>,
    #[serde(default)]
    pub max_output: Option<u64>,
    #[serde(default)]
    pub input_cost: Option<f64>,
    #[serde(default)]
    pub output_cost: Option<f64>,
    #[serde(default)]
    pub supports_tools: Option<bool>,
    #[serde(default)]
    pub supports_vision: Option<bool>,
    #[serde(default)]
    pub supports_streaming: Option<bool>,
}

/// The model database, loaded from cache or snapshot.
pub struct ModelsDb {
    models: RwLock<HashMap<String, Vec<ModelInfo>>>,
    cache_path: PathBuf,
}

impl ModelsDb {
    /// Load the model database.
    pub fn load() -> Arc<Self> {
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("opencoder");
        std::fs::create_dir_all(&cache_dir).ok();
        let cache_path = cache_dir.join("models.json");

        let db = Arc::new(Self {
            models: RwLock::new(HashMap::new()),
            cache_path,
        });

        // Try loading from cache
        if let Err(e) = db.load_from_cache() {
            debug!("no cached models: {e}");
            // Load from embedded snapshot
            db.load_snapshot();
        }

        db
    }

    /// Load from cache file.
    fn load_from_cache(&self) -> Result<()> {
        if !self.cache_path.exists() {
            anyhow::bail!("cache file not found");
        }
        let content = std::fs::read_to_string(&self.cache_path)?;
        let models: HashMap<String, Vec<ModelInfo>> = serde_json::from_str(&content)?;
        let mut lock = self.models.write().unwrap();
        *lock = models;
        debug!("loaded models from cache");
        Ok(())
    }

    /// Load from embedded snapshot (compile-time fallback).
    fn load_snapshot(&self) {
        // For now, populate with known models as hardcoded data.
        // In production, this would be include_bytes!("models-snapshot.json")
        let mut models = HashMap::new();

        models.insert("anthropic".to_string(), vec![
            ModelInfo {
                id: "claude-opus-4-20250514".to_string(),
                name: "Claude Opus 4".to_string(),
                provider: "anthropic".to_string(),
                context_length: Some(200_000),
                max_output: Some(32_000),
                input_cost: Some(15.0),
                output_cost: Some(75.0),
                supports_tools: Some(true),
                supports_vision: Some(true),
                supports_streaming: Some(true),
            },
            ModelInfo {
                id: "claude-sonnet-4-20250514".to_string(),
                name: "Claude Sonnet 4".to_string(),
                provider: "anthropic".to_string(),
                context_length: Some(200_000),
                max_output: Some(16_000),
                input_cost: Some(3.0),
                output_cost: Some(15.0),
                supports_tools: Some(true),
                supports_vision: Some(true),
                supports_streaming: Some(true),
            },
            ModelInfo {
                id: "claude-haiku-4-20250514".to_string(),
                name: "Claude Haiku 4".to_string(),
                provider: "anthropic".to_string(),
                context_length: Some(200_000),
                max_output: Some(8_192),
                input_cost: Some(0.80),
                output_cost: Some(4.0),
                supports_tools: Some(true),
                supports_vision: Some(true),
                supports_streaming: Some(true),
            },
        ]);

        models.insert("openai".to_string(), vec![
            ModelInfo {
                id: "gpt-4o".to_string(),
                name: "GPT-4o".to_string(),
                provider: "openai".to_string(),
                context_length: Some(128_000),
                max_output: Some(16_384),
                input_cost: Some(2.50),
                output_cost: Some(10.0),
                supports_tools: Some(true),
                supports_vision: Some(true),
                supports_streaming: Some(true),
            },
            ModelInfo {
                id: "gpt-4o-mini".to_string(),
                name: "GPT-4o Mini".to_string(),
                provider: "openai".to_string(),
                context_length: Some(128_000),
                max_output: Some(16_384),
                input_cost: Some(0.15),
                output_cost: Some(0.60),
                supports_tools: Some(true),
                supports_vision: Some(true),
                supports_streaming: Some(true),
            },
            ModelInfo {
                id: "o3".to_string(),
                name: "o3".to_string(),
                provider: "openai".to_string(),
                context_length: Some(200_000),
                max_output: Some(100_000),
                input_cost: Some(10.0),
                output_cost: Some(40.0),
                supports_tools: Some(true),
                supports_vision: Some(true),
                supports_streaming: Some(true),
            },
        ]);

        let mut lock = self.models.write().unwrap();
        *lock = models;
    }

    /// Refresh from the remote API.
    pub async fn refresh(&self) -> Result<()> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        let resp = client.get("https://models.dev/api.json").send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("models.dev API returned {}", resp.status());
        }

        let body = resp.text().await?;

        // Validate JSON
        let _: serde_json::Value = serde_json::from_str(&body)?;

        // Write to cache
        std::fs::write(&self.cache_path, &body)?;

        // Reload
        self.load_from_cache()?;
        info!("refreshed model database from models.dev");

        Ok(())
    }

    /// Get a specific model.
    pub fn get(&self, provider_id: &str, model_id: &str) -> Option<ModelInfo> {
        let lock = self.models.read().unwrap();
        lock.get(provider_id)?
            .iter()
            .find(|m| m.id == model_id)
            .cloned()
    }

    /// List all models for a provider.
    pub fn list_for_provider(&self, provider_id: &str) -> Vec<ModelInfo> {
        let lock = self.models.read().unwrap();
        lock.get(provider_id).cloned().unwrap_or_default()
    }

    /// List all providers.
    pub fn providers(&self) -> Vec<String> {
        let lock = self.models.read().unwrap();
        lock.keys().cloned().collect()
    }

    /// Start a background refresh task.
    pub fn start_refresh_task(db: Arc<Self>) {
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
                if let Err(e) = db.refresh().await {
                    warn!("model refresh failed: {e}");
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_snapshot() {
        let db = ModelsDb::load();
        let anthropic_models = db.list_for_provider("anthropic");
        assert!(!anthropic_models.is_empty());

        let claude = db.get("anthropic", "claude-sonnet-4-20250514");
        assert!(claude.is_some());
        assert_eq!(claude.unwrap().context_length, Some(200_000));
    }

    #[test]
    fn providers_list() {
        let db = ModelsDb::load();
        let providers = db.providers();
        assert!(providers.contains(&"anthropic".to_string()));
        assert!(providers.contains(&"openai".to_string()));
    }
}
