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
    resolve_agent_late(project_hash, &agents)
}

/// Core resolution logic (steps 4–7), testable with injected agents list.
fn resolve_agent_late(project_hash: &str, agents: &[String]) -> Option<String> {
    // 4. Single-agent shortcut
    if agents.len() == 1 {
        tracing::info!(agent = %agents[0], source = "single_agent", "Agent resolved");
        return Some(agents[0].clone());
    }

    // 5. Parent process session: extract session_id from PPID's --resume arg
    if let Some(agent) = resolve_from_parent_session(project_hash, agents) {
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

    // No silent fallback. Unassigned sessions must be explicitly assigned.
    // This prevents "session theft" where the alphabetically-first agent (e.g. arc)
    // gets hijacked by every unassigned session.
    let agent_count = agents.len();
    if agent_count > 0 {
        tracing::warn!(
            agent_count,
            "No agent assigned to this session. {} agents registered but none matched. \
             Use ai_agent_select or set AI_SMARTNESS_AGENT_ID.",
            agent_count
        );
    }
    None
}

/// Extract session_id from parent process (Claude Code CLI) command line.
/// Returns the session_id if found in the parent's --resume argument, None otherwise.
#[cfg(target_os = "linux")]
fn extract_parent_session_id() -> Option<String> {
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
    Some(session_id.to_string())
}

#[cfg(not(target_os = "linux"))]
fn extract_parent_session_id() -> Option<String> {
    None
}

/// Extract session_id from parent process (Claude Code CLI) command line,
/// then look up the per-session agent file to determine this MCP's agent.
///
/// The parent process is the Claude Code CLI that spawned this MCP server.
/// Its cmdline contains `--resume {session_id}` which uniquely identifies the panel.
/// The per-session agent file maps session_id → agent_id.
fn resolve_from_parent_session(project_hash: &str, agents: &[String]) -> Option<String> {
    let session_id = extract_parent_session_id()?;

    // Look up per-session agent file
    let per_session_path =
        ai_smartness::storage::path_utils::per_session_agent_path(project_hash, &session_id);
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
                "[ai-mcp] Error: No agent assigned to this session.\n\
                 Assign an agent:  call ai_agent_select with your session_id\n\
                 Or register one:  ai-smartness agent add <name> --project-hash {}\n\
                 Or set env var:   AI_SMARTNESS_AGENT_ID=<agent_id>",
                project_hash
            );
            std::process::exit(1);
        }
    };

    // Write per-session agent file so hooks find the agent identity on first invocation.
    // This prevents hooks from falling back to the global session file when per-session
    // isolation is needed.
    //
    // Priority: per-session (session_id) > per-PID (fallback if no session_id).
    if let Some(session_id) = extract_parent_session_id() {
        let per_session_path =
            ai_smartness::storage::path_utils::per_session_agent_path(&project_hash, &session_id);
        if let Some(parent) = per_session_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&per_session_path, &agent_id) {
            tracing::warn!(
                path = %per_session_path.display(),
                session_id,
                "Failed to write per-session agent file: {}",
                e
            );
        } else {
            tracing::debug!(
                path = %per_session_path.display(),
                session_id,
                "Wrote per-session agent file at startup"
            );
        }
    } else {
        // No session_id (non-Linux, or Claude Code without --resume).
        // Write per-PID file so hooks don't fall back to global.
        let pid = std::process::id();
        let pid_path =
            ai_smartness::storage::path_utils::per_session_agent_path(&project_hash, &pid.to_string());
        if let Some(parent) = pid_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&pid_path, &agent_id) {
            tracing::warn!(
                path = %pid_path.display(),
                pid,
                "Failed to write per-PID agent file: {}",
                e
            );
        } else {
            tracing::debug!(
                path = %pid_path.display(),
                pid,
                "Wrote per-PID agent file at startup (fallback)"
            );
        }
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    // T-M30.1: Multiple agents, no session file → None (no fallback to first())
    #[test]
    fn test_resolve_no_session_no_fallback() {
        let agents = vec![
            "agent_alpha".to_string(),
            "agent_beta".to_string(),
            "agent_gamma".to_string(),
        ];
        // Use a project hash that will never have a session file on disk
        let result = resolve_agent_late("nonexistent-ph-m301", &agents);
        assert_eq!(result, None, "Must NOT fall back to first agent");
    }

    // T-M30.2: Session file points to valid agent → returns it
    #[test]
    fn test_resolve_session_correct_project() {
        let ph = "test-m302-session-ok";
        let session_path = ai_smartness::storage::path_utils::agent_session_path(ph);
        let project_dir = session_path.parent().unwrap().to_path_buf();

        // Setup: create session file pointing to agent_b
        std::fs::create_dir_all(&project_dir).unwrap();
        std::fs::write(&session_path, "agent_b").unwrap();

        let agents = vec!["agent_a".to_string(), "agent_b".to_string()];
        let result = resolve_agent_late(ph, &agents);

        // Cleanup
        let _ = std::fs::remove_dir_all(&project_dir);

        assert_eq!(result, Some("agent_b".to_string()));
    }

    // T-M30.3: Session file points to agent NOT in agents list → None (cross-project isolation)
    #[test]
    fn test_resolve_session_wrong_project_isolation() {
        let ph = "test-m303-session-iso";
        let session_path = ai_smartness::storage::path_utils::agent_session_path(ph);
        let project_dir = session_path.parent().unwrap().to_path_buf();

        // Setup: session file references an agent from a *different* project
        std::fs::create_dir_all(&project_dir).unwrap();
        std::fs::write(&session_path, "foreign_agent").unwrap();

        let agents = vec!["agent_a".to_string(), "agent_b".to_string()];
        let result = resolve_agent_late(ph, &agents);

        // Cleanup
        let _ = std::fs::remove_dir_all(&project_dir);

        assert_eq!(result, None, "Agent from different project must not resolve");
    }

    // T-M30.4: CLI arg takes priority over everything
    #[test]
    fn test_resolve_cli_arg_priority() {
        // CLI arg is checked first (step 1), before env vars or DB
        let result = resolve_agent("", Some("explicit-cli-agent"));
        assert_eq!(result, Some("explicit-cli-agent".to_string()));
    }
}
