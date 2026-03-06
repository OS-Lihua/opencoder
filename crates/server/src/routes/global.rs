//! Global routes: health check, SSE events, version.

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::get;
use axum::{Json, Router};
use futures::stream::Stream;
use serde::Serialize;

use crate::AppState;

type S = Arc<AppState>;

pub fn router() -> Router<S> {
    Router::new()
        .route("/health", get(health))
        .route("/version", get(version))
        .route("/events", get(events_sse))
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok".into() })
}

#[derive(Serialize)]
struct HealthResponse {
    status: String,
}

async fn version() -> Json<VersionResponse> {
    Json(VersionResponse {
        version: env!("CARGO_PKG_VERSION").into(),
    })
}

#[derive(Serialize)]
struct VersionResponse {
    version: String,
}

/// SSE endpoint for real-time bus events.
async fn events_sse(
    State(state): State<S>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut rx = state.bus.subscribe();

    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    if let Ok(json) = serde_json::to_string(&event) {
                        yield Ok(Event::default().data(json));
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("SSE consumer lagged by {n} events");
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    };

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}
