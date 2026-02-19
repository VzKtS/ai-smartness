use anyhow::{Context, Result};
use ai_smartness::project_registry::{MessagingMode, ProjectEntry, ProjectRegistryTrait};
use ai_smartness::storage::database::{open_connection, ConnectionRole};
use ai_smartness::storage::migrations;
use ai_smartness::storage::path_utils;
use ai_smartness::storage::project_registry_impl::SqliteProjectRegistry;

pub fn add(path: &str, name: Option<&str>) -> Result<()> {
    let project_path = std::path::PathBuf::from(path)
        .canonicalize()
        .context("Failed to resolve project path")?;

    let hash = path_utils::project_hash(&project_path)
        .context("Failed to compute project hash")?;

    let project_name = name
        .map(|n| n.to_string())
        .unwrap_or_else(|| {
            project_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string())
        });

    let reg_path = path_utils::registry_db_path();
    let reg_conn = open_connection(&reg_path, ConnectionRole::Cli)
        .context("Failed to open registry database")?;
    migrations::migrate_registry_db(&reg_conn)
        .context("Failed to migrate registry database")?;

    let entry = ProjectEntry {
        hash: hash.clone(),
        path: project_path.to_string_lossy().to_string(),
        name: Some(project_name.clone()),
        provider: "claude".to_string(),
        messaging_mode: MessagingMode::Cognitive,
        provider_config: serde_json::json!({}),
        created_at: chrono::Utc::now(),
        last_accessed: Some(chrono::Utc::now()),
    };

    let mut registry = SqliteProjectRegistry::new(reg_conn);
    registry.add_project(entry)
        .context("Failed to add project")?;

    // Ensure agents/ directory exists
    let agents_dir = path_utils::project_dir(&hash).join("agents");
    let _ = std::fs::create_dir_all(&agents_dir);

    // Install Claude Code hooks
    if let Err(e) = ai_smartness::hook_setup::install_claude_hooks(&project_path, &hash) {
        eprintln!("Warning: failed to install hooks: {}", e);
    } else {
        println!("Hooks installed in {}/.claude/settings.json", project_path.display());
    }

    // Install MCP server config
    if let Err(e) = ai_smartness::hook_setup::install_mcp_config(&project_path, &hash) {
        eprintln!("Warning: failed to install MCP config: {}", e);
    } else {
        println!("MCP config installed in {}/.mcp.json", project_path.display());
    }

    println!("Project registered: {} ({})", project_name, &hash[..8]);
    Ok(())
}

pub fn remove(hash: &str) -> Result<()> {
    let reg_path = path_utils::registry_db_path();
    let reg_conn = open_connection(&reg_path, ConnectionRole::Cli)
        .context("Failed to open registry database")?;

    // 1. List agents before deletion (for wake signal cleanup)
    let agents: Vec<String> = reg_conn
        .prepare("SELECT id FROM agents WHERE project_hash = ?1")
        .and_then(|mut stmt| {
            stmt.query_map(rusqlite::params![hash], |row| row.get::<_, String>(0))
                .map(|rows| rows.flatten().collect())
        })
        .unwrap_or_default();

    // 2. Remove agents + tasks from registry (bypass last-agent check)
    let _ = reg_conn.execute(
        "DELETE FROM agent_tasks WHERE assigned_to IN \
         (SELECT id FROM agents WHERE project_hash = ?1)",
        rusqlite::params![hash],
    );
    let _ = reg_conn.execute(
        "DELETE FROM agents WHERE project_hash = ?1",
        rusqlite::params![hash],
    );

    // 3. Remove project from registry
    let mut registry = SqliteProjectRegistry::new(reg_conn);
    registry.remove_project(hash)
        .context("Failed to remove project")?;

    // 4. Remove entire project directory
    let project_dir = path_utils::project_dir(hash);
    if project_dir.exists() {
        std::fs::remove_dir_all(&project_dir)
            .context("Failed to remove project directory")?;
    }

    // 5. Remove wake signals (separate directory)
    for agent_id in &agents {
        let wake_path = path_utils::wake_signal_path(agent_id);
        if wake_path.exists() {
            let _ = std::fs::remove_file(&wake_path);
        }
    }

    println!("Project removed: {} ({} agents cleaned)", hash, agents.len());
    Ok(())
}

pub fn list() -> Result<()> {
    let reg_path = path_utils::registry_db_path();
    let reg_conn = open_connection(&reg_path, ConnectionRole::Cli)
        .context("Failed to open registry database")?;

    let registry = SqliteProjectRegistry::new(reg_conn);
    let projects = registry.list_projects()
        .context("Failed to list projects")?;

    if projects.is_empty() {
        println!("No projects registered.");
        return Ok(());
    }

    println!(
        "{:<10}  {:<20}  {:<40}  {}",
        "HASH", "NAME", "PATH", "LAST ACCESSED"
    );
    println!("{}", "-".repeat(80));

    for p in &projects {
        let hash_short = if p.hash.len() > 9 { &p.hash[..9] } else { &p.hash };
        let name_str = p.name.as_deref().unwrap_or("unnamed");
        let name = if name_str.len() > 19 {
            format!("{}...", &name_str[..16])
        } else {
            name_str.to_string()
        };
        let path_display = if p.path.len() > 39 {
            format!("...{}", &p.path[p.path.len() - 36..])
        } else {
            p.path.clone()
        };
        let accessed = p
            .last_accessed
            .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "never".to_string());

        println!(
            "{:<10}  {:<20}  {:<40}  {}",
            hash_short, name, path_display, accessed
        );
    }

    println!("\nTotal: {} projects", projects.len());
    Ok(())
}
