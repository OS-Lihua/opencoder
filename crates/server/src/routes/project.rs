//! Project API routes.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::{get, put};
use axum::{Json, Router};
use serde::Deserialize;

use crate::AppState;

type S = Arc<AppState>;

pub fn router() -> Router<S> {
    Router::new()
        .route("/", get(list_projects))
        .route("/{id}", get(get_project))
        .route("/{id}", put(update_project))
}

async fn list_projects(
    State(state): State<S>,
) -> Result<Json<Vec<opencoder_project::Project>>, AppError> {
    let projects = state.project_svc.list()?;
    Ok(Json(projects))
}

async fn get_project(
    State(state): State<S>,
    Path(id): Path<String>,
) -> Result<Json<opencoder_project::Project>, AppError> {
    let project = state.project_svc.get(&id)?;
    Ok(Json(project))
}

#[derive(Deserialize)]
struct UpdateBody {
    name: Option<String>,
}

async fn update_project(
    State(state): State<S>,
    Path(id): Path<String>,
    Json(body): Json<UpdateBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    if let Some(name) = body.name {
        state.project_svc.update_name(&id, &name)?;
    }
    Ok(Json(serde_json::json!({"ok": true})))
}

struct AppError(anyhow::Error);

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        AppError(e)
    }
}

impl axum::response::IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let body = serde_json::json!({"error": self.0.to_string()});
        (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Json(body)).into_response()
    }
}
