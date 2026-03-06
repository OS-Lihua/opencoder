//! Config API routes.

use std::sync::Arc;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};

use crate::AppState;

type S = Arc<AppState>;

pub fn router() -> Router<S> {
    Router::new()
        .route("/", get(get_config))
}

async fn get_config(
    State(state): State<S>,
) -> Json<opencoder_core::config::Config> {
    Json(state.config.clone())
}
