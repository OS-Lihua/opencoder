//! Terminal User Interface built with ratatui.

pub mod app;
pub mod components;
pub mod key;
pub mod screens;
pub mod theme;

use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::prelude::*;

use opencoder_agent::AgentRegistry;
use opencoder_core::bus::Bus;
use opencoder_core::config::Config;
use opencoder_core::storage::Database;
use opencoder_provider::provider::LlmProvider;
use opencoder_session::SessionService;
use opencoder_tool::ToolRegistry;

use app::{ActiveOverlay, App, Screen};

/// Run the TUI application.
#[allow(clippy::too_many_arguments)]
pub async fn run_tui(
    db: Arc<Database>,
    bus: Bus,
    config: Config,
    project_dir: PathBuf,
    provider: Option<Arc<dyn LlmProvider>>,
    tool_registry: Arc<ToolRegistry>,
    agent_registry: Arc<AgentRegistry>,
    session_svc: Arc<SessionService>,
) -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state
    let mut app = App::new(
        db,
        bus.clone(),
        config,
        project_dir,
        provider,
        tool_registry,
        agent_registry,
        session_svc,
    );

    // Load initial data
    app.load_sessions()?;

    // Main loop
    let result = run_loop(&mut terminal, &mut app, &bus).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    bus: &Bus,
) -> Result<()> {
    let mut bus_rx = bus.subscribe();

    loop {
        // Draw
        terminal.draw(|f| {
            match app.screen {
                Screen::Home => screens::home::render(f, app),
                Screen::Session => screens::session::render(f, app),
            }
            // Render overlay on top if active
            match &app.overlay {
                ActiveOverlay::None => {}
                ActiveOverlay::Permission(state) => {
                    components::permission_dialog::render(f, state);
                }
                ActiveOverlay::Question(state) => {
                    components::question_dialog::render(f, state);
                }
            }
        })?;

        // Handle events with 50ms poll timeout
        let timeout = std::time::Duration::from_millis(50);

        // Check for terminal events
        if crossterm::event::poll(timeout)?
            && let Event::Key(key) = event::read()?
        {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            let action = key::handle_key(key, &app.screen, &app.input_mode, &app.overlay);
            if app.handle_action(action).await? {
                break; // quit
            }
        }

        // Check for bus events
        while let Ok(event) = bus_rx.try_recv() {
            app.handle_bus_event(event);
        }
    }

    Ok(())
}
