use ai_smartness::config::GuardianConfig;
use ai_smartness::storage::database::{open_connection, ConnectionRole};
use ai_smartness::storage::path_utils;
use ai_smartness::storage::migrations;
use ai_smartness::storage::threads::ThreadStorage;
use ai_smartness::storage::bridges::BridgeStorage;
use ai_smartness::thread::ThreadStatus;
use ai_smartness::processing::daemon_ipc_client;
use ai_smartness::project_registry::{MessagingMode, ProjectEntry, ProjectRegistryTrait};
use ai_smartness::storage::project_registry_impl::SqliteProjectRegistry;
use ai_smartness::registry::registry::{AgentRegistry, AgentUpdate, HierarchyNode};
use ai_smartness::agent::{Agent, AgentStatus, CoordinationMode};

// ─── Dashboard ───────────────────────────────────────────────

#[tauri::command]
pub fn get_dashboard(project_hash: String, agent_id: String) -> Result<serde_json::Value, String> {
    tracing::info!(project = %&project_hash[..8.min(project_hash.len())], agent = %agent_id, "GUI: get_dashboard");
    let agent_db = path_utils::agent_db_path(&project_hash, &agent_id);
    let conn = open_connection(&agent_db, ConnectionRole::Cli)
        .map_err(|e| e.to_string())?;
    migrations::migrate_agent_db(&conn).map_err(|e| e.to_string())?;

    let active = ThreadStorage::count_by_status(&conn, &ThreadStatus::Active)
        .map_err(|e| e.to_string())?;
    let suspended = ThreadStorage::count_by_status(&conn, &ThreadStatus::Suspended)
        .map_err(|e| e.to_string())?;
    let archived = ThreadStorage::count_by_status(&conn, &ThreadStatus::Archived)
        .map_err(|e| e.to_string())?;
    let bridge_count = BridgeStorage::list_all(&conn)
        .map(|b| b.len())
        .unwrap_or(0);

    let (daemon_running, daemon_pid) = check_daemon();

    Ok(serde_json::json!({
        "daemon_status": {
            "running": daemon_running,
            "pid": daemon_pid,
            "version": env!("CARGO_PKG_VERSION"),
        },
        "thread_counts": {
            "active": active,
            "suspended": suspended,
            "archived": archived,
            "total": active + suspended + archived,
        },
        "bridge_count": bridge_count,
    }))
}

/// Aggregated dashboard for all agents in a project.
#[tauri::command]
pub fn get_project_overview(project_hash: String) -> Result<serde_json::Value, String> {
    tracing::info!(project = %&project_hash[..8.min(project_hash.len())], "GUI: get_project_overview");
    // 1. List all agents from registry
    let reg_path = path_utils::registry_db_path();
    let reg_conn = open_connection(&reg_path, ConnectionRole::Cli)
        .map_err(|e| e.to_string())?;
    migrations::migrate_registry_db(&reg_conn).map_err(|e| e.to_string())?;

    let agents = AgentRegistry::list(&reg_conn, Some(&project_hash), None, None)
        .map_err(|e| e.to_string())?;

    // 2. Aggregate metrics per agent
    let mut total_active: usize = 0;
    let mut total_suspended: usize = 0;
    let mut total_archived: usize = 0;
    let mut total_bridges: usize = 0;
    let mut agent_metrics = Vec::new();

    for agent in &agents {
        let agent_db = path_utils::agent_db_path(&project_hash, &agent.id);
        let (active, suspended, archived, bridges) = if let Ok(conn) = open_connection(&agent_db, ConnectionRole::Cli) {
            let _ = migrations::migrate_agent_db(&conn);
            let a = ThreadStorage::count_by_status(&conn, &ThreadStatus::Active).unwrap_or(0);
            let s = ThreadStorage::count_by_status(&conn, &ThreadStatus::Suspended).unwrap_or(0);
            let ar = ThreadStorage::count_by_status(&conn, &ThreadStatus::Archived).unwrap_or(0);
            let b = BridgeStorage::count(&conn).unwrap_or(0);
            (a, s, ar, b)
        } else {
            (0, 0, 0, 0)
        };

        total_active += active;
        total_suspended += suspended;
        total_archived += archived;
        total_bridges += bridges;

        agent_metrics.push(serde_json::json!({
            "id": agent.id,
            "name": agent.name,
            "role": agent.role,
            "active": active,
            "suspended": suspended,
            "archived": archived,
            "bridges": bridges,
        }));
    }

    // 3. Daemon status
    let (daemon_running, daemon_pid) = check_daemon();

    Ok(serde_json::json!({
        "daemon_status": {
            "running": daemon_running,
            "pid": daemon_pid,
            "version": env!("CARGO_PKG_VERSION"),
        },
        "totals": {
            "active": total_active,
            "suspended": total_suspended,
            "archived": total_archived,
            "bridges": total_bridges,
        },
        "agents": agent_metrics,
    }))
}

// ─── Daemon control ──────────────────────────────────────────

#[tauri::command]
pub fn daemon_status() -> Result<serde_json::Value, String> {
    tracing::info!("GUI: daemon_status");
    let (running, pid) = check_daemon();
    Ok(serde_json::json!({ "running": running, "pid": pid }))
}

#[tauri::command]
pub fn daemon_start() -> Result<serde_json::Value, String> {
    tracing::info!("GUI: daemon_start");
    let self_bin = std::env::current_exe().map_err(|e| e.to_string())?;
    let child = std::process::Command::new(&self_bin)
        .args(["daemon", "run-foreground"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;

    Ok(serde_json::json!({ "started": true, "pid": child.id() }))
}

#[tauri::command]
pub fn daemon_stop() -> Result<serde_json::Value, String> {
    tracing::info!("GUI: daemon_stop");
    match daemon_ipc_client::shutdown() {
        Ok(_) => Ok(serde_json::json!({ "stopped": true })),
        Err(e) => Ok(serde_json::json!({ "stopped": false, "error": e.to_string() })),
    }
}

// ─── Projects ────────────────────────────────────────────────

#[tauri::command]
pub fn list_projects() -> Result<serde_json::Value, String> {
    tracing::info!("GUI: list_projects");
    let reg_path = path_utils::registry_db_path();
    let reg_conn = open_connection(&reg_path, ConnectionRole::Cli)
        .map_err(|e| e.to_string())?;
    migrations::migrate_registry_db(&reg_conn).map_err(|e| e.to_string())?;

    let registry = SqliteProjectRegistry::new(reg_conn);
    let projects = registry.list_projects().map_err(|e| e.to_string())?;

    let result: Vec<serde_json::Value> = projects.iter().map(|p| {
        serde_json::json!({
            "hash": p.hash,
            "path": p.path,
            "name": p.name,
            "provider": p.provider,
        })
    }).collect();
    Ok(serde_json::json!(result))
}

#[tauri::command]
pub fn add_project(path: String, name: Option<String>) -> Result<serde_json::Value, String> {
    tracing::info!(path = %path, name = ?name, "GUI: add_project called");
    let project_path = std::path::PathBuf::from(&path)
        .canonicalize()
        .map_err(|e| format!("Invalid path: {}", e))?;

    let hash = path_utils::project_hash(&project_path)
        .map_err(|e| e.to_string())?;
    tracing::info!(hash = %&hash[..8.min(hash.len())], canonical = %project_path.display(), "GUI: project hash computed");

    let project_name = name.unwrap_or_else(|| {
        project_path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string())
    });

    let reg_path = path_utils::registry_db_path();
    let reg_conn = open_connection(&reg_path, ConnectionRole::Cli)
        .map_err(|e| e.to_string())?;
    migrations::migrate_registry_db(&reg_conn).map_err(|e| e.to_string())?;

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
    registry.add_project(entry).map_err(|e| e.to_string())?;

    // Ensure agents/ directory exists
    let agents_dir = path_utils::project_dir(&hash).join("agents");
    let _ = std::fs::create_dir_all(&agents_dir);

    // Initialize agent DB for this project (main agent)
    let agent_db = path_utils::agent_db_path(&hash, "main");
    tracing::info!(path = %agent_db.display(), "GUI: creating main agent DB");
    if let Ok(conn) = open_connection(&agent_db, ConnectionRole::Cli) {
        let _ = migrations::migrate_agent_db(&conn);
        tracing::info!("GUI: main agent DB created");
    } else {
        tracing::warn!(path = %agent_db.display(), "GUI: failed to create main agent DB");
    }

    // Install Claude Code hooks
    match ai_smartness::hook_setup::install_claude_hooks(&project_path, &hash) {
        Ok(()) => tracing::info!(path = %project_path.display(), "GUI: hooks installed"),
        Err(e) => tracing::warn!(error = %e, "GUI: failed to install hooks"),
    }

    // Install MCP server config
    match ai_smartness::hook_setup::install_mcp_config(&project_path, &hash) {
        Ok(()) => tracing::info!(path = %project_path.display(), "GUI: MCP config installed"),
        Err(e) => tracing::warn!(error = %e, "GUI: failed to install MCP config"),
    }

    // Write default config.json if it doesn't exist
    let config_path = path_utils::data_dir().join("config.json");
    if !config_path.exists() {
        let default_config = ai_smartness::config::GuardianConfig::default();
        if let Ok(json) = serde_json::to_string_pretty(&default_config) {
            match std::fs::write(&config_path, json) {
                Ok(()) => tracing::info!(path = %config_path.display(), "GUI: default config.json created"),
                Err(e) => tracing::warn!(error = %e, "GUI: failed to write config.json"),
            }
        }
    }

    tracing::info!(hash = %&hash[..8.min(hash.len())], "GUI: add_project completed");
    Ok(serde_json::json!({
        "hash": hash,
        "name": project_name,
        "path": project_path.to_string_lossy(),
    }))
}

#[tauri::command]
pub fn update_project(
    hash: String,
    name: Option<String>,
    path: Option<String>,
    provider: Option<String>,
) -> Result<serde_json::Value, String> {
    tracing::info!(hash = %&hash[..8.min(hash.len())], "GUI: update_project");
    let reg_path = path_utils::registry_db_path();
    let reg_conn = open_connection(&reg_path, ConnectionRole::Cli)
        .map_err(|e| e.to_string())?;
    migrations::migrate_registry_db(&reg_conn).map_err(|e| e.to_string())?;

    let mut registry = SqliteProjectRegistry::new(reg_conn);
    registry.update_project(
        &hash,
        name.as_deref(),
        path.as_deref(),
        provider.as_deref(),
    ).map_err(|e| e.to_string())?;

    Ok(serde_json::json!({ "updated": true }))
}

#[tauri::command]
pub fn remove_project(hash: String) -> Result<serde_json::Value, String> {
    tracing::info!(hash = %&hash[..8.min(hash.len())], "GUI: remove_project");
    let reg_path = path_utils::registry_db_path();
    let reg_conn = open_connection(&reg_path, ConnectionRole::Cli)
        .map_err(|e| e.to_string())?;
    migrations::migrate_registry_db(&reg_conn).map_err(|e| e.to_string())?;

    // 1. List agents before deletion (for wake signal cleanup)
    let agents: Vec<String> = reg_conn
        .prepare("SELECT id FROM agents WHERE project_hash = ?1")
        .and_then(|mut stmt| {
            stmt.query_map(rusqlite::params![&hash], |row| row.get::<_, String>(0))
                .map(|rows| rows.flatten().collect())
        })
        .unwrap_or_default();

    // 2. Remove agents + tasks from registry (bypass last-agent check)
    let _ = reg_conn.execute(
        "DELETE FROM agent_tasks WHERE assigned_to IN \
         (SELECT id FROM agents WHERE project_hash = ?1)",
        rusqlite::params![&hash],
    );
    let _ = reg_conn.execute(
        "DELETE FROM agents WHERE project_hash = ?1",
        rusqlite::params![&hash],
    );

    // 3. Remove project from registry
    let mut registry = SqliteProjectRegistry::new(reg_conn);
    registry.remove_project(&hash).map_err(|e| e.to_string())?;

    // 4. Remove entire project directory
    let project_dir = path_utils::project_dir(&hash);
    if project_dir.exists() {
        let _ = std::fs::remove_dir_all(&project_dir);
        tracing::info!(hash = %&hash[..8.min(hash.len())], "GUI: project directory removed");
    }

    // 5. Remove wake signals (separate directory from project)
    for agent_id in &agents {
        let wake_path = path_utils::wake_signal_path(agent_id);
        if wake_path.exists() {
            let _ = std::fs::remove_file(&wake_path);
        }
    }

    tracing::info!(hash = %&hash[..8.min(hash.len())], agents = agents.len(), "GUI: project fully removed");
    Ok(serde_json::json!({ "removed": true }))
}

// ─── Threads ─────────────────────────────────────────────────

#[tauri::command]
pub fn get_threads(
    project_hash: String,
    agent_id: String,
    status_filter: Option<String>,
) -> Result<serde_json::Value, String> {
    tracing::info!(project = %&project_hash[..8.min(project_hash.len())], agent = %agent_id, "GUI: get_threads");
    let agent_db = path_utils::agent_db_path(&project_hash, &agent_id);
    let conn = open_connection(&agent_db, ConnectionRole::Cli)
        .map_err(|e| e.to_string())?;

    let threads = match status_filter.as_deref() {
        Some("all") => ThreadStorage::list_all(&conn),
        Some("suspended") => ThreadStorage::list_by_status(&conn, &ThreadStatus::Suspended),
        Some("archived") => ThreadStorage::list_by_status(&conn, &ThreadStatus::Archived),
        _ => ThreadStorage::list_by_status(&conn, &ThreadStatus::Active),
    }.map_err(|e| e.to_string())?;

    let result: Vec<serde_json::Value> = threads.iter().map(|t| {
        let injection_stats = t.injection_stats.as_ref().map(|s| {
            serde_json::json!({
                "injection_count": s.injection_count,
                "used_count": s.used_count,
                "last_injected_at": s.last_injected_at,
            })
        });

        serde_json::json!({
            "id": t.id,
            "title": t.title,
            "status": format!("{:?}", t.status),
            "weight": t.weight,
            "importance": t.importance,
            "topics": t.topics,
            "labels": t.labels,
            "concepts": t.concepts,
            "summary": t.summary,
            "origin_type": format!("{:?}", t.origin_type),
            "injection_stats": injection_stats,
            "message_count": t.activation_count,
            "created_at": t.created_at.to_rfc3339(),
            "last_active": t.last_active.to_rfc3339(),
        })
    }).collect();

    Ok(serde_json::json!(result))
}

#[tauri::command]
pub fn search_threads(
    project_hash: String,
    agent_id: String,
    query: String,
) -> Result<serde_json::Value, String> {
    tracing::info!(project = %&project_hash[..8.min(project_hash.len())], agent = %agent_id, query = %query, "GUI: search_threads");
    let agent_db = path_utils::agent_db_path(&project_hash, &agent_id);
    let conn = open_connection(&agent_db, ConnectionRole::Cli)
        .map_err(|e| e.to_string())?;

    let threads = ThreadStorage::search(&conn, &query)
        .map_err(|e| e.to_string())?;

    let result: Vec<serde_json::Value> = threads.iter().map(|t| {
        serde_json::json!({
            "id": t.id,
            "title": t.title,
            "status": format!("{:?}", t.status),
            "weight": t.weight,
            "importance": t.importance,
            "topics": t.topics,
        })
    }).collect();

    Ok(serde_json::json!(result))
}

#[tauri::command]
pub fn search_threads_by_label(
    project_hash: String,
    agent_id: String,
    labels: Vec<String>,
) -> Result<serde_json::Value, String> {
    tracing::info!(project = %&project_hash[..8.min(project_hash.len())], agent = %agent_id, labels = ?labels, "GUI: search_threads_by_label");
    let agent_db = path_utils::agent_db_path(&project_hash, &agent_id);
    let conn = open_connection(&agent_db, ConnectionRole::Cli)
        .map_err(|e| e.to_string())?;

    let threads = ThreadStorage::search_by_labels(&conn, &labels)
        .map_err(|e| e.to_string())?;

    let result: Vec<serde_json::Value> = threads.iter().map(|t| {
        serde_json::json!({
            "id": t.id,
            "title": t.title,
            "status": format!("{:?}", t.status),
            "weight": t.weight,
            "importance": t.importance,
            "topics": t.topics,
            "labels": t.labels,
            "message_count": ThreadStorage::message_count(&conn, &t.id).unwrap_or(0),
        })
    }).collect();

    Ok(serde_json::json!(result))
}

#[tauri::command]
pub fn search_threads_by_topic(
    project_hash: String,
    agent_id: String,
    topics: Vec<String>,
) -> Result<serde_json::Value, String> {
    tracing::info!(project = %&project_hash[..8.min(project_hash.len())], agent = %agent_id, topics = ?topics, "GUI: search_threads_by_topic");
    let agent_db = path_utils::agent_db_path(&project_hash, &agent_id);
    let conn = open_connection(&agent_db, ConnectionRole::Cli)
        .map_err(|e| e.to_string())?;

    let threads = ThreadStorage::search_by_topics(&conn, &topics)
        .map_err(|e| e.to_string())?;

    let result: Vec<serde_json::Value> = threads.iter().map(|t| {
        serde_json::json!({
            "id": t.id,
            "title": t.title,
            "status": format!("{:?}", t.status),
            "weight": t.weight,
            "importance": t.importance,
            "topics": t.topics,
            "labels": t.labels,
            "message_count": ThreadStorage::message_count(&conn, &t.id).unwrap_or(0),
        })
    }).collect();

    Ok(serde_json::json!(result))
}

#[tauri::command]
pub fn list_all_labels(
    project_hash: String,
    agent_id: String,
) -> Result<serde_json::Value, String> {
    let agent_db = path_utils::agent_db_path(&project_hash, &agent_id);
    let conn = open_connection(&agent_db, ConnectionRole::Cli)
        .map_err(|e| e.to_string())?;

    let labels = ThreadStorage::list_all_labels(&conn)
        .map_err(|e| e.to_string())?;

    Ok(serde_json::json!(labels))
}

#[tauri::command]
pub fn list_all_topics(
    project_hash: String,
    agent_id: String,
) -> Result<serde_json::Value, String> {
    let agent_db = path_utils::agent_db_path(&project_hash, &agent_id);
    let conn = open_connection(&agent_db, ConnectionRole::Cli)
        .map_err(|e| e.to_string())?;

    let topics = ThreadStorage::list_all_topics(&conn)
        .map_err(|e| e.to_string())?;

    Ok(serde_json::json!(topics))
}

#[tauri::command]
pub fn get_bridges(
    project_hash: String,
    agent_id: String,
) -> Result<serde_json::Value, String> {
    tracing::info!(project = %&project_hash[..8.min(project_hash.len())], agent = %agent_id, "GUI: get_bridges");
    let agent_db = path_utils::agent_db_path(&project_hash, &agent_id);
    let conn = open_connection(&agent_db, ConnectionRole::Cli)
        .map_err(|e| e.to_string())?;
    migrations::migrate_agent_db(&conn).map_err(|e| e.to_string())?;

    let bridges = BridgeStorage::list_all(&conn).map_err(|e| e.to_string())?;
    let result: Vec<serde_json::Value> = bridges.iter().map(|b| {
        serde_json::json!({
            "id": b.id,
            "source_id": b.source_id,
            "target_id": b.target_id,
            "relation_type": format!("{:?}", b.relation_type),
            "weight": b.weight,
            "confidence": b.confidence,
            "status": format!("{:?}", b.status),
            "shared_concepts": b.shared_concepts,
            "use_count": b.use_count,
            "reason": b.reason,
        })
    }).collect();

    Ok(serde_json::json!(result))
}

// ─── Thread Detail + Delete + Purge ─────────────────────────

#[tauri::command]
pub fn get_thread_detail(
    project_hash: String,
    agent_id: String,
    thread_id: String,
) -> Result<serde_json::Value, String> {
    tracing::info!(thread = %thread_id, agent = %agent_id, "GUI: get_thread_detail");
    let agent_db = path_utils::agent_db_path(&project_hash, &agent_id);
    let conn = open_connection(&agent_db, ConnectionRole::Cli)
        .map_err(|e| e.to_string())?;

    let thread = ThreadStorage::get(&conn, &thread_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Thread '{}' not found", thread_id))?;

    let messages = ThreadStorage::get_messages(&conn, &thread_id)
        .map_err(|e| e.to_string())?;

    let bridges = BridgeStorage::list_for_thread(&conn, &thread_id)
        .map_err(|e| e.to_string())?;

    let msgs_json: Vec<serde_json::Value> = messages.iter().map(|m| {
        serde_json::json!({
            "msg_id": m.msg_id,
            "content": m.content,
            "source_type": m.source_type,
            "timestamp": m.timestamp.to_rfc3339(),
        })
    }).collect();

    let bridges_json: Vec<serde_json::Value> = bridges.iter().map(|b| {
        serde_json::json!({
            "id": b.id,
            "source_id": b.source_id,
            "target_id": b.target_id,
            "relation_type": format!("{:?}", b.relation_type),
            "weight": b.weight,
            "reason": b.reason,
        })
    }).collect();

    Ok(serde_json::json!({
        "thread": {
            "id": thread.id,
            "title": thread.title,
            "status": format!("{:?}", thread.status),
            "summary": thread.summary,
            "topics": thread.topics,
            "labels": thread.labels,
            "importance": thread.importance,
            "weight": thread.weight,
            "activation_count": thread.activation_count,
            "created_at": thread.created_at.to_rfc3339(),
            "last_active": thread.last_active.to_rfc3339(),
            "parent_id": thread.parent_id,
            "child_ids": thread.child_ids,
        },
        "messages": msgs_json,
        "bridges": bridges_json,
    }))
}

#[tauri::command]
pub fn delete_thread(
    project_hash: String,
    agent_id: String,
    thread_id: String,
) -> Result<serde_json::Value, String> {
    tracing::info!(thread = %thread_id, agent = %agent_id, "GUI: delete_thread");
    let agent_db = path_utils::agent_db_path(&project_hash, &agent_id);
    let conn = open_connection(&agent_db, ConnectionRole::Cli)
        .map_err(|e| e.to_string())?;

    // 1. Delete bridges referencing this thread
    let bridges_deleted = BridgeStorage::delete_for_thread(&conn, &thread_id)
        .map_err(|e| e.to_string())?;

    // 2. Delete messages
    ThreadStorage::delete_messages(&conn, &thread_id)
        .map_err(|e| e.to_string())?;

    // 3. Delete thread
    ThreadStorage::delete(&conn, &thread_id)
        .map_err(|e| e.to_string())?;

    tracing::info!(thread = %thread_id, bridges_deleted, "GUI: thread deleted");
    Ok(serde_json::json!({
        "deleted": true,
        "thread_id": thread_id,
        "bridges_deleted": bridges_deleted,
    }))
}

#[tauri::command]
pub fn purge_agent_db(
    project_hash: String,
    agent_id: String,
) -> Result<serde_json::Value, String> {
    tracing::info!(agent = %agent_id, "GUI: purge_agent_db");
    let agent_db = path_utils::agent_db_path(&project_hash, &agent_id);
    let conn = open_connection(&agent_db, ConnectionRole::Cli)
        .map_err(|e| e.to_string())?;

    conn.execute_batch(
        "DELETE FROM bridges;
         DELETE FROM thread_messages;
         DELETE FROM threads;
         DELETE FROM cognitive_inbox;
         DELETE FROM dead_letters;
         VACUUM;"
    ).map_err(|e| e.to_string())?;

    tracing::info!(agent = %agent_id, "GUI: agent DB purged");
    Ok(serde_json::json!({ "purged": true, "agent_id": agent_id }))
}

// ─── Settings (read + write) ─────────────────────────────────

#[tauri::command]
pub fn get_settings(_project_hash: String) -> Result<serde_json::Value, String> {
    tracing::info!("GUI: get_settings");
    let config = GuardianConfig::default();
    let config_path = path_utils::data_dir().join("config.json");

    // Merge file config over defaults
    let mut val = serde_json::to_value(&config).map_err(|e| e.to_string())?;
    if config_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&config_path) {
            if let Ok(file_val) = serde_json::from_str::<serde_json::Value>(&content) {
                merge_json(&mut val, &file_val);
            }
        }
    }
    Ok(val)
}

#[tauri::command]
pub fn save_settings(settings: serde_json::Value) -> Result<serde_json::Value, String> {
    tracing::info!("GUI: save_settings");
    let config_path = path_utils::data_dir().join("config.json");
    // Validate it can deserialize
    let config: GuardianConfig = serde_json::from_value(settings.clone())
        .map_err(|e| format!("Invalid config: {}", e))?;

    let content = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(&config_path, content).map_err(|e| e.to_string())?;

    // Sync hook/capture settings to per-project guardian_config.json
    ai_smartness::config_sync::sync_guardian_configs(&settings);

    // Sync MCP permissions to all project .claude/settings.json files
    sync_mcp_permissions(config.hooks.mcp_auto_allow);

    Ok(serde_json::json!({ "saved": true }))
}

/// Sync MCP auto-allow permissions to all registered projects' .claude/settings.json.
fn sync_mcp_permissions(enabled: bool) {
    let wildcards = ["mcp__ai-smartness__*"];

    let reg_path = path_utils::registry_db_path();
    let reg_conn = match open_connection(&reg_path, ConnectionRole::Cli) {
        Ok(c) => c,
        Err(_) => return,
    };
    let _ = migrations::migrate_registry_db(&reg_conn);

    let registry = SqliteProjectRegistry::new(reg_conn);
    let projects = match registry.list_projects() {
        Ok(p) => p,
        Err(_) => return,
    };

    for project in &projects {
        let settings_path = std::path::Path::new(&project.path)
            .join(".claude")
            .join("settings.json");
        if !settings_path.exists() {
            continue;
        }

        let content = match std::fs::read_to_string(&settings_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let mut settings: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let tools = settings
            .as_object_mut()
            .and_then(|o| o.get_mut("permissions"))
            .and_then(|p| p.as_object_mut())
            .and_then(|p| p.get_mut("allowedTools"))
            .and_then(|a| a.as_array_mut());

        if enabled {
            // Add wildcards if missing
            let tools = settings
                .pointer_mut("/permissions/allowedTools")
                .and_then(|a| a.as_array_mut());
            if let Some(arr) = tools {
                for wc in &wildcards {
                    if !arr.iter().any(|t| t.as_str() == Some(wc)) {
                        arr.push(serde_json::json!(wc));
                    }
                }
            } else {
                // Create permissions.allowedTools with wildcards
                let perms = settings
                    .as_object_mut()
                    .unwrap()
                    .entry("permissions")
                    .or_insert_with(|| serde_json::json!({}));
                if !perms.is_object() { *perms = serde_json::json!({}); }
                let arr: Vec<serde_json::Value> = wildcards.iter().map(|w| serde_json::json!(w)).collect();
                perms.as_object_mut().unwrap().insert("allowedTools".to_string(), serde_json::json!(arr));
            }
        } else {
            // Remove wildcards
            if let Some(arr) = settings
                .pointer_mut("/permissions/allowedTools")
                .and_then(|a| a.as_array_mut())
            {
                arr.retain(|t| {
                    t.as_str().map(|s| !wildcards.contains(&s)).unwrap_or(true)
                });
            }
        }

        if let Ok(out) = serde_json::to_string_pretty(&settings) {
            let _ = std::fs::write(&settings_path, out);
        }

        // Also sync settings.local.json (permissions.allow)
        let local_path = std::path::Path::new(&project.path)
            .join(".claude")
            .join("settings.local.json");
        sync_local_permissions(&local_path, enabled, &wildcards);
    }
}

/// Sync MCP wildcards into a project's `settings.local.json` (`permissions.allow`).
fn sync_local_permissions(local_path: &std::path::Path, enabled: bool, wildcards: &[&str]) {
    let mut local: serde_json::Value = if local_path.exists() {
        match std::fs::read_to_string(local_path).ok().and_then(|c| serde_json::from_str(&c).ok()) {
            Some(v) => v,
            None => return,
        }
    } else if enabled {
        serde_json::json!({})
    } else {
        return;
    };

    if !local.is_object() {
        local = serde_json::json!({});
    }

    if enabled {
        let permissions = local
            .as_object_mut()
            .unwrap()
            .entry("permissions")
            .or_insert_with(|| serde_json::json!({}));
        if !permissions.is_object() { *permissions = serde_json::json!({}); }
        let allow = permissions
            .as_object_mut()
            .unwrap()
            .entry("allow")
            .or_insert_with(|| serde_json::json!([]));
        if !allow.is_array() { *allow = serde_json::json!([]); }
        let arr = allow.as_array_mut().unwrap();
        for wc in wildcards {
            if !arr.iter().any(|v| v.as_str() == Some(wc)) {
                arr.push(serde_json::json!(wc));
            }
        }
    } else if let Some(arr) = local
        .pointer_mut("/permissions/allow")
        .and_then(|a| a.as_array_mut())
    {
        arr.retain(|v| v.as_str().map(|s| !wildcards.contains(&s)).unwrap_or(true));
    }

    if let Ok(out) = serde_json::to_string_pretty(&local) {
        let _ = std::fs::write(local_path, out);
    }
}

// ─── Agents ─────────────────────────────────────────────────

#[tauri::command]
pub fn list_agents(project_hash: String) -> Result<serde_json::Value, String> {
    tracing::info!(project = %&project_hash[..8.min(project_hash.len())], "GUI: list_agents");
    let reg_path = path_utils::registry_db_path();
    let reg_conn = open_connection(&reg_path, ConnectionRole::Cli)
        .map_err(|e| e.to_string())?;
    migrations::migrate_registry_db(&reg_conn).map_err(|e| e.to_string())?;

    let agents = AgentRegistry::list(&reg_conn, Some(&project_hash), None, None)
        .map_err(|e| e.to_string())?;

    let result: Vec<serde_json::Value> = agents.iter().map(|a| {
        serde_json::json!({
            "id": a.id,
            "name": a.name,
            "role": a.role,
            "status": a.status.as_str(),
            "supervisor_id": a.supervisor_id,
            "coordination_mode": a.coordination_mode.as_str(),
            "team": a.team,
            "capabilities": a.capabilities,
            "specializations": a.specializations,
            "thread_mode": a.thread_mode.as_str(),
            "thread_quota": a.thread_mode.quota(),
            "last_seen": a.last_seen.to_rfc3339(),
            "registered_at": a.registered_at.to_rfc3339(),
            "report_to": a.report_to,
            "custom_role": a.custom_role,
        })
    }).collect();

    Ok(serde_json::json!(result))
}

#[tauri::command]
pub fn add_agent(
    project_hash: String,
    agent_id: String,
    name: String,
    role: String,
    supervisor_id: Option<String>,
    team: Option<String>,
    is_supervisor: Option<bool>,
    thread_mode: Option<String>,
    report_to: Option<String>,
    custom_role: Option<String>,
) -> Result<serde_json::Value, String> {
    tracing::info!(
        agent = %agent_id, name = %name, role = %role,
        supervisor = ?supervisor_id, team = ?team, is_supervisor = ?is_supervisor,
        project = %&project_hash[..8.min(project_hash.len())],
        "GUI: add_agent called"
    );
    let reg_path = path_utils::registry_db_path();
    let reg_conn = open_connection(&reg_path, ConnectionRole::Cli)
        .map_err(|e| e.to_string())?;
    migrations::migrate_registry_db(&reg_conn).map_err(|e| e.to_string())?;

    // If the only agent is "main" (from init), rename it in-place to the new agent_id
    // (organic replacement: the first named agent inherits main's memory)
    let existing = AgentRegistry::list(&reg_conn, Some(&project_hash), None, None)
        .unwrap_or_default();
    if existing.len() == 1 && existing[0].id == "main" && agent_id != "main" {
        tracing::info!(
            new_id = %agent_id,
            "GUI: replacing main agent in-place (organic rename)"
        );
        AgentRegistry::rename(&reg_conn, "main", &agent_id, &project_hash)
            .map_err(|e| e.to_string())?;

        // Determine coordination mode
        let mode = if is_supervisor.unwrap_or(false) {
            CoordinationMode::Coordinator
        } else if supervisor_id.is_some() {
            CoordinationMode::Supervised
        } else {
            match role.as_str() {
                "coordinator" | "architect" => CoordinationMode::Coordinator,
                _ => CoordinationMode::Autonomous,
            }
        };

        // Update the renamed agent with the new metadata
        let update = AgentUpdate {
            name: Some(name),
            role: Some(role),
            description: None,
            supervisor_id: supervisor_id.map(Some),
            coordination_mode: Some(mode.as_str().to_string()),
            team: team.map(Some),
            specializations: None,
            capabilities: None,
            thread_mode: thread_mode.clone(),
            report_to: report_to.clone(),
            custom_role: custom_role.clone(),
            workspace_path: None,
        };
        AgentRegistry::update(&reg_conn, &agent_id, &project_hash, &update)
            .map_err(|e| e.to_string())?;

        tracing::info!(agent = %agent_id, "GUI: default replaced successfully");
        return Ok(serde_json::json!({ "registered": true, "id": agent_id, "replaced_default": true }));
    }

    // Validate supervisor exists
    if let Some(ref sup_id) = supervisor_id {
        let sup = AgentRegistry::get(&reg_conn, sup_id, &project_hash)
            .map_err(|e| e.to_string())?;
        if sup.is_none() {
            return Err(format!("Supervisor '{}' not found", sup_id));
        }
        AgentRegistry::validate_hierarchy(&reg_conn, &agent_id, sup_id, &project_hash)
            .map_err(|e| e.to_string())?;
    }

    // Determine coordination mode
    let mode = if is_supervisor.unwrap_or(false) {
        CoordinationMode::Coordinator
    } else if supervisor_id.is_some() {
        CoordinationMode::Supervised
    } else {
        match role.as_str() {
            "coordinator" | "architect" => CoordinationMode::Coordinator,
            _ => CoordinationMode::Autonomous,
        }
    };

    let agent = Agent {
        id: agent_id.clone(),
        project_hash: project_hash.clone(),
        name,
        description: String::new(),
        role,
        capabilities: vec![],
        status: AgentStatus::Active,
        last_seen: chrono::Utc::now(),
        registered_at: chrono::Utc::now(),
        supervisor_id,
        coordination_mode: mode,
        team,
        specializations: vec![],
        thread_mode: thread_mode
            .as_deref()
            .and_then(|s| s.parse().ok())
            .unwrap_or(ai_smartness::agent::ThreadMode::Normal),
        current_activity: String::new(),
        report_to,
        custom_role,
        workspace_path: String::new(),
    };

    AgentRegistry::register(&reg_conn, &agent)
        .map_err(|e| e.to_string())?;
    tracing::info!(agent = %agent_id, thread_mode = %agent.thread_mode, "GUI: agent registered in registry");

    // Initialize agent DB
    let agent_db = path_utils::agent_db_path(&project_hash, &agent_id);
    tracing::info!(path = %agent_db.display(), "GUI: creating agent DB");
    if let Ok(conn) = open_connection(&agent_db, ConnectionRole::Cli) {
        let _ = migrations::migrate_agent_db(&conn);
        tracing::info!("GUI: agent DB created and migrated");
    } else {
        tracing::warn!(path = %agent_db.display(), "GUI: failed to create agent DB");
    }

    // Ensure Claude Code hooks are installed for this project
    use ai_smartness::project_registry::ProjectRegistryTrait;
    let reg_conn2 = open_connection(&reg_path, ConnectionRole::Cli)
        .map_err(|e| e.to_string())?;
    let registry = SqliteProjectRegistry::new(reg_conn2);
    if let Ok(Some(project)) = registry.get_project(&project_hash) {
        let project_path = std::path::Path::new(&project.path);
        let hooks_exist = project_path.join(".claude/settings.json").exists();
        tracing::info!(path = %project_path.display(), hooks_exist, "GUI: checking hooks");
        if !hooks_exist {
            match ai_smartness::hook_setup::install_claude_hooks(project_path, &project_hash) {
                Ok(()) => tracing::info!("GUI: hooks installed successfully"),
                Err(e) => tracing::warn!(error = %e, "GUI: failed to install hooks"),
            }
        }
    } else {
        tracing::warn!(project = %&project_hash[..8.min(project_hash.len())], "GUI: project not found in registry");
    }

    tracing::info!(agent = %agent_id, "GUI: add_agent completed successfully");
    Ok(serde_json::json!({ "registered": true, "id": agent_id }))
}

#[tauri::command]
pub fn update_agent(
    project_hash: String,
    agent_id: String,
    name: Option<String>,
    role: Option<String>,
    description: Option<String>,
    supervisor_id: Option<String>,
    team: Option<String>,
    is_supervisor: Option<bool>,
    capabilities: Option<Vec<String>>,
    specializations: Option<Vec<String>>,
    thread_mode: Option<String>,
    report_to: Option<String>,
    custom_role: Option<String>,
) -> Result<serde_json::Value, String> {
    tracing::info!(
        agent = %agent_id, name = ?name, role = ?role, is_supervisor = ?is_supervisor,
        "GUI: update_agent called"
    );
    use ai_smartness::registry::registry::AgentUpdate;

    let reg_path = path_utils::registry_db_path();
    let reg_conn = open_connection(&reg_path, ConnectionRole::Cli)
        .map_err(|e| e.to_string())?;
    migrations::migrate_registry_db(&reg_conn).map_err(|e| e.to_string())?;

    // Determine coordination_mode from is_supervisor + supervisor_id
    let coordination_mode = match is_supervisor {
        Some(true) => Some("coordinator".to_string()),
        Some(false) if supervisor_id.is_some() => Some("supervised".to_string()),
        Some(false) => Some("autonomous".to_string()),
        None => None,
    };

    // Wrap supervisor_id: Some(value) means "set to value", None means "don't change"
    // To clear supervisor, frontend sends empty string which we convert to None
    let supervisor_update = supervisor_id.map(|s| {
        if s.is_empty() { None } else { Some(s) }
    });

    // Same for team
    let team_update = team.map(|t| {
        if t.is_empty() { None } else { Some(t) }
    });

    let update = AgentUpdate {
        name,
        role,
        description,
        supervisor_id: supervisor_update,
        coordination_mode,
        team: team_update,
        specializations,
        capabilities,
        thread_mode: thread_mode.clone(),
        report_to,
        custom_role,
        workspace_path: None,
    };

    AgentRegistry::update(&reg_conn, &agent_id, &project_hash, &update)
        .map_err(|e| e.to_string())?;

    // If thread_mode changed, notify daemon to update quota + enforce
    let mut threads_suspended = 0u64;
    if let Some(ref mode_str) = thread_mode {
        tracing::info!(agent = %agent_id, thread_mode = %mode_str, "GUI: notifying daemon of thread_mode change");
        match daemon_ipc_client::send_method("set_thread_mode", serde_json::json!({
            "project_hash": project_hash,
            "agent_id": agent_id,
            "thread_mode": mode_str,
        })) {
            Ok(resp) => {
                threads_suspended = resp.get("threads_suspended")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                tracing::info!(agent = %agent_id, threads_suspended, "GUI: daemon updated thread_mode");
            }
            Err(e) => {
                tracing::warn!(agent = %agent_id, error = %e, "GUI: daemon not available for thread_mode update (will apply on next capture)");
            }
        }
    }

    tracing::info!(agent = %agent_id, "GUI: update_agent completed");
    Ok(serde_json::json!({ "updated": true, "id": agent_id, "threads_suspended": threads_suspended }))
}

#[tauri::command]
pub fn remove_agent(project_hash: String, agent_id: String) -> Result<serde_json::Value, String> {
    tracing::info!(agent = %agent_id, "GUI: remove_agent called");
    let reg_path = path_utils::registry_db_path();
    let reg_conn = open_connection(&reg_path, ConnectionRole::Cli)
        .map_err(|e| e.to_string())?;

    AgentRegistry::delete(&reg_conn, &agent_id, &project_hash)
        .map_err(|e| e.to_string())?;

    tracing::info!(agent = %agent_id, "GUI: agent removed");
    Ok(serde_json::json!({ "removed": true }))
}

#[tauri::command]
pub fn get_hierarchy(project_hash: String) -> Result<serde_json::Value, String> {
    tracing::info!(project = %&project_hash[..8.min(project_hash.len())], "GUI: get_hierarchy");
    let reg_path = path_utils::registry_db_path();
    let reg_conn = open_connection(&reg_path, ConnectionRole::Cli)
        .map_err(|e| e.to_string())?;
    migrations::migrate_registry_db(&reg_conn).map_err(|e| e.to_string())?;

    let tree = AgentRegistry::build_hierarchy_tree(&reg_conn, &project_hash)
        .map_err(|e| e.to_string())?;

    fn node_to_json(node: &HierarchyNode) -> serde_json::Value {
        serde_json::json!({
            "id": node.id,
            "name": node.name,
            "role": node.role,
            "mode": node.mode,
            "team": node.team,
            "active_tasks": node.active_tasks,
            "subordinates": node.subordinates.iter().map(node_to_json).collect::<Vec<_>>(),
        })
    }

    let result: Vec<serde_json::Value> = tree.iter().map(node_to_json).collect();
    Ok(serde_json::json!(result))
}

// ─── Debug Window ────────────────────────────────────────────

#[tauri::command]
pub fn open_debug_window(app: tauri::AppHandle, project_hash: String) -> Result<(), String> {
    tracing::info!(project = %project_hash, "GUI: open_debug_window");
    use tauri::Manager;
    // Focus existing window if already open
    if let Some(win) = app.get_webview_window("debug") {
        win.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }

    let url = format!("debug.html?project={}", project_hash);
    tauri::WebviewWindowBuilder::new(
        &app,
        "debug",
        tauri::WebviewUrl::App(url.into()),
    )
    .title("AI Smartness \u{2014} Debug Console")
    .inner_size(960.0, 600.0)
    .resizable(true)
    .center()
    .build()
    .map_err(|e| e.to_string())?;

    Ok(())
}

// ─── Debug Logs ──────────────────────────────────────────────

#[tauri::command]
pub fn get_debug_logs(
    project_hash: String,
    offset: usize,
) -> Result<serde_json::Value, String> {
    tracing::debug!(project = %&project_hash[..8.min(project_hash.len())], offset, "GUI: get_debug_logs");
    let log_path = path_utils::project_dir(&project_hash).join("daemon.log");

    if !log_path.exists() {
        return Ok(serde_json::json!({ "lines": [], "offset": 0, "file": log_path.to_string_lossy() }));
    }

    let content = std::fs::read_to_string(&log_path).map_err(|e| e.to_string())?;
    let all_lines: Vec<&str> = content.lines().collect();
    let total = all_lines.len();

    // Return only new lines since offset
    let new_lines: Vec<&str> = if offset < total {
        all_lines[offset..].to_vec()
    } else {
        vec![]
    };

    Ok(serde_json::json!({
        "lines": new_lines,
        "offset": total,
        "total": total,
        "file": log_path.to_string_lossy(),
    }))
}

// ─── Daemon Settings ─────────────────────────────────────────

#[tauri::command]
pub fn get_daemon_settings() -> Result<serde_json::Value, String> {
    tracing::info!("GUI: get_daemon_settings");
    let config = ai_smartness::config::DaemonConfig::load();
    serde_json::to_value(&config).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_daemon_settings(settings: serde_json::Value) -> Result<serde_json::Value, String> {
    tracing::info!("GUI: save_daemon_settings");
    let config: ai_smartness::config::DaemonConfig = serde_json::from_value(settings)
        .map_err(|e| format!("Invalid daemon config: {}", e))?;
    config.save()?;
    Ok(serde_json::json!({ "saved": true }))
}

// ─── Global Debug Logs ───────────────────────────────────────

#[tauri::command]
pub fn get_global_debug_logs(offset: usize) -> Result<serde_json::Value, String> {
    tracing::debug!(offset, "GUI: get_global_debug_logs");
    let log_path = path_utils::data_dir().join("daemon.log");

    if !log_path.exists() {
        return Ok(serde_json::json!({ "lines": [], "offset": 0, "file": log_path.to_string_lossy() }));
    }

    let content = std::fs::read_to_string(&log_path).map_err(|e| e.to_string())?;
    let all_lines: Vec<&str> = content.lines().collect();
    let total = all_lines.len();

    let new_lines: Vec<&str> = if offset < total {
        all_lines[offset..].to_vec()
    } else {
        vec![]
    };

    Ok(serde_json::json!({
        "lines": new_lines,
        "offset": total,
        "total": total,
        "file": log_path.to_string_lossy(),
    }))
}

// ─── Resource Monitoring ─────────────────────────────────────

#[tauri::command]
pub fn get_system_resources() -> Result<serde_json::Value, String> {
    use sysinfo::System;

    let mut sys = System::new();
    sys.refresh_cpu_all();
    sys.refresh_memory();

    let cpu_percent = sys.global_cpu_usage();
    let mem_used = sys.used_memory() / (1024 * 1024); // MB
    let mem_total = sys.total_memory() / (1024 * 1024); // MB
    let mem_percent = if mem_total > 0 {
        (mem_used as f64 / mem_total as f64) * 100.0
    } else {
        0.0
    };

    // Data dir disk usage
    let data_dir = path_utils::data_dir();
    let data_dir_mb = dir_size_mb(&data_dir);

    // Daemon pool status via IPC
    let daemon_info = match daemon_ipc_client::daemon_status() {
        Ok(resp) => resp,
        Err(_) => serde_json::json!({"mode": "offline"}),
    };

    Ok(serde_json::json!({
        "cpu_percent": cpu_percent,
        "memory": {
            "used_mb": mem_used,
            "total_mb": mem_total,
            "percent": mem_percent,
        },
        "disk": {
            "data_dir_mb": data_dir_mb,
        },
        "daemon": daemon_info,
    }))
}

fn dir_size_mb(path: &std::path::Path) -> f64 {
    let mut total: u64 = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let meta = entry.metadata();
            if let Ok(m) = meta {
                if m.is_file() {
                    total += m.len();
                } else if m.is_dir() {
                    total += (dir_size_mb(&entry.path()) * 1024.0 * 1024.0) as u64;
                }
            }
        }
    }
    total as f64 / (1024.0 * 1024.0)
}

// ─── User Profile ───────────────────────────────────────────

#[tauri::command]
pub fn get_user_profile(project_hash: String, agent_id: String) -> Result<serde_json::Value, String> {
    tracing::info!(agent = %agent_id, "GUI: get_user_profile");
    let data_dir = path_utils::agent_data_dir(&project_hash, &agent_id);
    let profile = ai_smartness::user_profile::UserProfile::load(&data_dir);
    serde_json::to_value(&profile).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_user_profile(
    project_hash: String,
    agent_id: String,
    profile: serde_json::Value,
) -> Result<serde_json::Value, String> {
    tracing::info!(agent = %agent_id, "GUI: save_user_profile");
    let data_dir = path_utils::agent_data_dir(&project_hash, &agent_id);
    let mut p: ai_smartness::user_profile::UserProfile =
        serde_json::from_value(profile).map_err(|e| e.to_string())?;
    p.save(&data_dir);
    Ok(serde_json::json!({ "saved": true }))
}

// ─── Backup ─────────────────────────────────────────────────

#[tauri::command]
pub fn get_backup_settings() -> Result<serde_json::Value, String> {
    tracing::info!("GUI: get_backup_settings");
    let config = ai_smartness::storage::backup::BackupConfig::load();
    serde_json::to_value(&config).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_backup_settings(settings: serde_json::Value) -> Result<serde_json::Value, String> {
    tracing::info!("GUI: save_backup_settings");
    let config: ai_smartness::storage::backup::BackupConfig =
        serde_json::from_value(settings).map_err(|e| e.to_string())?;
    config.save();
    Ok(serde_json::json!({ "saved": true }))
}

#[tauri::command]
pub fn trigger_backup(
    project_hash: String,
    agent_id: Option<String>,
) -> Result<serde_json::Value, String> {
    tracing::info!(project = %project_hash, agent = ?agent_id, "GUI: trigger_backup");
    let config = ai_smartness::storage::backup::BackupConfig::load();
    let backup_dir = std::path::PathBuf::from(
        expand_tilde(&config.backup_path),
    );

    let agents = if let Some(ref aid) = agent_id {
        vec![aid.clone()]
    } else {
        // Backup all agents for this project
        let reg_path = path_utils::registry_db_path();
        let reg_conn = open_connection(&reg_path, ConnectionRole::Cli)
            .map_err(|e| e.to_string())?;
        AgentRegistry::list(&reg_conn, Some(&project_hash), None, None)
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|a| a.id)
            .collect()
    };

    let mut backed_up = Vec::new();
    for aid in &agents {
        match ai_smartness::storage::backup::BackupManager::backup_agent(
            &project_hash, aid, &backup_dir,
        ) {
            Ok(path) => backed_up.push(serde_json::json!({
                "agent_id": aid,
                "path": path.to_string_lossy(),
            })),
            Err(e) => {
                tracing::warn!(agent = %aid, error = %e, "Backup failed for agent");
            }
        }
    }

    // Enforce retention
    ai_smartness::storage::backup::BackupManager::enforce_retention(
        &backup_dir,
        config.retention_count,
    );

    // Update last_backup_at
    let mut config = config;
    config.last_backup_at = Some(chrono::Utc::now().to_rfc3339());
    config.save();

    Ok(serde_json::json!({
        "backed_up": backed_up,
        "count": backed_up.len(),
    }))
}

#[tauri::command]
pub fn list_backups() -> Result<serde_json::Value, String> {
    tracing::info!("GUI: list_backups");
    let config = ai_smartness::storage::backup::BackupConfig::load();
    let backup_dir = std::path::PathBuf::from(
        expand_tilde(&config.backup_path),
    );
    let backups = ai_smartness::storage::backup::BackupManager::list_backups(&backup_dir);
    serde_json::to_value(&backups).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn restore_backup(
    backup_path: String,
    project_hash: String,
    agent_id: String,
) -> Result<serde_json::Value, String> {
    tracing::info!(backup = %backup_path, agent = %agent_id, "GUI: restore_backup");
    let source = std::path::Path::new(&backup_path);
    let dest = path_utils::agent_db_path(&project_hash, &agent_id);
    ai_smartness::storage::backup::BackupManager::restore_backup(source, &dest)
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "restored": true }))
}

#[tauri::command]
pub fn delete_backup(backup_path: String) -> Result<serde_json::Value, String> {
    tracing::info!(path = %backup_path, "GUI: delete_backup");
    std::fs::remove_file(&backup_path).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "deleted": true }))
}

// ─── Reindex ────────────────────────────────────────────────

#[tauri::command]
pub fn reindex_agent(
    project_hash: String,
    agent_id: String,
    reset_weights: bool,
) -> Result<serde_json::Value, String> {
    tracing::info!(agent = %agent_id, reset_weights, "GUI: reindex_agent");
    let db_path = path_utils::agent_db_path(&project_hash, &agent_id);
    let conn = open_connection(&db_path, ConnectionRole::Cli)
        .map_err(|e| e.to_string())?;
    migrations::migrate_agent_db(&conn).map_err(|e| e.to_string())?;

    let threads = ThreadStorage::list_all(&conn).map_err(|e| e.to_string())?;
    let total = threads.len();
    let embedder = ai_smartness::processing::embeddings::EmbeddingManager::global();
    let mut reindexed = 0;

    for mut thread in threads {
        // Build embedding text: title + summary + topics
        let mut text = thread.title.clone();
        if let Some(ref s) = thread.summary {
            text.push(' ');
            text.push_str(s);
        }
        text.push(' ');
        text.push_str(&thread.topics.join(" "));

        let emb = embedder.embed(&text);
        thread.embedding = Some(emb);
        if reset_weights {
            thread.weight = 1.0;
        }
        ThreadStorage::update(&conn, &thread).ok();
        reindexed += 1;
    }

    tracing::info!(reindexed, total, "Reindex complete");
    Ok(serde_json::json!({ "reindexed": reindexed, "total": total }))
}

// ─── Updates ────────────────────────────────────────────────

#[tauri::command]
pub fn check_update() -> Result<serde_json::Value, String> {
    tracing::info!("GUI: check_update");
    let current = env!("CARGO_PKG_VERSION");
    let os = std::env::consts::OS;

    // Try fetching latest release from GitHub API
    let latest = match std::process::Command::new("curl")
        .args(["-s", "-m", "10", "https://api.github.com/repos/nicmusic-music/ai-smartness/releases/latest"])
        .output()
    {
        Ok(output) if output.status.success() => {
            let body = String::from_utf8_lossy(&output.stdout);
            serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| v.get("tag_name").and_then(|t| t.as_str()).map(|s| s.trim_start_matches('v').to_string()))
        }
        _ => None,
    };

    let update_available = latest.as_ref().map(|l| l != current).unwrap_or(false);

    Ok(serde_json::json!({
        "current_version": current,
        "latest_version": latest.unwrap_or_else(|| "unknown".to_string()),
        "update_available": update_available,
        "os": os,
    }))
}

// ─── Helpers ─────────────────────────────────────────────────

/// Expand ~ to home directory in paths (delegates to shared impl).
fn expand_tilde(path: &str) -> String {
    path_utils::expand_tilde(path)
}

fn check_daemon() -> (bool, Option<u32>) {
    match daemon_ipc_client::daemon_status() {
        Ok(resp) => {
            let pid = resp.get("pid").and_then(|v| v.as_u64()).map(|p| p as u32);
            (true, pid)
        }
        Err(_) => (false, None),
    }
}

fn merge_json(base: &mut serde_json::Value, overlay: &serde_json::Value) {
    match (base, overlay) {
        (serde_json::Value::Object(base_map), serde_json::Value::Object(overlay_map)) => {
            for (k, v) in overlay_map {
                if let Some(base_v) = base_map.get_mut(k) {
                    merge_json(base_v, v);
                } else {
                    base_map.insert(k.clone(), v.clone());
                }
            }
        }
        (base, _) => {
            *base = overlay.clone();
        }
    }
}
