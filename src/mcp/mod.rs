pub mod jsonrpc;
pub mod server;
pub mod tools;

use server::McpServer;

/// List registered agents for this project (opens registry DB).
fn list_project_agents(project_hash: &str) -> Vec<String> {
    let reg_path = ai_smartness::storage::path_utils::registry_db_path();
    let conn = match ai_smartness::storage::database::open_connection(
        &reg_path,
        ai_smartness::storage::database::ConnectionRole::Mcp,
    ) {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    let _ = ai_smartness::storage::migrations::migrate_registry_db(&conn);
    ai_smartness::registry::registry::AgentRegistry::list(&conn, Some(project_hash), None, None)
        .unwrap_or_default()
        .into_iter()
        .map(|a| a.id)
        .collect()
}

/// Resolve agent identity at MCP startup.
///
/// Cascade (no "default" — always resolves to a real agent):
///   1. CLI --agent-id arg
///   2. AI_SMARTNESS_AGENT_ID env var
///   3. AI_SMARTNESS_AGENT env var (compat with hooks / extension)
///   4. Single-agent shortcut (exactly 1 registered → use it)
///   5. Parent process session: read PPID cmdline → extract --resume {session_id}
///      → look up per-session agent file. This is the key mechanism for multi-panel:
///      each panel's Claude Code CLI has its own PID and session_id.
///   6. Global session file (set by `ai-smartness agent select` or extension)
///   7. First registered agent for this project
///   8. None → error, MCP cannot start without identity
fn resolve_agent(project_hash: &str, cli_agent_id: Option<&str>) -> Option<String> {
    // 1. Explicit CLI arg
    if let Some(id) = cli_agent_id {
        tracing::info!(agent = id, source = "cli_arg", "Agent resolved");
        return Some(id.to_string());
    }

    // 2. AI_SMARTNESS_AGENT_ID env var
    if let Ok(id) = std::env::var("AI_SMARTNESS_AGENT_ID") {
        if !id.is_empty() {
            tracing::info!(agent = %id, source = "env_AGENT_ID", "Agent resolved");
            return Some(id);
        }
    }

    // 3. AI_SMARTNESS_AGENT env var (same var hooks/extension use)
    if let Ok(id) = std::env::var("AI_SMARTNESS_AGENT") {
        if !id.is_empty() {
            tracing::info!(agent = %id, source = "env_AGENT", "Agent resolved");
            return Some(id);
        }
    }

    if project_hash.is_empty() {
        return None;
    }

    let agents = list_project_agents(project_hash);

    // 4. Single-agent shortcut
    if agents.len() == 1 {
        tracing::info!(agent = %agents[0], source = "single_agent", "Agent resolved");
        return Some(agents[0].clone());
    }

    // 5. Parent process session: extract session_id from PPID's --resume arg
    if let Some(agent) = resolve_from_parent_session(project_hash, &agents) {
        return Some(agent);
    }

    // 6. Global session file
    let session_path = ai_smartness::storage::path_utils::agent_session_path(project_hash);
    if let Ok(contents) = std::fs::read_to_string(&session_path) {
        let trimmed = contents.trim().to_string();
        if !trimmed.is_empty() {
            // Verify agent actually exists in registry
            if agents.contains(&trimmed) {
                tracing::info!(agent = %trimmed, source = "session_file", "Agent resolved");
                return Some(trimmed);
            }
            // Session file references unknown agent — might have been removed
            tracing::warn!(agent = %trimmed, "Session file references unregistered agent, skipping");
        }
    }

    // 7. First registered agent
    if let Some(first) = agents.first() {
        tracing::info!(agent = %first, source = "first_registered", "Agent resolved (fallback to first registered)");
        return Some(first.clone());
    }

    // 8. No agents at all
    None
}

/// Extract session_id from parent process (Claude Code CLI) command line,
/// then look up the per-session agent file to determine this MCP's agent.
///
/// The parent process is the Claude Code CLI that spawned this MCP server.
/// Its cmdline contains `--resume {session_id}` which uniquely identifies the panel.
/// The per-session agent file maps session_id → agent_id.
#[cfg(target_os = "linux")]
fn resolve_from_parent_session(project_hash: &str, agents: &[String]) -> Option<String> {
    // Read parent PID from /proc/self/stat (field 4 after the comm field)
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    let after_comm = stat.rfind(')')? + 2;
    let fields: Vec<&str> = stat[after_comm..].split_whitespace().collect();
    let ppid: u32 = fields.get(1)?.parse().ok()?;

    tracing::debug!(ppid, "Reading parent process cmdline");

    // Read parent's cmdline (null-separated args)
    let cmdline = std::fs::read_to_string(format!("/proc/{}/cmdline", ppid)).ok()?;
    let args: Vec<&str> = cmdline.split('\0').collect();

    // Find --resume {session_id}
    let session_id = args.windows(2)
        .find(|pair| pair[0] == "--resume")
        .map(|pair| pair[1])?;

    if session_id.is_empty() {
        return None;
    }

    tracing::debug!(session_id, "Extracted session_id from parent --resume arg");

    // Look up per-session agent file
    let per_session_path =
        ai_smartness::storage::path_utils::per_session_agent_path(project_hash, session_id);
    let agent_id = std::fs::read_to_string(&per_session_path).ok()?;
    let agent_id = agent_id.trim().to_string();

    if agent_id.is_empty() {
        return None;
    }

    // Verify agent exists in registry
    if agents.contains(&agent_id) {
        tracing::info!(
            agent = %agent_id,
            session_id,
            source = "parent_session",
            "Agent resolved from parent process session"
        );
        return Some(agent_id);
    }

    tracing::warn!(
        agent = %agent_id,
        session_id,
        "Per-session agent not found in registry"
    );
    None
}

#[cfg(not(target_os = "linux"))]
fn resolve_from_parent_session(_project_hash: &str, _agents: &[String]) -> Option<String> {
    // On non-Linux platforms, this resolution step is not available.
    // Falls through to global session file or first registered agent.
    None
}

/// Run MCP JSON-RPC server on stdin/stdout.
pub fn run(project_hash: Option<&str>, agent_id: Option<&str>) {
    let project_hash = project_hash
        .map(|s| s.to_string())
        .or_else(|| std::env::var("AI_SMARTNESS_PROJECT_HASH").ok())
        .unwrap_or_default();

    let project_hash = if project_hash.is_empty() {
        let cwd = std::env::current_dir().unwrap_or_default();
        ai_smartness::storage::path_utils::project_hash(&cwd).unwrap_or_default()
    } else {
        project_hash
    };

    // Initialize tracing to daemon.log (append mode) so MCP tool calls
    // appear in the Debug Console. Must be after project_hash resolution.
    if !project_hash.is_empty() {
        ai_smartness::tracing_init::init_file_tracing(&project_hash);
    } else {
        // Fallback: stderr if no project hash available
        tracing_subscriber::fmt().with_writer(std::io::stderr).init();
        eprintln!("[ai-mcp] Warning: No project hash. Set AI_SMARTNESS_PROJECT_HASH or pass as arg.");
    }

    // Validate computed hash exists in registry
    if !project_hash.is_empty() {
        let reg_path = ai_smartness::storage::path_utils::registry_db_path();
        if let Ok(conn) = ai_smartness::storage::database::open_connection(
            &reg_path,
            ai_smartness::storage::database::ConnectionRole::Cli,
        ) {
            let exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM projects WHERE hash = ?1",
                    rusqlite::params![&project_hash],
                    |r| r.get(0),
                )
                .unwrap_or(false);
            if !exists {
                tracing::warn!(
                    hash = %&project_hash[..8.min(project_hash.len())],
                    "Computed project hash not found in registry — cwd may differ from registered path"
                );
            }
        }
    }

    // Resolve agent identity — no "default" fallback, always a real agent
    let agent_id = match resolve_agent(&project_hash, agent_id) {
        Some(id) => {
            tracing::info!(agent = %id, "MCP starting with agent identity");
            id
        }
        None => {
            eprintln!(
                "[ai-mcp] Error: No agent identity found.\n\
                 Register an agent first: ai-smartness agent add <name> --project-hash {}\n\
                 Or set AI_SMARTNESS_AGENT_ID environment variable.",
                project_hash
            );
            std::process::exit(1);
        }
    };

    match McpServer::new(project_hash, agent_id) {
        Ok(mut server) => {
            if let Err(e) = server.run() {
                eprintln!("[ai-mcp] Server error: {}", e);
            }
        }
        Err(e) => {
            eprintln!("[ai-mcp] Failed to initialize: {}", e);
            std::process::exit(1);
        }
    }
}
