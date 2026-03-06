//! HTTP API server built with axum.
//!
//! Mirrors `src/server/` from the original OpenCode.
//! Provides REST API for sessions, projects, config, providers, and events.

pub mod routes;
pub mod state;

pub use state::AppState;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use tokio::net::TcpListener;
use tower_http::cors::{Any, CorsLayer};
use tracing::info;

/// Build the axum router with all routes.
pub fn build_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .nest("/session", routes::session::router())
        .nest("/project", routes::project::router())
        .nest("/config", routes::config::router())
        .nest("/provider", routes::provider::router())
        .merge(routes::global::router())
        .layer(cors)
        .with_state(Arc::new(state))
}

/// Start the HTTP server on the given port.
pub async fn serve(state: AppState, port: u16) -> anyhow::Result<()> {
    let app = build_router(state);
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(addr).await?;
    info!("server listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
