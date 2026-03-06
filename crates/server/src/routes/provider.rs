//! Provider API routes.

use std::sync::Arc;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

use crate::AppState;

type S = Arc<AppState>;

pub fn router() -> Router<S> {
    Router::new()
        .route("/", get(list_providers))
}

#[derive(Serialize)]
struct ProviderInfo {
    id: String,
    name: String,
}

async fn list_providers(
    State(_state): State<S>,
) -> Json<Vec<ProviderInfo>> {
    // Return the known built-in providers
    Json(vec![
        ProviderInfo { id: "anthropic".into(), name: "Anthropic".into() },
        ProviderInfo { id: "openai".into(), name: "OpenAI".into() },
        ProviderInfo { id: "google".into(), name: "Google".into() },
        ProviderInfo { id: "azure".into(), name: "Azure OpenAI".into() },
        ProviderInfo { id: "groq".into(), name: "Groq".into() },
        ProviderInfo { id: "openrouter".into(), name: "OpenRouter".into() },
    ])
}
