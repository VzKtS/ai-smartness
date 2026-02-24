use anyhow::{Context, Result};
use ai_smartness::registry::registry::{AgentRegistry, AgentUpdate, HierarchyNode};
use ai_smartness::registry::tasks::AgentTaskStorage;
use ai_smartness::agent::{Agent, AgentStatus, CoordinationMode};
use ai_smartness::project_registry::ProjectRegistryTrait;
use ai_smartness::storage::database::{open_connection, ConnectionRole};
use ai_smartness::storage::migrations;
use ai_smartness::storage::path_utils;

use ai_smartness::constants::truncate_safe;

use super::resolve_project_hash;

pub fn add(
    id: &str,
    project_hash: Option<&str>,
    role: &str,
    supervisor: Option<&str>,
    description: &str,
    team: Option<&str>,
) -> Result<()> {
    let hash = resolve_project_hash(project_hash)?;
    eprintln!("[agent::add] id={} project={} role={} supervisor={:?} team={:?}",
        id, &hash[..8.min(hash.len())], role, supervisor, team);

    let reg_path = path_utils::registry_db_path();
    eprintln!("[agent::add] registry DB: {} (exists={})", reg_path.display(), reg_path.exists());
    let reg_conn = open_connection(&reg_path, ConnectionRole::Cli)
        .context("Failed to open registry database")?;
    migrations::migrate_registry_db(&reg_conn)
        .context("Failed to migrate registry database")?;

    // If the only agent is "main" (from init), rename it in-place (organic replacement)
    let existing = AgentRegistry::list(&reg_conn, Some(&hash), None, None)
        .unwrap_or_default();
    if existing.len() == 1 && existing[0].id == "main" && id != "main" {
        eprintln!("[agent::add] Replacing main agent in-place -> {}", id);
        AgentRegistry::rename(&reg_conn, "main", id, &hash)
            .context("Failed to rename main agent")?;

        let mode = if supervisor.is_some() {
            CoordinationMode::Supervised
        } else {
            CoordinationMode::Autonomous
        };
        let update = AgentUpdate {
            name: Some(id.to_string()),
            role: Some(role.to_string()),
            description: if description.is_empty() { None } else { Some(description.to_string()) },
            supervisor_id: supervisor.map(|s| Some(s.to_string())),
            coordination_mode: Some(mode.as_str().to_string()),
            team: team.map(|t| Some(t.to_string())),
            specializations: None,
            capabilities: None,
            thread_mode: None,
            report_to: None,
            custom_role: None,
            workspace_path: None,
            full_permissions: None,
        };
        AgentRegistry::update(&reg_conn, id, &hash, &update)
            .context("Failed to update renamed agent")?;

        println!("Agent registered: {} (replaced default)", id);
        return Ok(());
    }

    // Validate supervisor exists if provided
    if let Some(sup_id) = supervisor {
        let sup = AgentRegistry::get(&reg_conn, sup_id, &hash)
            .context("Failed to check supervisor")?;
        if sup.is_none() {
            anyhow::bail!("Supervisor '{}' not found in project {}", sup_id, &hash[..8]);
        }
        // Validate no cycle
        AgentRegistry::validate_hierarchy(&reg_conn, id, sup_id, &hash)
            .context("Hierarchy validation failed")?;
    }

    let mode = if supervisor.is_some() {
        CoordinationMode::Supervised
    } else {
        CoordinationMode::Autonomous
    };

    let agent = Agent {
        id: id.to_string(),
        project_hash: hash.clone(),
        name: id.to_string(),
        description: description.to_string(),
        role: role.to_string(),
        capabilities: vec![],
        status: AgentStatus::Active,
        last_seen: chrono::Utc::now(),
        registered_at: chrono::Utc::now(),
        supervisor_id: supervisor.map(|s| s.to_string()),
        coordination_mode: mode,
        team: team.map(|t| t.to_string()),
        specializations: vec![],
        thread_mode: ai_smartness::agent::ThreadMode::Normal,
        current_activity: String::new(),
        report_to: None,
        custom_role: None,
        workspace_path: String::new(),
        full_permissions: false,
    };

    AgentRegistry::register(&reg_conn, &agent)
        .context("Failed to register agent")?;
    eprintln!("[agent::add] Agent registered in registry");

    // Initialize agent DB
    let agent_db_path = path_utils::agent_db_path(&hash, id);
    eprintln!("[agent::add] Agent DB path: {}", agent_db_path.display());
    let agent_conn = open_connection(&agent_db_path, ConnectionRole::Cli)
        .context("Failed to open agent database")?;
    migrations::migrate_agent_db(&agent_conn)
        .context("Failed to migrate agent database")?;
    eprintln!("[agent::add] Agent DB created and migrated");

    // Ensure Claude Code hooks are installed for this project
    let reg_conn2 = open_connection(&reg_path, ConnectionRole::Cli)
        .context("Failed to open registry database")?;
    let registry = ai_smartness::storage::project_registry_impl::SqliteProjectRegistry::new(reg_conn2);
    if let Ok(Some(project)) = registry.get_project(&hash) {
        let project_path = std::path::Path::new(&project.path);
        let hooks_exist = project_path.join(".claude/settings.json").exists();
        eprintln!("[agent::add] Project path: {} hooks_exist={}", project_path.display(), hooks_exist);
        if !hooks_exist {
            if let Err(e) = ai_smartness::hook_setup::install_claude_hooks(project_path, &hash) {
                eprintln!("[agent::add] Warning: hooks not installed: {}", e);
            } else {
                println!("Hooks installed in {}/.claude/settings.json", project_path.display());
            }
        }
    } else {
        eprintln!("[agent::add] Warning: project {} not found in registry", &hash[..8.min(hash.len())]);
    }

    println!("Agent registered: {} (role: {})", id, role);
    if let Some(sup) = supervisor {
        println!("  Supervisor: {}", sup);
    }
    if let Some(t) = team {
        println!("  Team: {}", t);
    }
    println!("  Use AI_SMARTNESS_AGENT={} to bind this agent to a session", id);

    Ok(())
}

pub fn remove(id: &str, project_hash: Option<&str>) -> Result<()> {
    let hash = resolve_project_hash(project_hash)?;

    let reg_path = path_utils::registry_db_path();
    let reg_conn = open_connection(&reg_path, ConnectionRole::Cli)
        .context("Failed to open registry database")?;

    AgentRegistry::delete(&reg_conn, id, &hash)
        .context("Failed to remove agent")?;

    println!("Agent removed: {}", id);
    Ok(())
}

pub fn list(project_hash: Option<&str>) -> Result<()> {
    let hash = resolve_project_hash(project_hash)?;

    let reg_path = path_utils::registry_db_path();
    let reg_conn = open_connection(&reg_path, ConnectionRole::Cli)
        .context("Failed to open registry database")?;

    let agents = AgentRegistry::list(&reg_conn, Some(&hash), None, None)
        .context("Failed to list agents")?;

    if agents.is_empty() {
        println!("No agents registered.");
        return Ok(());
    }

    println!(
        "{:<12}  {:<12}  {:<10}  {:<12}  {:<12}  {}",
        "ID", "ROLE", "STATUS", "SUPERVISOR", "TEAM", "MODE"
    );
    println!("{}", "-".repeat(75));

    for a in &agents {
        let sup = a
            .supervisor_id
            .as_deref()
            .unwrap_or("-");
        let team = a
            .team
            .as_deref()
            .unwrap_or("-");

        println!(
            "{:<12}  {:<12}  {:<10}  {:<12}  {:<12}  {}",
            a.id,
            a.role,
            a.status.as_str(),
            sup,
            team,
            a.coordination_mode.as_str(),
        );
    }

    println!("\nTotal: {} agents", agents.len());
    Ok(())
}

pub fn hierarchy(project_hash: Option<&str>) -> Result<()> {
    let hash = resolve_project_hash(project_hash)?;

    let reg_path = path_utils::registry_db_path();
    let reg_conn = open_connection(&reg_path, ConnectionRole::Cli)
        .context("Failed to open registry database")?;

    let tree = AgentRegistry::build_hierarchy_tree(&reg_conn, &hash)
        .context("Failed to build hierarchy tree")?;

    if tree.is_empty() {
        println!("No agents registered.");
        return Ok(());
    }

    println!("Agent Hierarchy:");
    println!();
    for node in &tree {
        print_node(node, 0);
    }

    Ok(())
}

fn print_node(node: &HierarchyNode, depth: usize) {
    let indent = "  ".repeat(depth);
    let prefix = if depth == 0 { "" } else { "|- " };
    let tasks_info = if node.active_tasks > 0 {
        format!(" [{} tasks]", node.active_tasks)
    } else {
        String::new()
    };
    let team_info = node
        .team
        .as_ref()
        .map(|t| format!(" (team: {})", t))
        .unwrap_or_default();

    println!(
        "{}{}{} [{}] {}{}{}",
        indent, prefix, node.name, node.role, node.mode, team_info, tasks_info
    );

    for sub in &node.subordinates {
        print_node(sub, depth + 1);
    }
}

pub fn tasks(id: &str, project_hash: Option<&str>) -> Result<()> {
    let hash = resolve_project_hash(project_hash)?;

    let reg_path = path_utils::registry_db_path();
    let reg_conn = open_connection(&reg_path, ConnectionRole::Cli)
        .context("Failed to open registry database")?;

    let tasks = AgentTaskStorage::list_tasks_for_agent(&reg_conn, id, &hash)
        .context("Failed to list tasks")?;

    if tasks.is_empty() {
        println!("No tasks for agent: {}", id);
        return Ok(());
    }

    println!("Tasks for agent: {}\n", id);
    println!(
        "{:<12}  {:<25}  {:<10}  {:<8}  {}",
        "ID", "TITLE", "STATUS", "PRIORITY", "ASSIGNED BY"
    );
    println!("{}", "-".repeat(72));

    for t in &tasks {
        let id_short = if t.id.len() > 11 { &t.id[..11] } else { &t.id };
        let title = if t.title.len() > 24 {
            format!("{}...", truncate_safe(&t.title, 21))
        } else {
            t.title.clone()
        };

        println!(
            "{:<12}  {:<25}  {:<10}  {:<8}  {}",
            id_short,
            title,
            t.status.as_str(),
            t.priority.as_str(),
            t.assigned_by,
        );
    }

    println!("\nTotal: {} tasks", tasks.len());
    Ok(())
}

pub fn select(id: Option<&str>, project_hash: Option<&str>) -> Result<()> {
    let hash = resolve_project_hash(project_hash)?;
    let session_path = path_utils::agent_session_path(&hash);
    eprintln!("[agent::select] project={} session_path={} agent={:?}",
        &hash[..8.min(hash.len())], session_path.display(), id);

    match id {
        Some(agent_id) => {
            // Verify agent exists
            let reg_path = path_utils::registry_db_path();
            let reg_conn = open_connection(&reg_path, ConnectionRole::Cli)
                .context("Failed to open registry database")?;

            let agent = AgentRegistry::get(&reg_conn, agent_id, &hash)
                .context("Failed to query agent")?;

            if agent.is_none() {
                // List available agents
                let agents = AgentRegistry::list(&reg_conn, Some(&hash), None, None)
                    .unwrap_or_default();
                if agents.is_empty() {
                    anyhow::bail!("Agent '{}' not found. No agents registered for this project.", agent_id);
                }
                let available: Vec<&str> = agents.iter().map(|a| a.id.as_str()).collect();
                anyhow::bail!(
                    "Agent '{}' not found. Available: {}",
                    agent_id,
                    available.join(", ")
                );
            }

            // Write session file
            if let Some(parent) = session_path.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            std::fs::write(&session_path, agent_id)
                .context("Failed to write session file")?;
            eprintln!("[agent::select] Session file written: {} -> {}", session_path.display(), agent_id);

            let a = agent.unwrap();
            println!("Session agent: {} ({})", a.name, a.role);
            println!("Profile active for project {}", &hash[..8]);
        }
        None => {
            // Clear session binding
            if session_path.exists() {
                std::fs::remove_file(&session_path)
                    .context("Failed to remove session file")?;
                println!("Session agent cleared.");
            } else {
                println!("No session agent was set.");
            }
        }
    }

    Ok(())
}
