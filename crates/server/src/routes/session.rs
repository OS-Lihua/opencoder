//! Session API routes.
//!
//! Mirrors `src/server/route/session.ts` from the original OpenCode.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::AppState;

type S = Arc<AppState>;

pub fn router() -> Router<S> {
    Router::new()
        .route("/", get(list_sessions))
        .route("/", post(create_session))
        .route("/{id}", get(get_session))
        .route("/{id}", delete(delete_session))
        .route("/{id}/title", put(set_title))
        .route("/{id}/archive", put(archive_session))
        .route("/{id}/messages", get(get_messages))
        .route("/{id}/messages", post(send_message))
        .route("/{id}/fork", post(fork_session))
        .route("/{id}/share", post(share_session))
        .route("/{id}/share", delete(unshare_session))
}

#[derive(Deserialize)]
struct ListQuery {
    project_id: Option<String>,
}

async fn list_sessions(
    State(state): State<S>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<opencoder_session::Session>>, AppError> {
    let project = state.project_svc.ensure(&state.project_dir)?;
    let project_id = query.project_id.unwrap_or(project.id);
    let sessions = state.session_svc.list(&project_id)?;
    Ok(Json(sessions))
}

#[derive(Deserialize)]
struct CreateBody {
    title: Option<String>,
}

async fn create_session(
    State(state): State<S>,
    Json(body): Json<CreateBody>,
) -> Result<Json<opencoder_session::Session>, AppError> {
    let project = state.project_svc.ensure(&state.project_dir)?;
    let dir = state.project_dir.to_string_lossy().to_string();
    let mut session = state.session_svc.create(&project.id, &dir, None)?;
    if let Some(title) = body.title {
        state.session_svc.set_title(&session.id, &title)?;
        session.title = title;
    }
    Ok(Json(session))
}

async fn get_session(
    State(state): State<S>,
    Path(id): Path<String>,
) -> Result<Json<opencoder_session::Session>, AppError> {
    let session = state.session_svc.get(&id)?;
    Ok(Json(session))
}

async fn delete_session(
    State(state): State<S>,
    Path(id): Path<String>,
) -> Result<Json<DeleteResponse>, AppError> {
    state.session_svc.remove(&id)?;
    Ok(Json(DeleteResponse { ok: true }))
}

#[derive(Serialize)]
struct DeleteResponse {
    ok: bool,
}

#[derive(Deserialize)]
struct TitleBody {
    title: String,
}

async fn set_title(
    State(state): State<S>,
    Path(id): Path<String>,
    Json(body): Json<TitleBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.session_svc.set_title(&id, &body.title)?;
    Ok(Json(serde_json::json!({"ok": true})))
}

async fn archive_session(
    State(state): State<S>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.session_svc.archive(&id)?;
    Ok(Json(serde_json::json!({"ok": true})))
}

async fn get_messages(
    State(state): State<S>,
    Path(id): Path<String>,
) -> Result<Json<Vec<opencoder_session::MessageWithParts>>, AppError> {
    let messages = state.session_svc.messages(&id)?;
    Ok(Json(messages))
}

#[derive(Deserialize)]
struct SendMessageBody {
    content: String,
    #[serde(default)]
    agent: Option<String>,
}

#[derive(Serialize)]
struct SendMessageResponse {
    message_id: String,
    session_id: String,
}

async fn send_message(
    State(state): State<S>,
    Path(id): Path<String>,
    Json(body): Json<SendMessageBody>,
) -> Result<Json<SendMessageResponse>, AppError> {
    let agent_name = body.agent.unwrap_or_else(|| "build".to_string());
    let session_id = id.clone();

    // Determine model
    let model_str = state.config.model.as_deref().unwrap_or("anthropic/claude-sonnet-4-20250514");
    let (_provider_id, model_id) = opencoder_provider::init::parse_model_str(model_str);

    let cancel = tokio_util::sync::CancellationToken::new();

    let loop_config = opencoder_agent::agent_loop::AgentLoopConfig {
        session_id: session_id.clone(),
        project_id: state.project_svc.ensure(&state.project_dir)?.id,
        agent_name: agent_name.clone(),
        model: model_id,
        provider: state.provider.clone(),
        cancel,
        project_dir: state.project_dir.clone(),
        config: state.config.clone(),
    };

    let session_svc = state.session_svc.clone();
    let agent_registry = state.agent_registry.clone();
    let tools = state.tool_registry.all().clone();
    let bus = state.bus.clone();
    let content = body.content;

    // Spawn agent loop asynchronously
    tokio::spawn(async move {
        if let Err(e) = opencoder_agent::agent_loop::run(
            loop_config,
            &content,
            session_svc,
            &agent_registry,
            tools,
            &bus,
        ).await {
            tracing::error!(session_id = %id, error = %e, "agent loop failed");
            bus.publish(opencoder_core::bus::Event::SessionError {
                session_id: id.parse().unwrap_or_else(|_| {
                    opencoder_core::id::Identifier::create(opencoder_core::id::Prefix::Session)
                }),
                error: e.to_string(),
            });
        }
    });

    Ok(Json(SendMessageResponse {
        message_id: "pending".to_string(),
        session_id,
    }))
}

async fn fork_session(
    State(state): State<S>,
    Path(id): Path<String>,
) -> Result<Json<opencoder_session::Session>, AppError> {
    let project = state.project_svc.ensure(&state.project_dir)?;
    let dir = state.project_dir.to_string_lossy().to_string();
    let forked = state.session_svc.fork(&id, &project.id, &dir)?;
    Ok(Json(forked))
}

async fn share_session(
    State(state): State<S>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let url = opencoder_session::share::share(&id, &state.session_svc)?;
    Ok(Json(serde_json::json!({"url": url})))
}

async fn unshare_session(
    State(state): State<S>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    opencoder_session::share::unshare(&id, &state.session_svc)?;
    Ok(Json(serde_json::json!({"ok": true})))
}

/// Simple error wrapper for route handlers.
struct AppError(anyhow::Error);

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        AppError(e)
    }
}

impl axum::response::IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let body = serde_json::json!({
            "error": self.0.to_string(),
        });
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(body),
        )
            .into_response()
    }
}
