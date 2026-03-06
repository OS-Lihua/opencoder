//! Shared application state for the server.

use std::path::PathBuf;
use std::sync::Arc;

use opencoder_agent::AgentRegistry;
use opencoder_core::bus::Bus;
use opencoder_core::config::Config;
use opencoder_core::storage::Database;
use opencoder_project::ProjectService;
use opencoder_provider::provider::LlmProvider;
use opencoder_session::SessionService;
use opencoder_tool::ToolRegistry;

/// Shared state passed to all route handlers.
pub struct AppState {
    pub db: Arc<Database>,
    pub bus: Bus,
    pub config: Config,
    pub project_dir: PathBuf,
    pub session_svc: Arc<SessionService>,
    pub project_svc: Arc<ProjectService>,
    pub provider: Arc<dyn LlmProvider>,
    pub agent_registry: Arc<AgentRegistry>,
    pub tool_registry: Arc<ToolRegistry>,
}

impl AppState {
    pub fn new(
        db: Arc<Database>,
        bus: Bus,
        config: Config,
        project_dir: PathBuf,
        provider: Arc<dyn LlmProvider>,
        agent_registry: Arc<AgentRegistry>,
        tool_registry: Arc<ToolRegistry>,
    ) -> Self {
        let session_svc = Arc::new(SessionService::new(db.clone(), bus.clone()));
        let project_svc = Arc::new(ProjectService::new(db.clone()));
        Self {
            db,
            bus,
            config,
            project_dir,
            session_svc,
            project_svc,
            provider,
            agent_registry,
            tool_registry,
        }
    }
}
