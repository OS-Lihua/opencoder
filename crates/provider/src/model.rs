//! Model definitions and capabilities.
//!
//! Mirrors the Model type from `src/provider/provider.ts`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Input/output modality capabilities.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Modalities {
    pub text: bool,
    pub audio: bool,
    pub image: bool,
    pub video: bool,
    pub pdf: bool,
}

/// Model capabilities.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Capabilities {
    pub temperature: bool,
    pub reasoning: bool,
    pub attachment: bool,
    pub toolcall: bool,
    pub input: Modalities,
    pub output: Modalities,
    pub interleaved: bool,
}

/// Token cost per million.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheCost {
    pub read: f64,
    pub write: f64,
}

/// Cost structure for a model.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Cost {
    pub input: f64,
    pub output: f64,
    pub cache: CacheCost,
}

/// Token limits.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Limits {
    pub context: u64,
    pub input: Option<u64>,
    pub output: u64,
}

/// A model definition with all metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub provider_id: String,
    pub api: ApiInfo,
    pub name: String,
    pub family: Option<String>,
    pub capabilities: Capabilities,
    pub cost: Cost,
    pub limit: Limits,
    pub status: ModelStatus,
    #[serde(default)]
    pub options: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    pub release_date: Option<String>,
    pub variants: Option<HashMap<String, HashMap<String, serde_json::Value>>>,
}

/// API connection info.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ApiInfo {
    pub id: String,
    pub url: String,
    pub npm: String,
}

/// Model status.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelStatus {
    #[default]
    Active,
    Alpha,
    Beta,
    Deprecated,
}

/// Provider info with its models.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub source: ProviderSource,
    pub env: Vec<String>,
    pub key: Option<String>,
    #[serde(default)]
    pub options: HashMap<String, serde_json::Value>,
    pub models: HashMap<String, Model>,
}

/// How the provider was discovered.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderSource {
    #[default]
    Env,
    Config,
    Custom,
    Api,
}
