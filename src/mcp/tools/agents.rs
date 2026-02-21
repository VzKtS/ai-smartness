use ai_smartness::registry::discovery::Discovery;
use ai_smartness::registry::heartbeat::{Heartbeat, HeartbeatConfig};
use ai_smartness::registry::registry::AgentRegistry;
use ai_smartness::registry::tasks::AgentTaskStorage;
use ai_smartness::{id_gen, time_utils};
use ai_smartness::agent::{AgentTask, TaskPriority, TaskStatus};
use ai_smartness::AiResult;

use super::{optional_str, required_str, ToolContext, ToolOutput};

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
        description: String::new(),
        priority: priority_str.parse().unwrap_or(TaskPriority::Normal),
        status: TaskStatus::Pending,
        created_at: now,
        updated_at: now,
        deadline: None,
        dependencies: vec![],
        result: None,
    };

    AgentTaskStorage::create_task(ctx.registry_conn, &task)?;
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
        }))
    } else {
        Err(ai_smartness::AiError::InvalidInput(format!(
            "Task '{}' not found",
            task_id
        )))
    }
}
