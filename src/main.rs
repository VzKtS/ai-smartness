mod cli;
mod daemon;
mod gui;
mod hook;
mod mcp;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "ai-smartness", version, about = "AI Smartness — Cognitive Memory for AI Agents")]
struct App {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Open admin GUI dashboard
    Gui,
    /// Initialize AI Smartness for current project
    Init {
        /// Project path (defaults to current directory)
        path: Option<String>,
    },
    /// Show memory status
    Status {
        #[arg(long)]
        project_hash: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },
    /// List or filter threads
    Threads {
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        project_hash: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },
    /// List bridges
    Bridges {
        #[arg(long)]
        project_hash: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },
    /// Search threads
    Search {
        query: String,
        #[arg(long)]
        project_hash: Option<String>,
        #[arg(long)]
        agent_id: Option<String>,
    },
    /// Manage daemon
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },
    /// AI provider hook (inject/capture/health)
    Hook {
        #[command(subcommand)]
        action: HookAction,
    },
    /// Run MCP server (JSON-RPC on stdin/stdout)
    Mcp {
        /// Project hash
        project_hash: Option<String>,
        /// Agent ID
        agent_id: Option<String>,
    },
    /// View or modify configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Manage projects
    Project {
        #[command(subcommand)]
        action: ProjectAction,
    },
    /// Manage agents
    Agent {
        #[command(subcommand)]
        action: AgentAction,
    },
    /// Download ONNX Runtime for neural embeddings
    SetupOnnx {
        /// Force re-download even if already installed
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Display the full configuration
    Show,
    /// Get a config value (dot notation: hooks.guard_write_enabled)
    Get {
        /// Config key (dot notation)
        key: String,
    },
    /// Set a config value and propagate to projects
    Set {
        /// Config key (dot notation)
        key: String,
        /// Value (JSON: true, false, 42, "string")
        value: String,
    },
}

#[derive(Subcommand)]
enum DaemonAction {
    /// Start the global daemon (background)
    Start,
    /// Stop the daemon
    Stop,
    /// Show daemon status
    Status,
    /// Run daemon in foreground (used internally by 'start')
    RunForeground {
        /// [DEPRECATED] Ignored — global daemon serves all projects
        #[arg(long, hide = true)]
        project_hash: Option<String>,
        /// [DEPRECATED] Ignored — global daemon serves all agents
        #[arg(long, hide = true)]
        agent_id: Option<String>,
    },
}

#[derive(Subcommand)]
enum HookAction {
    /// Inject memory context into AI prompt
    Inject {
        /// Project hash
        project_hash: String,
        /// Agent ID (auto-detected if not specified)
        agent_id: Option<String>,
    },
    /// Capture tool output
    Capture {
        /// Project hash
        project_hash: String,
        /// Agent ID (auto-detected if not specified)
        agent_id: Option<String>,
    },
    /// Health check and self-heal
    Health {
        /// Project hash
        project_hash: String,
        /// Agent ID (auto-detected if not specified)
        agent_id: Option<String>,
    },
    /// PreToolUse dispatcher (guard-write + virtual paths)
    Pretool {
        /// Project hash
        project_hash: String,
        /// Agent ID (auto-detected if not specified)
        agent_id: Option<String>,
    },
}

#[derive(Subcommand)]
enum ProjectAction {
    /// Register a project
    Add {
        path: String,
        #[arg(long)]
        name: Option<String>,
    },
    /// Unregister a project
    Remove { hash: String },
    /// List registered projects
    List,
}

#[derive(Subcommand)]
enum AgentAction {
    /// Register a new agent
    Add {
        id: String,
        #[arg(long)]
        project_hash: Option<String>,
        #[arg(long, default_value = "developer")]
        role: String,
        #[arg(long)]
        supervisor: Option<String>,
        #[arg(long, default_value = "")]
        description: String,
        #[arg(long)]
        team: Option<String>,
    },
    /// Remove an agent
    Remove {
        id: String,
        #[arg(long)]
        project_hash: Option<String>,
    },
    /// List agents
    List {
        #[arg(long)]
        project_hash: Option<String>,
    },
    /// Show agent hierarchy tree
    Hierarchy {
        #[arg(long)]
        project_hash: Option<String>,
    },
    /// List tasks for an agent
    Tasks {
        id: String,
        #[arg(long)]
        project_hash: Option<String>,
    },
    /// Select agent profile for the current project session
    Select {
        /// Agent ID to bind (omit to clear)
        id: Option<String>,
        #[arg(long)]
        project_hash: Option<String>,
    },
}

fn main() {
    let app = App::parse();

    match app.command {
        // No subcommand or Gui → launch GUI
        None | Some(Commands::Gui) => {
            gui::launch();
        }

        // Hook: ALWAYS exit 0
        Some(Commands::Hook { action }) => {
            let hook_action = match action {
                HookAction::Inject { project_hash, agent_id } => {
                    hook::HookAction::Inject { project_hash, agent_id }
                }
                HookAction::Capture { project_hash, agent_id } => {
                    hook::HookAction::Capture { project_hash, agent_id }
                }
                HookAction::Health { project_hash, agent_id } => {
                    hook::HookAction::Health { project_hash, agent_id }
                }
                HookAction::Pretool { project_hash, agent_id } => {
                    hook::HookAction::PreTool { project_hash, agent_id }
                }
            };
            hook::run(hook_action);
        }


        // MCP: JSON-RPC on stdin/stdout
        Some(Commands::Mcp { project_hash, agent_id }) => {
            mcp::run(project_hash.as_deref(), agent_id.as_deref());
        }

        // Daemon: run-foreground is the actual global daemon process
        Some(Commands::Daemon { action }) => match action {
            DaemonAction::RunForeground { project_hash, agent_id } => {
                if project_hash.is_some() || agent_id.is_some() {
                    eprintln!("[ai-daemon] Warning: --project-hash and --agent-id are deprecated. Global daemon serves all projects.");
                }
                daemon::run();
            }
            DaemonAction::Start => {
                cli::daemon::start()
                    .unwrap_or_else(|e| eprintln!("Error: {}", e));
            }
            DaemonAction::Stop => {
                cli::daemon::stop()
                    .unwrap_or_else(|e| eprintln!("Error: {}", e));
            }
            DaemonAction::Status => {
                cli::daemon::status()
                    .unwrap_or_else(|e| eprintln!("Error: {}", e));
            }
        },

        // CLI commands
        Some(Commands::Init { path }) => {
            cli::init::run(path.as_deref())
                .unwrap_or_else(|e| eprintln!("Error: {}", e));
        }
        Some(Commands::Status { project_hash, agent_id }) => {
            cli::status::run(project_hash.as_deref(), agent_id.as_deref())
                .unwrap_or_else(|e| eprintln!("Error: {}", e));
        }
        Some(Commands::Threads { status, project_hash, agent_id }) => {
            cli::threads::run(status.as_deref(), project_hash.as_deref(), agent_id.as_deref())
                .unwrap_or_else(|e| eprintln!("Error: {}", e));
        }
        Some(Commands::Bridges { project_hash, agent_id }) => {
            cli::bridges::run(project_hash.as_deref(), agent_id.as_deref())
                .unwrap_or_else(|e| eprintln!("Error: {}", e));
        }
        Some(Commands::Search { query, project_hash, agent_id }) => {
            cli::search::run(&query, project_hash.as_deref(), agent_id.as_deref())
                .unwrap_or_else(|e| eprintln!("Error: {}", e));
        }
        Some(Commands::Config { action }) => {
            let result = match action {
                ConfigAction::Show => cli::config::run_show(),
                ConfigAction::Get { key } => cli::config::run_get(&key),
                ConfigAction::Set { key, value } => cli::config::run_set(&key, &value),
            };
            result.unwrap_or_else(|e| eprintln!("Error: {}", e));
        }
        Some(Commands::Project { action }) => match action {
            ProjectAction::Add { path, name } => {
                cli::project::add(&path, name.as_deref())
                    .unwrap_or_else(|e| eprintln!("Error: {}", e));
            }
            ProjectAction::Remove { hash } => {
                cli::project::remove(&hash)
                    .unwrap_or_else(|e| eprintln!("Error: {}", e));
            }
            ProjectAction::List => {
                cli::project::list()
                    .unwrap_or_else(|e| eprintln!("Error: {}", e));
            }
        },
        Some(Commands::Agent { action }) => match action {
            AgentAction::Add { id, project_hash, role, supervisor, description, team } => {
                cli::agent::add(&id, project_hash.as_deref(), &role, supervisor.as_deref(), &description, team.as_deref())
                    .unwrap_or_else(|e| eprintln!("Error: {}", e));
            }
            AgentAction::Remove { id, project_hash } => {
                cli::agent::remove(&id, project_hash.as_deref())
                    .unwrap_or_else(|e| eprintln!("Error: {}", e));
            }
            AgentAction::List { project_hash } => {
                cli::agent::list(project_hash.as_deref())
                    .unwrap_or_else(|e| eprintln!("Error: {}", e));
            }
            AgentAction::Hierarchy { project_hash } => {
                cli::agent::hierarchy(project_hash.as_deref())
                    .unwrap_or_else(|e| eprintln!("Error: {}", e));
            }
            AgentAction::Tasks { id, project_hash } => {
                cli::agent::tasks(&id, project_hash.as_deref())
                    .unwrap_or_else(|e| eprintln!("Error: {}", e));
            }
            AgentAction::Select { id, project_hash } => {
                cli::agent::select(id.as_deref(), project_hash.as_deref())
                    .unwrap_or_else(|e| eprintln!("Error: {}", e));
            }
        },
        Some(Commands::SetupOnnx { force }) => {
            cli::setup_onnx::run(force)
                .unwrap_or_else(|e| eprintln!("Error: {}", e));
        }
    }
}
