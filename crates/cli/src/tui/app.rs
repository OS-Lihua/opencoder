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
use opencoder_session::message::Part;
use opencoder_session::session::SessionService;
use opencoder_snapshot::SnapshotStore;
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

/// Active overlay (dialog) on top of the current screen.
pub enum ActiveOverlay {
    None,
    Permission(PermissionDialogState),
    Question(QuestionDialogState),
    AgentSelector(AgentSelectorState),
    FileSelector(super::components::file_selector::FileSelectorState),
    ModelSelector(super::components::model_selector::ModelSelectorState),
}

/// State for the agent selector overlay.
pub struct AgentSelectorState {
    pub agents: Vec<(String, String)>, // (name, description)
    pub selected: usize,
}

/// State for the permission dialog overlay.
pub struct PermissionDialogState {
    pub request_id: String,
    pub session_id: String,
    pub tool_name: String,
    pub description: String,
    pub selected: usize, // 0=Allow, 1=Deny, 2=Always Allow
}

/// State for the question dialog overlay.
pub struct QuestionDialogState {
    pub question_id: String,
    pub session_id: String,
    pub question_text: String,
    pub options: Vec<String>,
    pub input: String,
    pub selected_option: usize,
}

/// Input state with cursor position and history.
pub struct InputState {
    pub text: String,
    pub cursor: usize, // byte offset
    pub history: Vec<String>,
    pub history_index: Option<usize>,
}

impl InputState {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            cursor: 0,
            history: Vec::new(),
            history_index: None,
        }
    }

    pub fn insert_char(&mut self, c: char) {
        self.text.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    pub fn delete_char_before(&mut self) {
        if self.cursor > 0 {
            // Find the previous char boundary
            let prev = self.text[..self.cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.text.drain(prev..self.cursor);
            self.cursor = prev;
        }
    }

    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor = self.text[..self.cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    pub fn move_right(&mut self) {
        if self.cursor < self.text.len() {
            self.cursor = self.text[self.cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.cursor + i)
                .unwrap_or(self.text.len());
        }
    }

    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.text.len();
    }

    pub fn kill_to_end(&mut self) {
        self.text.truncate(self.cursor);
    }

    pub fn kill_to_start(&mut self) {
        self.text.drain(..self.cursor);
        self.cursor = 0;
    }

    pub fn kill_word_back(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let before = &self.text[..self.cursor];
        // Skip trailing spaces, then skip non-spaces
        let trimmed = before.trim_end();
        let word_start = trimmed
            .rfind(|c: char| c.is_whitespace())
            .map(|i| i + 1)
            .unwrap_or(0);
        self.text.drain(word_start..self.cursor);
        self.cursor = word_start;
    }

    pub fn history_up(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let idx = match self.history_index {
            Some(0) => return,
            Some(i) => i - 1,
            None => self.history.len() - 1,
        };
        self.history_index = Some(idx);
        self.text = self.history[idx].clone();
        self.cursor = self.text.len();
    }

    pub fn history_down(&mut self) {
        if let Some(idx) = self.history_index {
            if idx + 1 < self.history.len() {
                let new_idx = idx + 1;
                self.history_index = Some(new_idx);
                self.text = self.history[new_idx].clone();
                self.cursor = self.text.len();
            } else {
                self.history_index = None;
                self.text.clear();
                self.cursor = 0;
            }
        }
    }

    pub fn take_text(&mut self) -> String {
        let text = std::mem::take(&mut self.text);
        if !text.trim().is_empty() {
            self.history.push(text.clone());
        }
        self.cursor = 0;
        self.history_index = None;
        text
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
        self.history_index = None;
    }

    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
    }

    pub fn insert_newline(&mut self) {
        self.insert_char('\n');
    }

    pub fn trim_start_starts_with(&self, prefix: &str) -> bool {
        self.text.trim_start().starts_with(prefix)
    }
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
    CursorLeft,
    CursorRight,
    CursorHome,
    CursorEnd,
    KillToEnd,
    KillToStart,
    KillWordBack,
    HistoryUp,
    HistoryDown,
    StartSearch,
    OpenAgentSelector,
    OpenModelSelector,
    // Overlay actions
    OverlaySelect(usize),
    OverlayConfirm,
    OverlayDismiss,
    OverlayInput(char),
    OverlayBackspace,
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
    pub input_state: InputState,
    pub scroll_offset: usize,
    pub status_text: String,
    pub agent_running: bool,
    pub search_query: String,
    pub searching: bool,
    pub current_agent: String,

    // Overlay
    pub overlay: ActiveOverlay,

    // Snapshot
    pub snapshot_store: Option<Arc<SnapshotStore>>,

    // Services
    pub db: Arc<Database>,
    pub bus: Bus,
    pub config: Config,
    pub project_dir: PathBuf,
    pub provider: Option<Arc<dyn LlmProvider>>,
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
        provider: Option<Arc<dyn LlmProvider>>,
        tool_registry: Arc<ToolRegistry>,
        agent_registry: Arc<AgentRegistry>,
        session_svc: Arc<SessionService>,
        snapshot_store: Option<Arc<SnapshotStore>>,
    ) -> Self {
        Self {
            screen: Screen::Home,
            input_mode: InputMode::Normal,
            sessions: Vec::new(),
            selected_session: 0,
            current_session: None,
            messages: Vec::new(),
            input_state: InputState::new(),
            scroll_offset: 0,
            status_text: String::new(),
            agent_running: false,
            search_query: String::new(),
            searching: false,
            current_agent: "build".to_string(),
            overlay: ActiveOverlay::None,
            snapshot_store,
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
                self.input_state.clear();
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
                if self.screen == Screen::Home && self.searching {
                    self.search_query.push(c);
                } else if c == '@' && self.screen == Screen::Session {
                    // Open file selector
                    let mut state =
                        super::components::file_selector::FileSelectorState::new(&self.project_dir);
                    state.update_matches();
                    self.overlay = ActiveOverlay::FileSelector(state);
                } else {
                    self.input_state.insert_char(c);
                }
            }
            Action::DeleteChar => {
                if self.screen == Screen::Home && self.searching {
                    self.search_query.pop();
                } else {
                    self.input_state.delete_char_before();
                }
            }
            Action::InsertNewline => {
                self.input_state.insert_newline();
            }
            Action::CursorLeft => {
                self.input_state.move_left();
            }
            Action::CursorRight => {
                self.input_state.move_right();
            }
            Action::CursorHome => {
                self.input_state.move_home();
            }
            Action::CursorEnd => {
                self.input_state.move_end();
            }
            Action::KillToEnd => {
                self.input_state.kill_to_end();
            }
            Action::KillToStart => {
                self.input_state.kill_to_start();
            }
            Action::KillWordBack => {
                self.input_state.kill_word_back();
            }
            Action::HistoryUp => {
                if self.input_state.text.is_empty() {
                    self.input_state.history_up();
                }
            }
            Action::HistoryDown => {
                if self.input_state.history_index.is_some() {
                    self.input_state.history_down();
                }
            }
            Action::SendMessage => {
                if !self.input_state.is_empty() && !self.agent_running {
                    if self.input_state.trim_start_starts_with("/") {
                        self.handle_slash_command().await?;
                    } else {
                        self.send_message().await?;
                    }
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
            Action::OpenModelSelector => {
                if !self.agent_running {
                    let current_model = self
                        .config
                        .model
                        .as_deref()
                        .unwrap_or("anthropic/claude-sonnet-4-20250514");
                    let state =
                        super::components::model_selector::ModelSelectorState::new(current_model);
                    self.overlay = ActiveOverlay::ModelSelector(state);
                }
            }
            Action::OpenAgentSelector => {
                if !self.agent_running {
                    let agents: Vec<(String, String)> = self
                        .agent_registry
                        .list()
                        .iter()
                        .map(|a| (a.name.clone(), a.description.clone()))
                        .collect();
                    let selected = agents
                        .iter()
                        .position(|(name, _)| name == &self.current_agent)
                        .unwrap_or(0);
                    self.overlay =
                        ActiveOverlay::AgentSelector(AgentSelectorState { agents, selected });
                }
            }
            Action::OverlaySelect(idx) => match &mut self.overlay {
                ActiveOverlay::Permission(state) => {
                    if idx < 3 {
                        state.selected = idx;
                    }
                }
                ActiveOverlay::Question(state) => {
                    if !state.options.is_empty() && idx < state.options.len() {
                        state.selected_option = idx;
                    }
                }
                ActiveOverlay::AgentSelector(state) => {
                    if idx < state.agents.len() {
                        state.selected = idx;
                    }
                }
                ActiveOverlay::FileSelector(state) => {
                    if idx < state.matches.len() {
                        state.selected = idx;
                    }
                }
                ActiveOverlay::ModelSelector(state) => {
                    if idx < state.filtered.len() {
                        state.selected = idx;
                    }
                }
                ActiveOverlay::None => {}
            },
            Action::OverlayConfirm => {
                match std::mem::replace(&mut self.overlay, ActiveOverlay::None) {
                    ActiveOverlay::Permission(state) => {
                        let reply = match state.selected {
                            0 => "allow",
                            1 => "deny",
                            2 => "always",
                            _ => "deny",
                        };
                        self.bus.publish(BusEvent::PermissionReplied {
                            session_id: state.session_id.parse().unwrap_or_else(|_| {
                                opencoder_core::id::Identifier::create(
                                    opencoder_core::id::Prefix::Session,
                                )
                            }),
                            request_id: state.request_id.parse().unwrap_or_else(|_| {
                                opencoder_core::id::Identifier::create(
                                    opencoder_core::id::Prefix::Permission,
                                )
                            }),
                            reply: reply.to_string(),
                        });
                    }
                    ActiveOverlay::Question(state) => {
                        let reply = if !state.options.is_empty() {
                            state.options[state.selected_option].clone()
                        } else {
                            state.input.clone()
                        };
                        self.bus.publish(BusEvent::QuestionReplied {
                            id: state.question_id.parse().unwrap_or_else(|_| {
                                opencoder_core::id::Identifier::create(
                                    opencoder_core::id::Prefix::Question,
                                )
                            }),
                            session_id: state.session_id.parse().unwrap_or_else(|_| {
                                opencoder_core::id::Identifier::create(
                                    opencoder_core::id::Prefix::Session,
                                )
                            }),
                            reply,
                        });
                    }
                    ActiveOverlay::AgentSelector(state) => {
                        if let Some((name, _)) = state.agents.get(state.selected) {
                            self.current_agent = name.clone();
                        }
                    }
                    ActiveOverlay::FileSelector(state) => {
                        if let Some(file) = state.selected_file() {
                            let file = file.to_string();
                            self.input_state.insert_char('@');
                            for c in file.chars() {
                                self.input_state.insert_char(c);
                            }
                        }
                    }
                    ActiveOverlay::ModelSelector(state) => {
                        if let Some(model_str) = state.selected_model() {
                            let model_str = model_str.to_string();
                            match provider_init::build_provider_with_config(
                                &model_str,
                                &self.config,
                            ) {
                                Ok((provider, _)) => {
                                    self.provider = Some(provider);
                                    self.config.model = Some(model_str.clone());
                                    self.status_text = format!("Switched to model: {model_str}");
                                }
                                Err(e) => {
                                    self.status_text = format!("Failed to switch model: {e}");
                                }
                            }
                        }
                    }
                    ActiveOverlay::None => {}
                }
            }
            Action::OverlayDismiss => {
                if let ActiveOverlay::Permission(state) =
                    std::mem::replace(&mut self.overlay, ActiveOverlay::None)
                {
                    // Dismiss = deny
                    self.bus.publish(BusEvent::PermissionReplied {
                        session_id: state.session_id.parse().unwrap_or_else(|_| {
                            opencoder_core::id::Identifier::create(
                                opencoder_core::id::Prefix::Session,
                            )
                        }),
                        request_id: state.request_id.parse().unwrap_or_else(|_| {
                            opencoder_core::id::Identifier::create(
                                opencoder_core::id::Prefix::Permission,
                            )
                        }),
                        reply: "deny".to_string(),
                    });
                }
                // For question, dismiss just closes without replying (timeout will handle it)
            }
            Action::OverlayInput(c) => match &mut self.overlay {
                ActiveOverlay::Question(state) if state.options.is_empty() => {
                    state.input.push(c);
                }
                ActiveOverlay::FileSelector(state) => {
                    state.insert_char(c);
                }
                ActiveOverlay::ModelSelector(state) => {
                    state.insert_char(c);
                }
                _ => {}
            },
            Action::OverlayBackspace => match &mut self.overlay {
                ActiveOverlay::Question(state) if state.options.is_empty() => {
                    state.input.pop();
                }
                ActiveOverlay::FileSelector(state) => {
                    state.delete_char();
                }
                ActiveOverlay::ModelSelector(state) => {
                    state.delete_char();
                }
                _ => {}
            },
            Action::Noop => {}
        }
        Ok(false)
    }

    async fn send_message(&mut self) -> Result<()> {
        let content = self.input_state.take_text();
        let session = self.current_session.as_ref().unwrap();

        let model_str = self
            .config
            .model
            .as_deref()
            .unwrap_or("anthropic/claude-sonnet-4-20250514");

        // Lazy-init provider if not yet available
        if self.provider.is_none() {
            match provider_init::build_provider_with_config(model_str, &self.config) {
                Ok((p, _)) => {
                    self.provider = Some(p);
                }
                Err(e) => {
                    self.input_state.text = content; // restore input so user doesn't lose it
                    self.input_state.cursor = self.input_state.text.len();
                    self.status_text =
                        format!("Provider error: {e} — set ANTHROPIC_API_KEY or OPENAI_API_KEY");
                    return Ok(());
                }
            }
        }

        let (_, model_id) = provider_init::parse_model_str(model_str);

        let cancel = tokio_util::sync::CancellationToken::new();
        self.cancel = Some(cancel.clone());
        self.agent_running = true;
        self.status_text = "Thinking...".to_string();

        let loop_config = AgentLoopConfig {
            session_id: session.id.clone(),
            project_id: session.project_id.clone(),
            agent_name: self.current_agent.clone(),
            model: model_id,
            provider: self.provider.clone().unwrap(),
            cancel,
            project_dir: self.project_dir.clone(),
            config: self.config.clone(),
            db: self.db.clone(),
            snapshot_store: self.snapshot_store.clone(),
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
                agent_registry,
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

    async fn handle_slash_command(&mut self) -> Result<()> {
        let input = self.input_state.take_text();
        let parts: Vec<&str> = input.trim().splitn(2, ' ').collect();
        let cmd = parts[0];
        match cmd {
            "/new" => {
                let project_svc = ProjectService::new(self.db.clone());
                let project = project_svc.ensure(&self.project_dir)?;
                let dir = self.project_dir.to_string_lossy().to_string();
                let session = self.session_svc.create(&project.id, &dir, None)?;
                self.current_session = Some(session);
                self.messages.clear();
                self.screen = Screen::Session;
                self.input_mode = InputMode::Editing;
            }
            "/help" => {
                self.status_text = "Commands: /new /undo /compact /models /help".to_string();
            }
            "/compact" => {
                if let Some(ref session) = self.current_session {
                    let session_id = session.id.clone();
                    let session_svc = self.session_svc.clone();
                    let provider = self.provider.clone();
                    let model_str = self
                        .config
                        .model
                        .as_deref()
                        .unwrap_or("anthropic/claude-sonnet-4-20250514");
                    let (_, model_id) = provider_init::parse_model_str(model_str);
                    if let Some(provider) = provider {
                        tokio::spawn(async move {
                            if let Err(e) = opencoder_session::compaction::process(
                                &session_id,
                                &session_svc,
                                &provider,
                                &model_id,
                            )
                            .await
                            {
                                tracing::warn!("compaction failed: {e}");
                            }
                        });
                        self.status_text = "Compaction started.".to_string();
                    } else {
                        self.status_text = "No provider available.".to_string();
                    }
                }
            }
            "/models" => {
                self.status_text = format!(
                    "Model: {}",
                    self.config.model.as_deref().unwrap_or("default")
                );
            }
            "/undo" => {
                if let Some(ref store) = self.snapshot_store {
                    // Find the most recent StepStart with a snapshot_hash
                    let hash = self
                        .messages
                        .iter()
                        .rev()
                        .flat_map(|m| m.parts.iter().rev())
                        .find_map(|p| match &p.part {
                            Part::StepStart(s) => s.snapshot_hash.clone(),
                            _ => None,
                        });
                    if let Some(h) = hash {
                        let s = store.clone();
                        match tokio::task::spawn_blocking(move || s.restore(&h)).await {
                            Ok(Ok(())) => {
                                self.status_text = "Restored to previous state.".to_string();
                            }
                            Ok(Err(e)) => {
                                self.status_text = format!("Undo failed: {e}");
                            }
                            Err(e) => {
                                self.status_text = format!("Undo failed: {e}");
                            }
                        }
                    } else {
                        self.status_text = "No snapshot to restore.".to_string();
                    }
                } else {
                    self.status_text = "Snapshots not enabled.".to_string();
                }
            }
            _ => {
                self.status_text = format!("Unknown command: {cmd}. Type /help");
                self.input_state.text = input; // restore input
                self.input_state.cursor = self.input_state.text.len();
            }
        }
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
            BusEvent::PermissionAsked {
                id,
                session_id,
                tool_name,
                description,
            } => {
                // Only handle if it's for the current session
                let current_sid = self.current_session.as_ref().map(|s| s.id.as_str());
                if current_sid == Some(session_id.as_str()) {
                    self.overlay = ActiveOverlay::Permission(PermissionDialogState {
                        request_id: id.to_string(),
                        session_id: session_id.to_string(),
                        tool_name,
                        description,
                        selected: 0,
                    });
                }
            }
            BusEvent::QuestionAsked {
                id,
                session_id,
                question,
                options,
            } => {
                let current_sid = self.current_session.as_ref().map(|s| s.id.as_str());
                if current_sid == Some(session_id.as_str()) {
                    self.overlay = ActiveOverlay::Question(QuestionDialogState {
                        question_id: id.to_string(),
                        session_id: session_id.to_string(),
                        question_text: question,
                        options,
                        input: String::new(),
                        selected_option: 0,
                    });
                }
            }
            BusEvent::SessionUpdated(session_event) => {
                if let Some(ref mut session) = self.current_session
                    && session.id == session_event.id.as_str()
                {
                    session.title = session_event.title.clone();
                }
                self.load_sessions().ok();
            }
            _ => {}
        }
    }
}
