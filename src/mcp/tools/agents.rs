use ai_smartness::registry::discovery::Discovery;
use ai_smartness::registry::heartbeat::{Heartbeat, HeartbeatConfig};
use ai_smartness::registry::registry::AgentRegistry;
use ai_smartness::registry::tasks::AgentTaskStorage;
use ai_smartness::{id_gen, time_utils};
use ai_smartness::agent::{AgentTask, TaskPriority, TaskStatus};
use ai_smartness::constants::{truncate_safe, MAX_MESSAGE_SIZE_BYTES};
use ai_smartness::AiResult;

use super::{optional_str, required_str, ToolContext, ToolOutput};
use super::messaging::emit_wake_signal;

pub fn handle_agent_list(
    _params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let agents = AgentRegistry::list(ctx.registry_conn, Some(ctx.project_hash), None, None)?;

    let hb_config = HeartbeatConfig::default();
    let results: Vec<serde_json::Value> = agents
        .iter()
        .map(|a| {
            let alive = Heartbeat::is_alive(ctx.registry_conn, &a.id, ctx.project_hash, &hb_config)
                .unwrap_or(false);
            serde_json::json!({
                "id": a.id,
                "name": a.name,
                "role": a.role,
                "status": a.status.as_str(),
                "is_alive": alive,
                "team": a.team,
                "coordination_mode": a.coordination_mode.as_str(),
                "report_to": a.report_to,
                "custom_role": a.custom_role,
                "workspace_path": a.workspace_path,
                "full_permissions": a.full_permissions,
            })
        })
        .collect();

    Ok(serde_json::json!({"agents": results, "count": results.len()}))
}

pub fn handle_agent_query(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let capability = required_str(params, "capability")?;
    let agents = Discovery::find_by_capability(ctx.registry_conn, &capability)?;

    let results: Vec<serde_json::Value> = agents
        .iter()
        .map(|a| {
            serde_json::json!({
                "id": a.id,
                "name": a.name,
                "role": a.role,
                "capabilities": a.capabilities,
                "specializations": a.specializations,
            })
        })
        .collect();

    Ok(serde_json::json!({"agents": results, "count": results.len()}))
}

pub fn handle_agent_status(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let agent_id = required_str(params, "agent_id")?;

    let agent = AgentRegistry::get(ctx.registry_conn, &agent_id, ctx.project_hash)?
        .ok_or_else(|| ai_smartness::AiError::AgentNotFound(agent_id.clone()))?;

    let hb_config = HeartbeatConfig::default();
    let alive =
        Heartbeat::is_alive(ctx.registry_conn, &agent_id, ctx.project_hash, &hb_config).unwrap_or(false);
    let subordinates =
        AgentRegistry::list_subordinates(ctx.registry_conn, &agent_id, ctx.project_hash)?;
    let supervisor_chain =
        AgentRegistry::get_supervisor_chain(ctx.registry_conn, &agent_id, ctx.project_hash)?;
    let tasks =
        AgentTaskStorage::list_tasks_for_agent(ctx.registry_conn, &agent_id, ctx.project_hash)?;

    Ok(serde_json::json!({
        "agent": {
            "id": agent.id,
            "name": agent.name,
            "role": agent.role,
            "status": agent.status.as_str(),
            "coordination_mode": agent.coordination_mode.as_str(),
            "team": agent.team,
            "description": agent.description,
            "current_activity": agent.current_activity,
            "report_to": agent.report_to,
            "custom_role": agent.custom_role,
            "workspace_path": agent.workspace_path,
            "full_permissions": agent.full_permissions,
        },
        "is_alive": alive,
        "subordinates": subordinates.iter().map(|a| serde_json::json!({"id": a.id, "name": a.name})).collect::<Vec<_>>(),
        "supervisor_chain": supervisor_chain.iter().map(|a| serde_json::json!({"id": a.id, "name": a.name})).collect::<Vec<_>>(),
        "tasks": tasks.iter().take(10).map(|t| serde_json::json!({
            "id": t.id, "title": t.title, "status": t.status.as_str(), "priority": t.priority.as_str(),
        })).collect::<Vec<_>>(),
    }))
}

pub fn handle_agent_cleanup(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let remove_agent = optional_str(params, "remove_agent");

    if let Some(agent_id) = remove_agent {
        AgentRegistry::delete(ctx.registry_conn, &agent_id, ctx.project_hash)?;
        return Ok(serde_json::json!({"removed": agent_id}));
    }

    let hb_config = HeartbeatConfig::default();
    Heartbeat::mark_stale(ctx.registry_conn, &hb_config)?;
    Ok(serde_json::json!({"action": "cleanup", "status": "ok"}))
}

pub fn handle_agent_configure(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let agent_id = required_str(params, "agent_id")?;
    let project_hash = required_str(params, "project_hash")?;

    let update = ai_smartness::registry::registry::AgentUpdate {
        name: optional_str(params, "name"),
        role: optional_str(params, "role"),
        description: optional_str(params, "description"),
        supervisor_id: optional_str(params, "supervisor_id").map(Some),
        team: optional_str(params, "team").map(Some),
        coordination_mode: optional_str(params, "coordination_mode"),
        specializations: None,
        capabilities: None,
        thread_mode: optional_str(params, "thread_mode"),
        report_to: optional_str(params, "report_to"),
        custom_role: optional_str(params, "custom_role"),
        workspace_path: optional_str(params, "workspace_path"),
        full_permissions: super::optional_bool(params, "full_permissions"),
        expected_model: super::optional_str(params, "expected_model").map(|s| if s.is_empty() { None } else { Some(s) }),
    };

    AgentRegistry::update(ctx.registry_conn, &agent_id, &project_hash, &update)?;
    Ok(serde_json::json!({"configured": agent_id}))
}

pub fn handle_agent_tasks(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let action = required_str(params, "action")?;

    match action.as_str() {
        "list" => {
            let tasks =
                AgentTaskStorage::list_tasks(ctx.registry_conn, ctx.project_hash, None, None)?;
            let results: Vec<serde_json::Value> = tasks
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "id": t.id,
                        "title": t.title,
                        "assigned_to": t.assigned_to,
                        "assigned_by": t.assigned_by,
                        "status": t.status.as_str(),
                        "priority": t.priority.as_str(),
                    })
                })
                .collect();
            Ok(serde_json::json!({"tasks": results}))
        }
        "create" => {
            let title = required_str(params, "title")?;
            let assigned_to = required_str(params, "assigned_to")?;
            let priority_str =
                optional_str(params, "priority").unwrap_or_else(|| "normal".into());
            let now = time_utils::now();

            let task = AgentTask {
                id: id_gen::message_id(),
                project_hash: ctx.project_hash.to_string(),
                assigned_to,
                assigned_by: ctx.agent_id.to_string(),
                title,
                description: optional_str(params, "description").unwrap_or_default(),
                priority: priority_str.parse().unwrap_or(TaskPriority::Normal),
                status: TaskStatus::Pending,
                created_at: now,
                updated_at: now,
                deadline: None,
                dependencies: vec![],
                result: None,
            };
            AgentTaskStorage::create_task(ctx.registry_conn, &task)?;
            Ok(serde_json::json!({"created": task.id}))
        }
        "update_status" | "complete" => {
            let task_id = required_str(params, "task_id")?;
            let new_status = if action == "complete" {
                TaskStatus::Completed
            } else {
                let s =
                    optional_str(params, "status").unwrap_or_else(|| "in_progress".into());
                s.parse().unwrap_or(TaskStatus::InProgress)
            };
            let result = optional_str(params, "result");
            AgentTaskStorage::update_task_status(
                ctx.registry_conn,
                &task_id,
                new_status,
                result.as_deref(),
            )?;
            Ok(serde_json::json!({"updated": task_id}))
        }
        "delete" => {
            let task_id = required_str(params, "task_id")?;
            AgentTaskStorage::delete_task(ctx.registry_conn, &task_id)?;
            Ok(serde_json::json!({"deleted": task_id}))
        }
        _ => Err(ai_smartness::AiError::InvalidInput(format!(
            "Unknown action: {}",
            action
        ))),
    }
}

/// Select a different agent for this session.
/// Writes a per-session agent file (keyed by session_id) so the next hook invocation
/// uses the new agent. Also updates the global session file as fallback.
pub fn handle_agent_select(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<ToolOutput> {
    let target_agent_id = required_str(params, "agent_id")?;
    let session_id = optional_str(params, "session_id");

    // 1. Validate target agent exists in registry
    let agent = AgentRegistry::get(ctx.registry_conn, &target_agent_id, ctx.project_hash)?
        .ok_or_else(|| ai_smartness::AiError::AgentNotFound(target_agent_id.clone()))?;

    // 2. Write per-session file (keyed by session_id for multi-panel isolation)
    if let Some(ref sid) = session_id {
        let per_session_path =
            ai_smartness::storage::path_utils::per_session_agent_path(ctx.project_hash, sid);
        if let Some(parent) = per_session_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(&per_session_path, &target_agent_id).map_err(|e| {
            ai_smartness::AiError::Storage(format!("Failed to write per-session agent file: {}", e))
        })?;
        tracing::info!(
            from = ctx.agent_id,
            to = %target_agent_id,
            session_id = %sid,
            path = %per_session_path.display(),
            "Agent session switched (per-session)"
        );
    }

    // 3. Write global session file ONLY when no session_id is provided.
    //    When session_id is present, the per-session file provides isolation
    //    and writing the global would break other panels' agent resolution.
    if session_id.is_none() {
        let session_path =
            ai_smartness::storage::path_utils::agent_session_path(ctx.project_hash);
        if let Some(parent) = session_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(&session_path, &target_agent_id).map_err(|e| {
            ai_smartness::AiError::Storage(format!("Failed to write session file: {}", e))
        })?;
        tracing::info!(
            from = ctx.agent_id,
            to = %target_agent_id,
            "Agent session switched (global)"
        );
    }

    let result = serde_json::json!({
        "switched": true,
        "previous_agent": ctx.agent_id,
        "new_agent": {
            "id": agent.id,
            "name": agent.name,
            "role": agent.role,
        },
        "note": "Agent switched. All subsequent tool calls use the new agent identity."
    });

    Ok(ToolOutput::AgentSwitch {
        result,
        new_agent_id: target_agent_id,
    })
}

pub fn handle_task_delegate(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let to = required_str(params, "to")?;
    let task_desc = required_str(params, "task")?;
    let priority_str = optional_str(params, "priority").unwrap_or_else(|| "normal".into());
    let now = time_utils::now();

    let task = AgentTask {
        id: id_gen::message_id(),
        project_hash: ctx.project_hash.to_string(),
        assigned_to: to.clone(),
        assigned_by: ctx.agent_id.to_string(),
        title: task_desc,
        description: optional_str(params, "context")
            .map(|c| truncate_safe(&c, MAX_MESSAGE_SIZE_BYTES).to_string())
            .unwrap_or_default(),
        priority: priority_str.parse().unwrap_or(TaskPriority::Normal),
        status: TaskStatus::Pending,
        created_at: now,
        updated_at: now,
        deadline: None,
        dependencies: vec![],
        result: None,
    };

    AgentTaskStorage::create_task(ctx.registry_conn, &task)?;
    emit_wake_signal(&to, ctx.agent_id, &format!("Task delegated: {}", task.title), "inbox", false);
    Ok(serde_json::json!({"delegated": true, "task_id": task.id, "to": to}))
}

pub fn handle_task_status(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let task_id = required_str(params, "task_id")?;

    if let Some(task) = AgentTaskStorage::get_task(ctx.registry_conn, &task_id)? {
        Ok(serde_json::json!({
            "id": task.id,
            "title": task.title,
            "assigned_to": task.assigned_to,
            "status": task.status.as_str(),
            "priority": task.priority.as_str(),
            "result": task.result,
            "description": task.description,
        }))
    } else {
        Err(ai_smartness::AiError::InvalidInput(format!(
            "Task '{}' not found",
            task_id
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    const PH: &str = "test-ph-tasks";
    const AGENT: &str = "test-agent-del";
    const TARGET: &str = "test-target-del";

    fn setup_agent_db() -> Connection {
        let conn = Connection::open(":memory:").unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;").unwrap();
        ai_smartness::storage::migrations::migrate_agent_db(&conn).unwrap();
        conn
    }

    fn setup_registry_db() -> Connection {
        let conn = Connection::open(":memory:").unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;").unwrap();
        ai_smartness::storage::migrations::migrate_registry_db(&conn).unwrap();
        conn
    }

    fn setup_shared_db() -> Connection {
        let conn = Connection::open(":memory:").unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;").unwrap();
        ai_smartness::storage::migrations::migrate_shared_db(&conn).unwrap();
        conn
    }

    fn insert_project(conn: &Connection) {
        let now = ai_smartness::time_utils::to_sqlite(&ai_smartness::time_utils::now());
        conn.execute(
            "INSERT INTO projects (hash, path, name, created_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![PH, "/tmp/test", "test", now],
        ).unwrap();
    }

    fn register_agent(conn: &Connection, id: &str) {
        let now = chrono::Utc::now();
        let agent = ai_smartness::agent::Agent {
            id: id.to_string(),
            project_hash: PH.to_string(),
            name: id.to_string(),
            description: String::new(),
            role: "programmer".to_string(),
            capabilities: vec![],
            status: ai_smartness::agent::AgentStatus::Active,
            last_seen: now,
            registered_at: now,
            supervisor_id: None,
            coordination_mode: ai_smartness::agent::CoordinationMode::Autonomous,
            team: None,
            specializations: vec![],
            thread_mode: ai_smartness::agent::ThreadMode::Normal,
            current_activity: String::new(),
            report_to: None,
            custom_role: None,
            workspace_path: String::new(),
            full_permissions: false,
            expected_model: None,
        };
        AgentRegistry::register(conn, &agent).unwrap();
    }

    // T-C3.1: handle_task_delegate stores context
    #[test]
    fn test_task_delegate_stores_context() {
        let agent_conn = setup_agent_db();
        let registry_conn = setup_registry_db();
        let shared_conn = setup_shared_db();
        insert_project(&registry_conn);
        register_agent(&registry_conn, AGENT);
        register_agent(&registry_conn, TARGET);

        let ctx = ToolContext {
            agent_conn: &agent_conn,
            registry_conn: &registry_conn,
            shared_conn: &shared_conn,
            project_hash: PH,
            agent_id: AGENT,
        };

        let params = serde_json::json!({
            "to": TARGET,
            "task": "Implement quota guard",
            "context": "Check thread quota before creating threads. See inject.rs for reference.",
        });

        let result = handle_task_delegate(&params, &ctx).unwrap();
        let task_id = result["task_id"].as_str().unwrap();

        let task = ai_smartness::registry::tasks::AgentTaskStorage::get_task(&registry_conn, task_id)
            .unwrap()
            .unwrap();
        assert!(
            task.description.contains("quota"),
            "Task description should contain context"
        );
    }

    // T-C3.2: handle_task_status returns description
    #[test]
    fn test_task_status_returns_description() {
        let agent_conn = setup_agent_db();
        let registry_conn = setup_registry_db();
        let shared_conn = setup_shared_db();
        insert_project(&registry_conn);
        register_agent(&registry_conn, AGENT);
        register_agent(&registry_conn, TARGET);

        let ctx = ToolContext {
            agent_conn: &agent_conn,
            registry_conn: &registry_conn,
            shared_conn: &shared_conn,
            project_hash: PH,
            agent_id: AGENT,
        };

        let params = serde_json::json!({
            "to": TARGET,
            "task": "Test task",
            "context": "Detailed context for the task",
        });

        let result = handle_task_delegate(&params, &ctx).unwrap();
        let task_id = result["task_id"].as_str().unwrap();

        let status_params = serde_json::json!({"task_id": task_id});
        let status_result = handle_task_status(&status_params, &ctx).unwrap();
        assert!(status_result["description"].is_string());
        assert!(
            status_result["description"].as_str().unwrap().contains("context"),
            "Status should include description with context"
        );
    }

    // T-C3.3: task delegation emits wake signal
    #[test]
    fn test_task_delegate_emits_wake_signal() {
        let agent_conn = setup_agent_db();
        let registry_conn = setup_registry_db();
        let shared_conn = setup_shared_db();
        insert_project(&registry_conn);
        register_agent(&registry_conn, AGENT);

        let target = "test_wake_deleg_1";
        register_agent(&registry_conn, target);

        let ctx = ToolContext {
            agent_conn: &agent_conn,
            registry_conn: &registry_conn,
            shared_conn: &shared_conn,
            project_hash: PH,
            agent_id: AGENT,
        };

        let params = serde_json::json!({"to": target, "task": "Wake test task"});
        handle_task_delegate(&params, &ctx).unwrap();

        let signal_path = ai_smartness::storage::path_utils::wake_signal_path(target);
        assert!(signal_path.exists(), "Wake signal should be created");

        let content = std::fs::read_to_string(&signal_path).unwrap();
        let json: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(json["agent_id"], target);
        assert!(json["message"].as_str().unwrap().contains("Task delegated"));

        let _ = std::fs::remove_file(&signal_path);
    }
}
