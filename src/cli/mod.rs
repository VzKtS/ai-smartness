pub mod agent;
pub mod bridges;
pub mod config;
pub mod daemon;
pub mod init;
pub mod project;
pub mod search;
pub mod status;
pub mod threads;

use anyhow::{Context, Result};

/// Resolve project hash: use provided value or compute from current directory.
/// Validates that the hash corresponds to a registered project.
pub fn resolve_project_hash(provided: Option<&str>) -> Result<String> {
    let hash = match provided {
        Some(h) => h.to_string(),
        None => {
            let cwd = std::env::current_dir().context("Failed to get current directory")?;
            ai_smartness::storage::path_utils::project_hash(&cwd)
                .context("Failed to compute project hash from current directory")?
        }
    };

    // Validate hash exists in registry
    let reg_path = ai_smartness::storage::path_utils::registry_db_path();
    if let Ok(conn) = ai_smartness::storage::database::open_connection(
        &reg_path,
        ai_smartness::storage::database::ConnectionRole::Cli,
    ) {
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM projects WHERE hash = ?1",
                rusqlite::params![&hash],
                |r| r.get(0),
            )
            .unwrap_or(false);

        if !exists {
            anyhow::bail!(
                "No project registered with hash '{}' (from current directory). \
                 Use 'ai-smartness project add .' first, or specify --project-hash.",
                &hash[..8.min(hash.len())]
            );
        }
    }

    Ok(hash)
}

/// Resolve agent_id for CLI commands.
///
/// Cascade: explicit → AI_SMARTNESS_AGENT_ID → AI_SMARTNESS_AGENT → single-agent → session file → first registered
pub fn resolve_agent_id(explicit: Option<&str>, project_hash: &str) -> Result<String> {
    // 1. Explicit CLI arg
    if let Some(id) = explicit {
        return Ok(id.to_string());
    }
    // 2. AI_SMARTNESS_AGENT_ID env var
    if let Ok(id) = std::env::var("AI_SMARTNESS_AGENT_ID") {
        if !id.is_empty() { return Ok(id); }
    }
    // 3. AI_SMARTNESS_AGENT env var
    if let Ok(id) = std::env::var("AI_SMARTNESS_AGENT") {
        if !id.is_empty() { return Ok(id); }
    }
    // 4-6. Registry-based resolution
    let reg_path = ai_smartness::storage::path_utils::registry_db_path();
    let conn = ai_smartness::storage::database::open_connection(
        &reg_path,
        ai_smartness::storage::database::ConnectionRole::Cli,
    ).context("Failed to open registry database")?;
    let _ = ai_smartness::storage::migrations::migrate_registry_db(&conn);
    let agents = ai_smartness::registry::registry::AgentRegistry::list(
        &conn, Some(project_hash), None, None,
    ).unwrap_or_default();

    // 4. Single-agent shortcut
    if agents.len() == 1 {
        return Ok(agents[0].id.clone());
    }
    // 5. Global session file
    let session_path = ai_smartness::storage::path_utils::agent_session_path(project_hash);
    if let Ok(contents) = std::fs::read_to_string(&session_path) {
        let trimmed = contents.trim().to_string();
        if !trimmed.is_empty() && agents.iter().any(|a| a.id == trimmed) {
            return Ok(trimmed);
        }
    }
    // 6. First registered agent
    if let Some(first) = agents.first() {
        return Ok(first.id.clone());
    }
    anyhow::bail!("No agent found. Register an agent first: ai-smartness agent add <name>")
}
