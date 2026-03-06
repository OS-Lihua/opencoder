mod output;
mod tui;
mod upgrade;

use std::path::PathBuf;
use std::process;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

use opencoder_agent::agent_loop::{self, AgentLoopConfig};
use opencoder_agent::AgentRegistry;
use opencoder_core::bus::Bus;
use opencoder_core::config::Config;
use opencoder_core::global;
use opencoder_core::storage::Database;
use opencoder_project::ProjectService;
use opencoder_provider::init as provider_init;
use opencoder_session::SessionService;
use opencoder_tool::ToolRegistry;

const DEFAULT_MODEL: &str = "anthropic/claude-sonnet-4-20250514";

#[derive(Parser)]
#[command(name = "opencoder", version, about = "AI-powered coding agent")]
struct Cli {
    /// Project directory to operate on
    #[arg(global = true)]
    directory: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the headless API server
    Serve {
        /// Port to listen on
        #[arg(short, long, default_value = "4096")]
        port: u16,
    },
    /// Run a single prompt (non-interactive)
    Run {
        /// The prompt to send to the agent
        prompt: String,
        /// Agent to use (default: build)
        #[arg(short, long, default_value = "build")]
        agent: String,
        /// Model to use (default: from config or anthropic/claude-sonnet-4-20250514)
        #[arg(short, long)]
        model: Option<String>,
    },
    /// Show available models
    Models,
    /// Show version info
    Version,
    /// List sessions for the current project
    Sessions,
    /// Show project info
    Project,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let dir = cli
        .directory
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    let config = match Config::load(&dir) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error loading config: {e}");
            process::exit(1);
        }
    };

    let db = match Database::open(&global::db_path()) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("error opening database: {e}");
            process::exit(1);
        }
    };

    let bus = Bus::default();

    match cli.command {
        Some(Commands::Serve { port }) => {
            // Build provider and registries for server
            let model_str = config.model.as_deref().unwrap_or(DEFAULT_MODEL);
            let (provider, _model_id) = match provider_init::build_provider_with_config(model_str, &config) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("error initializing provider: {e}");
                    eprintln!("hint: set ANTHROPIC_API_KEY or OPENAI_API_KEY environment variable");
                    process::exit(1);
                }
            };

            let agent_registry = Arc::new(AgentRegistry::new());
            let tool_registry = Arc::new(ToolRegistry::with_builtins());

            let state = opencoder_server::AppState::new(
                db, bus, config, dir, provider, agent_registry, tool_registry,
            );
            if let Err(e) = opencoder_server::serve(state, port).await {
                eprintln!("server error: {e}");
                process::exit(1);
            }
        }
        Some(Commands::Run { prompt, agent, model }) => {
            let model_str = model
                .as_deref()
                .or(config.model.as_deref())
                .unwrap_or(DEFAULT_MODEL);

            let (provider, model_id) = match provider_init::build_provider_with_config(model_str, &config) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("error: {e}");
                    eprintln!("hint: set ANTHROPIC_API_KEY or OPENAI_API_KEY environment variable");
                    process::exit(1);
                }
            };

            let project_svc = ProjectService::new(db.clone());
            let session_svc = Arc::new(SessionService::new(db.clone(), bus.clone()));
            let registry = AgentRegistry::new();
            let tools = ToolRegistry::with_builtins();

            // Ensure project
            let project = match project_svc.ensure(&dir) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("error: {e}");
                    process::exit(1);
                }
            };

            // Create session
            let session = match session_svc.create(&project.id, &dir.to_string_lossy(), None) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("error creating session: {e}");
                    process::exit(1);
                }
            };

            eprintln!("\x1b[2m[model: {model_str} | agent: {agent} | session: {}]\x1b[0m", session.id);

            let cancel = tokio_util::sync::CancellationToken::new();

            // Set up Ctrl+C handler
            let cancel_clone = cancel.clone();
            tokio::spawn(async move {
                tokio::signal::ctrl_c().await.ok();
                eprintln!("\n\x1b[33mCancelling...\x1b[0m");
                cancel_clone.cancel();
            });

            let loop_config = AgentLoopConfig {
                session_id: session.id.clone(),
                project_id: project.id.clone(),
                agent_name: agent.clone(),
                model: model_id,
                provider,
                cancel,
                project_dir: dir.clone(),
                config: config.clone(),
            };

            // Start streaming output in background
            let bus_clone = bus.clone();
            let session_id_clone = session.id.clone();
            let output_handle = tokio::spawn(async move {
                output::print_stream(&bus_clone, &session_id_clone).await;
            });

            // Run agent loop
            if let Err(e) = agent_loop::run(
                loop_config,
                &prompt,
                session_svc.clone(),
                &registry,
                tools.all().clone(),
                &bus,
            ).await {
                eprintln!("\n\x1b[31mAgent error: {e}\x1b[0m");
                process::exit(1);
            }

            // Wait for output to finish
            output_handle.await.ok();
        }
        Some(Commands::Models) => {
            println!("Configured model: {}", config.model.as_deref().unwrap_or("(default)"));
            if let Some(small) = config.small_model.as_deref() {
                println!("Small model: {small}");
            }
            println!();
            println!("Built-in providers:");
            println!("  anthropic  - Anthropic (Claude)");
            println!("  openai     - OpenAI (GPT)");
            println!("  google     - Google (Gemini)");
            println!("  groq       - Groq");
            println!("  openrouter - OpenRouter");
            println!("  together   - Together AI");
            println!("  fireworks  - Fireworks AI");
            println!("  deepseek   - DeepSeek");
            println!("  mistral    - Mistral AI");
            println!("  xai        - xAI (Grok)");
            println!();
            println!("Model format: provider/model-name (e.g., anthropic/claude-opus-4-6)");
        }
        Some(Commands::Version) => {
            println!("opencoder {}", env!("CARGO_PKG_VERSION"));
        }
        Some(Commands::Sessions) => {
            let project_svc = ProjectService::new(db.clone());
            let session_svc = SessionService::new(db.clone(), bus);
            match project_svc.get_by_worktree(&dir.to_string_lossy()) {
                Ok(project) => {
                    match session_svc.list(&project.id) {
                        Ok(sessions) => {
                            if sessions.is_empty() {
                                println!("No sessions found.");
                            } else {
                                println!("{:<30} {:<40} {}", "ID", "TITLE", "CREATED");
                                for s in sessions {
                                    let ts = chrono::DateTime::from_timestamp_millis(s.time_created)
                                        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                                        .unwrap_or_default();
                                    println!("{:<30} {:<40} {}", s.id, s.title, ts);
                                }
                            }
                        }
                        Err(e) => eprintln!("error listing sessions: {e}"),
                    }
                }
                Err(_) => println!("No project found for {}", dir.display()),
            }
        }
        Some(Commands::Project) => {
            let project_svc = ProjectService::new(db.clone());
            match project_svc.ensure(&dir) {
                Ok(project) => {
                    println!("Project: {}", project.name);
                    println!("ID: {}", project.id);
                    println!("Directory: {}", project.worktree);
                    if let Some(vcs) = &project.vcs {
                        println!("VCS: {}", vcs);
                    }
                    let ts = chrono::DateTime::from_timestamp_millis(project.time_created)
                        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                        .unwrap_or_default();
                    println!("Created: {ts}");
                }
                Err(e) => eprintln!("error: {e}"),
            }
        }
        None => {
            // Default: launch TUI
            let model_str = config.model.as_deref().unwrap_or(DEFAULT_MODEL);
            let (provider, _model_id) = match provider_init::build_provider_with_config(model_str, &config) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("error initializing provider: {e}");
                    eprintln!("hint: set ANTHROPIC_API_KEY or OPENAI_API_KEY environment variable");
                    eprintln!();
                    eprintln!("Or use `opencoder run <prompt>` for non-interactive mode.");
                    process::exit(1);
                }
            };

            let session_svc = Arc::new(SessionService::new(db.clone(), bus.clone()));
            let agent_registry = Arc::new(AgentRegistry::new());
            let tool_registry = Arc::new(ToolRegistry::with_builtins());

            if let Err(e) = tui::run_tui(
                db, bus, config, dir,
                provider, tool_registry, agent_registry, session_svc,
            ).await {
                eprintln!("TUI error: {e}");
                process::exit(1);
            }
        }
    }
}
