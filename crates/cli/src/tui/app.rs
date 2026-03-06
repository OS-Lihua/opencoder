//! Application state machine.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;

use opencoder_agent::AgentRegistry;
use opencoder_agent::agent_loop::{self, AgentLoopConfig};
use opencoder_core::bus::{Bus, Event as BusEvent};
use opencoder_core::config::Config;
use opencoder_core::storage::Database;
use opencoder_project::ProjectService;
use opencoder_provider::init as provider_init;
use opencoder_provider::provider::LlmProvider;
use opencoder_session::Session;
use opencoder_session::session::SessionService;
use opencoder_tool::ToolRegistry;

/// Current screen.
#[derive(Debug, Clone, PartialEq)]
pub enum Screen {
    Home,
    Session,
}

/// Input mode for the session screen.
#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    Normal,
    Editing,
}

/// User action from key input.
#[derive(Debug, Clone)]
pub enum Action {
    Quit,
    NewSession,
    DeleteSession,
    EnterSession,
    BackToHome,
    MoveUp,
    MoveDown,
    ScrollUp,
    ScrollDown,
    SendMessage,
    CancelAgent,
    InsertChar(char),
    DeleteChar,
    InsertNewline,
    StartSearch,
    Noop,
}

/// Main application state.
pub struct App {
    pub screen: Screen,
    pub input_mode: InputMode,
    pub sessions: Vec<Session>,
    pub selected_session: usize,
    pub current_session: Option<Session>,
    pub messages: Vec<opencoder_session::MessageWithParts>,
    pub input: String,
    pub scroll_offset: usize,
    pub status_text: String,
    pub agent_running: bool,
    pub search_query: String,
    pub searching: bool,

    // Services
    pub db: Arc<Database>,
    pub bus: Bus,
    pub config: Config,
    pub project_dir: PathBuf,
    pub provider: Arc<dyn LlmProvider>,
    pub tool_registry: Arc<ToolRegistry>,
    pub agent_registry: Arc<AgentRegistry>,
    pub session_svc: Arc<SessionService>,
    pub cancel: Option<tokio_util::sync::CancellationToken>,
}

impl App {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        db: Arc<Database>,
        bus: Bus,
        config: Config,
        project_dir: PathBuf,
        provider: Arc<dyn LlmProvider>,
        tool_registry: Arc<ToolRegistry>,
        agent_registry: Arc<AgentRegistry>,
        session_svc: Arc<SessionService>,
    ) -> Self {
        Self {
            screen: Screen::Home,
            input_mode: InputMode::Normal,
            sessions: Vec::new(),
            selected_session: 0,
            current_session: None,
            messages: Vec::new(),
            input: String::new(),
            scroll_offset: 0,
            status_text: String::new(),
            agent_running: false,
            search_query: String::new(),
            searching: false,
            db,
            bus,
            config,
            project_dir,
            provider,
            tool_registry,
            agent_registry,
            session_svc,
            cancel: None,
        }
    }

    pub fn load_sessions(&mut self) -> Result<()> {
        let project_svc = ProjectService::new(self.db.clone());
        let project = project_svc.ensure(&self.project_dir)?;
        self.sessions = self.session_svc.list(&project.id)?;
        Ok(())
    }

    pub fn load_messages(&mut self) -> Result<()> {
        if let Some(session) = &self.current_session {
            self.messages = self.session_svc.messages(&session.id)?;
        }
        Ok(())
    }

    /// Handle an action. Returns true if the app should quit.
    pub async fn handle_action(&mut self, action: Action) -> Result<bool> {
        match action {
            Action::Quit => return Ok(true),
            Action::MoveUp => {
                if self.selected_session > 0 {
                    self.selected_session -= 1;
                }
            }
            Action::MoveDown => {
                if self.selected_session + 1 < self.sessions.len() {
                    self.selected_session += 1;
                }
            }
            Action::ScrollUp => {
                self.scroll_offset = self.scroll_offset.saturating_sub(3);
            }
            Action::ScrollDown => {
                self.scroll_offset += 3;
            }
            Action::NewSession => {
                let project_svc = ProjectService::new(self.db.clone());
                let project = project_svc.ensure(&self.project_dir)?;
                let dir = self.project_dir.to_string_lossy().to_string();
                let session = self.session_svc.create(&project.id, &dir, None)?;
                self.current_session = Some(session);
                self.messages.clear();
                self.input.clear();
                self.screen = Screen::Session;
                self.input_mode = InputMode::Editing;
            }
            Action::EnterSession => {
                if let Some(session) = self.sessions.get(self.selected_session) {
                    self.current_session = Some(session.clone());
                    self.load_messages()?;
                    self.screen = Screen::Session;
                    self.input_mode = InputMode::Editing;
                    self.scroll_offset = 0;
                }
            }
            Action::DeleteSession => {
                if let Some(session) = self.sessions.get(self.selected_session) {
                    self.session_svc.remove(&session.id)?;
                    self.load_sessions()?;
                    if self.selected_session >= self.sessions.len() && self.selected_session > 0 {
                        self.selected_session -= 1;
                    }
                }
            }
            Action::BackToHome => {
                self.screen = Screen::Home;
                self.input_mode = InputMode::Normal;
                self.load_sessions()?;
            }
            Action::InsertChar(c) => {
                self.input.push(c);
            }
            Action::DeleteChar => {
                self.input.pop();
            }
            Action::InsertNewline => {
                self.input.push('\n');
            }
            Action::SendMessage => {
                if !self.input.trim().is_empty() && !self.agent_running {
                    self.send_message().await?;
                }
            }
            Action::CancelAgent => {
                if let Some(cancel) = &self.cancel {
                    cancel.cancel();
                    self.agent_running = false;
                    self.status_text = "Cancelled.".to_string();
                }
            }
            Action::StartSearch => {
                self.searching = !self.searching;
                if !self.searching {
                    self.search_query.clear();
                }
            }
            Action::Noop => {}
        }
        Ok(false)
    }

    async fn send_message(&mut self) -> Result<()> {
        let content = std::mem::take(&mut self.input);
        let session = self.current_session.as_ref().unwrap();

        let model_str = self
            .config
            .model
            .as_deref()
            .unwrap_or("anthropic/claude-sonnet-4-20250514");
        let (_, model_id) = provider_init::parse_model_str(model_str);

        let cancel = tokio_util::sync::CancellationToken::new();
        self.cancel = Some(cancel.clone());
        self.agent_running = true;
        self.status_text = "Thinking...".to_string();

        let loop_config = AgentLoopConfig {
            session_id: session.id.clone(),
            project_id: session.project_id.clone(),
            agent_name: "build".to_string(),
            model: model_id,
            provider: self.provider.clone(),
            cancel,
            project_dir: self.project_dir.clone(),
            config: self.config.clone(),
        };

        let session_svc = self.session_svc.clone();
        let agent_registry = self.agent_registry.clone();
        let tools = self.tool_registry.all().clone();
        let bus = self.bus.clone();

        tokio::spawn(async move {
            if let Err(e) = agent_loop::run(
                loop_config,
                &content,
                session_svc,
                &agent_registry,
                tools,
                &bus,
            )
            .await
            {
                tracing::error!(error = %e, "agent loop failed");
            }
        });

        Ok(())
    }

    pub fn handle_bus_event(&mut self, event: BusEvent) {
        match event {
            BusEvent::SessionStatus { status, .. } => {
                match status {
                    opencoder_core::bus::SessionStatusInfo::Idle => {
                        self.agent_running = false;
                        self.status_text.clear();
                        self.cancel = None;
                        // Reload messages
                        self.load_messages().ok();
                    }
                    opencoder_core::bus::SessionStatusInfo::Busy => {
                        self.agent_running = true;
                        self.status_text = "Thinking...".to_string();
                    }
                    _ => {}
                }
            }
            BusEvent::PartUpdated { .. } | BusEvent::PartDelta { .. } => {
                // Reload messages to show updates
                self.load_messages().ok();
            }
            _ => {}
        }
    }
}
