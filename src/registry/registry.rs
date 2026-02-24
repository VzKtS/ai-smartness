//! Agent Registry -- full CRUD with hierarchy validation.

use crate::time_utils;
use crate::agent::{Agent, AgentStatus, CoordinationMode, ThreadMode};
use crate::{AiError, AiResult};
use rusqlite::{params, Connection, Row};
use serde::{Deserialize, Serialize};

pub struct AgentRegistry;

// -- Row mapping --

pub(crate) fn agent_from_row(row: &Row) -> rusqlite::Result<Agent> {
    let status_str: String = row.get("status")?;
    let mode_str: String = row.get("coordination_mode")?;
    let caps_json: String = row.get("capabilities")?;
    let specs_json: String = row.get("specializations")?;
    let registered_str: String = row.get("registered_at")?;
    let seen_str: String = row.get("last_seen")?;

    Ok(Agent {
        id: row.get("id")?,
        project_hash: row.get("project_hash")?,
        name: row.get("name")?,
        description: row
            .get::<_, Option<String>>("description")?
            .unwrap_or_default(),
        role: row
            .get::<_, Option<String>>("role")?
            .unwrap_or_default(),
        capabilities: serde_json::from_str(&caps_json).unwrap_or_default(),
        status: status_str.parse().unwrap_or(AgentStatus::Active),
        last_seen: time_utils::from_sqlite(&seen_str).unwrap_or_else(|_| chrono::Utc::now()),
        registered_at: time_utils::from_sqlite(&registered_str)
            .unwrap_or_else(|_| chrono::Utc::now()),
        supervisor_id: row.get("supervisor_id")?,
        coordination_mode: mode_str.parse().unwrap_or(CoordinationMode::Autonomous),
        team: row.get("team")?,
        specializations: serde_json::from_str(&specs_json).unwrap_or_default(),
        thread_mode: row.get::<_, Option<String>>("thread_mode")
            .ok()
            .flatten()
            .and_then(|s| s.parse().ok())
            .unwrap_or(ThreadMode::Normal),
        current_activity: row.get::<_, Option<String>>("current_activity")
            .ok()
            .flatten()
            .unwrap_or_default(),
        report_to: row.get::<_, Option<String>>("report_to")
            .ok()
            .flatten(),
        custom_role: row.get::<_, Option<String>>("custom_role")
            .ok()
            .flatten(),
        workspace_path: row.get::<_, Option<String>>("workspace_path")
            .ok()
            .flatten()
            .unwrap_or_default(),
        full_permissions: row.get::<_, Option<bool>>("full_permissions")
            .ok()
            .flatten()
            .unwrap_or(false),
    })
}

// -- CRUD --

impl AgentRegistry {
    pub fn register(conn: &Connection, agent: &Agent) -> AiResult<()> {
        if let Some(ref sup_id) = agent.supervisor_id {
            Self::validate_hierarchy(conn, &agent.id, sup_id, &agent.project_hash)?;
        }

        conn.execute(
            "INSERT OR REPLACE INTO agents (
                id, project_hash, name, description, role, capabilities,
                status, last_seen, registered_at,
                supervisor_id, coordination_mode, team, specializations, thread_mode,
                report_to, custom_role, workspace_path, full_permissions
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
            params![
                agent.id,
                agent.project_hash,
                agent.name,
                agent.description,
                agent.role,
                serde_json::to_string(&agent.capabilities).unwrap_or_else(|_| "[]".into()),
                agent.status.as_str(),
                time_utils::to_sqlite(&agent.last_seen),
                time_utils::to_sqlite(&agent.registered_at),
                agent.supervisor_id,
                agent.coordination_mode.as_str(),
                agent.team,
                serde_json::to_string(&agent.specializations).unwrap_or_else(|_| "[]".into()),
                agent.thread_mode.as_str(),
                agent.report_to,
                agent.custom_role,
                agent.workspace_path,
                agent.full_permissions,
            ],
        )
        .map_err(|e| AiError::Storage(format!("Register agent failed: {}", e)))?;

        Ok(())
    }

    pub fn unregister(conn: &Connection, agent_id: &str, project_hash: &str) -> AiResult<()> {
        conn.execute(
            "UPDATE agents SET status = 'offline' WHERE id = ?1 AND project_hash = ?2",
            params![agent_id, project_hash],
        )
        .map_err(|e| AiError::Storage(format!("Unregister agent failed: {}", e)))?;
        Ok(())
    }

    pub fn delete(conn: &Connection, agent_id: &str, project_hash: &str) -> AiResult<()> {
        // Refuse to delete the last agent in a project
        let agent_count: usize = conn
            .query_row(
                "SELECT COUNT(*) FROM agents WHERE project_hash = ?1 AND status != 'offline'",
                params![project_hash],
                |r| r.get(0),
            )
            .unwrap_or(0);

        if agent_count <= 1 {
            return Err(AiError::InvalidInput(
                "Cannot delete the last agent in a project".into(),
            ));
        }

        // Check for active tasks assigned BY this agent
        let active_tasks: usize = conn
            .query_row(
                "SELECT COUNT(*) FROM agent_tasks WHERE assigned_by = ?1 AND status IN ('pending', 'in_progress')",
                params![agent_id],
                |r| r.get(0),
            )
            .unwrap_or(0);

        if active_tasks > 0 {
            return Err(AiError::InvalidInput(format!(
                "Cannot delete agent '{}': has {} active delegated tasks",
                agent_id, active_tasks
            )));
        }

        // Get supervisor for subordinate promotion
        let supervisor_id: Option<String> = conn
            .query_row(
                "SELECT supervisor_id FROM agents WHERE id = ?1 AND project_hash = ?2",
                params![agent_id, project_hash],
                |r| r.get(0),
            )
            .unwrap_or(None);

        // Promote subordinates
        match &supervisor_id {
            Some(sup_id) => {
                conn.execute(
                    "UPDATE agents SET supervisor_id = ?1 WHERE supervisor_id = ?2 AND project_hash = ?3",
                    params![sup_id, agent_id, project_hash],
                )
                .map_err(|e| AiError::Storage(e.to_string()))?;
            }
            None => {
                conn.execute(
                    "UPDATE agents SET supervisor_id = NULL, coordination_mode = 'autonomous' \
                     WHERE supervisor_id = ?1 AND project_hash = ?2",
                    params![agent_id, project_hash],
                )
                .map_err(|e| AiError::Storage(e.to_string()))?;
            }
        }

        // Delete tasks assigned TO this agent
        conn.execute(
            "DELETE FROM agent_tasks WHERE assigned_to = ?1",
            params![agent_id],
        )
        .map_err(|e| AiError::Storage(e.to_string()))?;

        // Cascade: nullify report_to references to deleted agent
        conn.execute(
            "UPDATE agents SET report_to = NULL WHERE report_to = ?1 AND project_hash = ?2",
            params![agent_id, project_hash],
        )
        .map_err(|e| AiError::Storage(e.to_string()))?;

        // Delete agent
        conn.execute(
            "DELETE FROM agents WHERE id = ?1 AND project_hash = ?2",
            params![agent_id, project_hash],
        )
        .map_err(|e| AiError::Storage(format!("Delete agent failed: {}", e)))?;

        // --- Filesystem cleanup (after successful DB deletion) ---
        use crate::storage::path_utils;

        // 1. Remove agent DB file + WAL/SHM
        let agent_db = path_utils::agent_db_path(project_hash, agent_id);
        if agent_db.exists() {
            let _ = std::fs::remove_file(&agent_db);
            let _ = std::fs::remove_file(agent_db.with_extension("db-shm"));
            let _ = std::fs::remove_file(agent_db.with_extension("db-wal"));
        }

        // 2. Remove agent data directory (beat.json, pins, profile, etc.)
        let agent_dir = path_utils::agent_data_dir(project_hash, agent_id);
        if agent_dir.exists() {
            let _ = std::fs::remove_dir_all(&agent_dir);
        }

        // 3. Clean session files pointing to this agent
        let session_path = path_utils::agent_session_path(project_hash);
        if let Ok(content) = std::fs::read_to_string(&session_path) {
            if content.trim() == agent_id {
                let _ = std::fs::remove_file(&session_path);
            }
        }

        // 4. Clean per-session files pointing to this agent
        let session_dir = path_utils::session_agents_dir(project_hash);
        if session_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&session_dir) {
                for entry in entries.flatten() {
                    if let Ok(content) = std::fs::read_to_string(entry.path()) {
                        if content.trim() == agent_id {
                            let _ = std::fs::remove_file(entry.path());
                        }
                    }
                }
            }
        }

        // 5. Remove wake signal file
        let wake_path = path_utils::wake_signal_path(agent_id);
        if wake_path.exists() {
            let _ = std::fs::remove_file(&wake_path);
        }

        tracing::info!(agent = %agent_id, project = %project_hash, "Agent deleted with filesystem cleanup");

        Ok(())
    }

    /// Rename an agent in-place: update registry + filesystem (DB file, data dir, session files).
    pub fn rename(
        conn: &Connection,
        old_id: &str,
        new_id: &str,
        project_hash: &str,
    ) -> AiResult<()> {
        use crate::storage::path_utils;

        // Validate: old exists, new does not
        let old_agent = Self::get(conn, old_id, project_hash)?
            .ok_or_else(|| AiError::AgentNotFound(old_id.to_string()))?;
        if Self::get(conn, new_id, project_hash)?.is_some() {
            return Err(AiError::InvalidInput(format!(
                "Agent '{}' already exists",
                new_id
            )));
        }

        // SQL updates in a transaction
        let tx = conn.unchecked_transaction()
            .map_err(|e| AiError::Storage(format!("Transaction failed: {}", e)))?;

        tx.execute(
            "UPDATE agents SET id = ?1 WHERE id = ?2 AND project_hash = ?3",
            params![new_id, old_id, project_hash],
        )
        .map_err(|e| AiError::Storage(format!("Rename agent failed: {}", e)))?;

        tx.execute(
            "UPDATE agents SET supervisor_id = ?1 WHERE supervisor_id = ?2 AND project_hash = ?3",
            params![new_id, old_id, project_hash],
        )
        .map_err(|e| AiError::Storage(e.to_string()))?;

        // Cascade report_to references
        tx.execute(
            "UPDATE agents SET report_to = ?1 WHERE report_to = ?2 AND project_hash = ?3",
            params![new_id, old_id, project_hash],
        )
        .map_err(|e| AiError::Storage(e.to_string()))?;

        // Best-effort task updates (table may not exist yet)
        let _ = tx.execute(
            "UPDATE agent_tasks SET assigned_to = ?1 WHERE assigned_to = ?2",
            params![new_id, old_id],
        );
        let _ = tx.execute(
            "UPDATE agent_tasks SET assigned_by = ?1 WHERE assigned_by = ?2",
            params![new_id, old_id],
        );

        tx.commit()
            .map_err(|e| AiError::Storage(format!("Commit failed: {}", e)))?;

        // Filesystem: rename agent DB file
        let old_db = path_utils::agent_db_path(project_hash, old_id);
        let new_db = path_utils::agent_db_path(project_hash, new_id);
        if old_db.exists() {
            std::fs::rename(&old_db, &new_db).map_err(|e| {
                AiError::Storage(format!(
                    "Failed to rename DB {} -> {}: {}",
                    old_db.display(),
                    new_db.display(),
                    e
                ))
            })?;
        }

        // Filesystem: rename agent data dir
        let old_dir = path_utils::agent_data_dir(project_hash, old_id);
        let new_dir = path_utils::agent_data_dir(project_hash, new_id);
        if old_dir.exists() {
            let _ = std::fs::rename(&old_dir, &new_dir);
        }

        // Update session files that reference old_id
        let session_path = path_utils::agent_session_path(project_hash);
        if let Ok(content) = std::fs::read_to_string(&session_path) {
            if content.trim() == old_id {
                let _ = std::fs::write(&session_path, new_id);
            }
        }

        // Update per-session files
        let session_dir = path_utils::session_agents_dir(project_hash);
        if session_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&session_dir) {
                for entry in entries.flatten() {
                    if let Ok(content) = std::fs::read_to_string(entry.path()) {
                        if content.trim() == old_id {
                            let _ = std::fs::write(entry.path(), new_id);
                        }
                    }
                }
            }
        }

        tracing::info!(
            old_id = %old_id,
            new_id = %new_id,
            project = %project_hash,
            "Agent renamed in-place"
        );

        // Return with the old agent data preserved (only id changed)
        drop(old_agent);
        Ok(())
    }

    pub fn list(
        conn: &Connection,
        project_hash: Option<&str>,
        team: Option<&str>,
        status_filter: Option<&str>,
    ) -> AiResult<Vec<Agent>> {
        let mut sql = String::from("SELECT * FROM agents WHERE 1=1");
        let mut values: Vec<String> = Vec::new();

        if let Some(ph) = project_hash {
            values.push(ph.to_string());
            sql.push_str(&format!(" AND project_hash = ?{}", values.len()));
        }

        if let Some(t) = team {
            values.push(t.to_string());
            sql.push_str(&format!(" AND team = ?{}", values.len()));
        }

        if let Some(sf) = status_filter {
            values.push(sf.to_string());
            sql.push_str(&format!(" AND status = ?{}", values.len()));
        } else {
            sql.push_str(" AND status != 'offline'");
        }

        sql.push_str(" ORDER BY name ASC");

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| AiError::Storage(e.to_string()))?;

        let params: Vec<&dyn rusqlite::ToSql> =
            values.iter().map(|v| v as &dyn rusqlite::ToSql).collect();

        let agents = stmt
            .query_map(params.as_slice(), agent_from_row)
            .map_err(|e| AiError::Storage(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(agents)
    }

    pub fn get(
        conn: &Connection,
        agent_id: &str,
        project_hash: &str,
    ) -> AiResult<Option<Agent>> {
        let mut stmt = conn
            .prepare("SELECT * FROM agents WHERE id = ?1 AND project_hash = ?2")
            .map_err(|e| AiError::Storage(e.to_string()))?;

        let result = stmt
            .query_row(params![agent_id, project_hash], agent_from_row)
            .optional()
            .map_err(|e| AiError::Storage(e.to_string()))?;

        Ok(result)
    }

    pub fn count(conn: &Connection, project_hash: Option<&str>) -> AiResult<usize> {
        let count: usize = match project_hash {
            Some(ph) => conn.query_row(
                "SELECT COUNT(*) FROM agents WHERE project_hash = ?1 AND status != 'offline'",
                params![ph],
                |r| r.get(0),
            ),
            None => conn.query_row(
                "SELECT COUNT(*) FROM agents WHERE status != 'offline'",
                [],
                |r| r.get(0),
            ),
        }
        .map_err(|e| AiError::Storage(e.to_string()))?;

        Ok(count)
    }

    pub fn update(
        conn: &Connection,
        agent_id: &str,
        project_hash: &str,
        updates: &AgentUpdate,
    ) -> AiResult<()> {
        // Validate hierarchy if supervisor_id being set
        if let Some(Some(ref sup_id)) = updates.supervisor_id {
            Self::validate_hierarchy(conn, agent_id, sup_id, project_hash)?;
        }

        // Build dynamic SET clause
        let mut sets = Vec::new();
        let mut values: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(ref name) = updates.name {
            values.push(Box::new(name.clone()));
            sets.push(format!("name = ?{}", values.len()));
        }
        if let Some(ref role) = updates.role {
            values.push(Box::new(role.clone()));
            sets.push(format!("role = ?{}", values.len()));
        }
        if let Some(ref desc) = updates.description {
            values.push(Box::new(desc.clone()));
            sets.push(format!("description = ?{}", values.len()));
        }
        if let Some(ref sup) = updates.supervisor_id {
            values.push(Box::new(sup.clone()));
            sets.push(format!("supervisor_id = ?{}", values.len()));
        }
        if let Some(ref mode) = updates.coordination_mode {
            values.push(Box::new(mode.clone()));
            sets.push(format!("coordination_mode = ?{}", values.len()));
        }
        if let Some(ref team) = updates.team {
            values.push(Box::new(team.clone()));
            sets.push(format!("team = ?{}", values.len()));
        }
        if let Some(ref specs) = updates.specializations {
            values.push(Box::new(
                serde_json::to_string(specs).unwrap_or_else(|_| "[]".into()),
            ));
            sets.push(format!("specializations = ?{}", values.len()));
        }
        if let Some(ref caps) = updates.capabilities {
            values.push(Box::new(
                serde_json::to_string(caps).unwrap_or_else(|_| "[]".into()),
            ));
            sets.push(format!("capabilities = ?{}", values.len()));
        }
        if let Some(ref mode) = updates.thread_mode {
            values.push(Box::new(mode.clone()));
            sets.push(format!("thread_mode = ?{}", values.len()));
        }
        if let Some(ref report_to) = updates.report_to {
            values.push(Box::new(report_to.clone()));
            sets.push(format!("report_to = ?{}", values.len()));
        }
        if let Some(ref custom_role) = updates.custom_role {
            values.push(Box::new(custom_role.clone()));
            sets.push(format!("custom_role = ?{}", values.len()));
        }
        if let Some(ref wp) = updates.workspace_path {
            values.push(Box::new(wp.clone()));
            sets.push(format!("workspace_path = ?{}", values.len()));
        }
        if let Some(fp) = updates.full_permissions {
            values.push(Box::new(fp));
            sets.push(format!("full_permissions = ?{}", values.len()));
        }

        if sets.is_empty() {
            return Ok(());
        }

        // Always update last_seen
        values.push(Box::new(time_utils::to_sqlite(&chrono::Utc::now())));
        sets.push(format!("last_seen = ?{}", values.len()));

        // WHERE clause
        values.push(Box::new(agent_id.to_string()));
        let id_idx = values.len();
        values.push(Box::new(project_hash.to_string()));
        let ph_idx = values.len();

        let sql = format!(
            "UPDATE agents SET {} WHERE id = ?{} AND project_hash = ?{}",
            sets.join(", "),
            id_idx,
            ph_idx
        );

        let params_ref: Vec<&dyn rusqlite::ToSql> =
            values.iter().map(|b| b.as_ref()).collect();

        conn.execute(&sql, params_ref.as_slice())
            .map_err(|e| AiError::Storage(format!("Update agent failed: {}", e)))?;

        Ok(())
    }

    /// Update topology fields (report_to, custom_role) for an agent.
    pub fn update_topology(
        conn: &Connection,
        agent_id: &str,
        project_hash: &str,
        report_to: Option<String>,
        custom_role: Option<String>,
    ) -> AiResult<()> {
        let updates = AgentUpdate {
            name: None,
            role: None,
            description: None,
            supervisor_id: None,
            coordination_mode: None,
            team: None,
            specializations: None,
            capabilities: None,
            thread_mode: None,
            report_to,
            custom_role,
            workspace_path: None,
            full_permissions: None,
        };
        Self::update(conn, agent_id, project_hash, &updates)
    }

    /// Set the workspace_path for an agent (worktree isolation).
    pub fn update_workspace(conn: &Connection, agent_id: &str, project_hash: &str, workspace_path: &str) -> AiResult<()> {
        conn.execute(
            "UPDATE agents SET workspace_path = ?1 WHERE id = ?2 AND project_hash = ?3",
            params![workspace_path, agent_id, project_hash],
        )
        .map_err(|e| AiError::Storage(format!("Update workspace_path failed: {}", e)))?;
        Ok(())
    }

    // -- Hierarchy --

    /// Validate hierarchy -- detect cycles.
    pub fn validate_hierarchy(
        conn: &Connection,
        agent_id: &str,
        proposed_supervisor_id: &str,
        project_hash: &str,
    ) -> AiResult<()> {
        if agent_id == proposed_supervisor_id {
            return Err(AiError::InvalidInput(
                "Agent cannot supervise itself".to_string(),
            ));
        }

        // Walk supervisor chain upward, checking for cycles
        let mut current = proposed_supervisor_id.to_string();
        let mut visited = std::collections::HashSet::new();
        visited.insert(agent_id.to_string());

        for _ in 0..10 {
            if visited.contains(&current) {
                return Err(AiError::InvalidInput(format!(
                    "Hierarchy cycle detected: {} -> ... -> {}",
                    agent_id, current
                )));
            }
            visited.insert(current.clone());

            let sup: Option<String> = conn
                .query_row(
                    "SELECT supervisor_id FROM agents WHERE id = ?1 AND project_hash = ?2",
                    params![current, project_hash],
                    |r| r.get(0),
                )
                .unwrap_or(None);

            match sup {
                Some(s) => current = s,
                None => break,
            }
        }

        Ok(())
    }

    /// List direct subordinates.
    pub fn list_subordinates(
        conn: &Connection,
        supervisor_id: &str,
        project_hash: &str,
    ) -> AiResult<Vec<Agent>> {
        let mut stmt = conn
            .prepare(
                "SELECT * FROM agents WHERE supervisor_id = ?1 AND project_hash = ?2 \
                 AND status != 'offline' ORDER BY name ASC",
            )
            .map_err(|e| AiError::Storage(e.to_string()))?;

        let agents = stmt
            .query_map(params![supervisor_id, project_hash], agent_from_row)
            .map_err(|e| AiError::Storage(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(agents)
    }

    /// Get supervisor chain from agent to root.
    pub fn get_supervisor_chain(
        conn: &Connection,
        agent_id: &str,
        project_hash: &str,
    ) -> AiResult<Vec<Agent>> {
        let mut chain = Vec::new();
        let agent = match Self::get(conn, agent_id, project_hash)? {
            Some(a) => a,
            None => return Ok(chain),
        };

        let mut current_sup = agent.supervisor_id;
        for _ in 0..10 {
            match current_sup {
                Some(ref sup_id) => {
                    if let Some(sup) = Self::get(conn, sup_id, project_hash)? {
                        current_sup = sup.supervisor_id.clone();
                        chain.push(sup);
                    } else {
                        break;
                    }
                }
                None => break,
            }
        }

        Ok(chain)
    }

    /// List agents by team.
    pub fn list_by_team(
        conn: &Connection,
        team: &str,
        project_hash: &str,
    ) -> AiResult<Vec<Agent>> {
        Self::list(conn, Some(project_hash), Some(team), None)
    }

    /// Build hierarchy tree.
    pub fn build_hierarchy_tree(
        conn: &Connection,
        project_hash: &str,
    ) -> AiResult<Vec<HierarchyNode>> {
        let all_agents = Self::list(conn, Some(project_hash), None, None)?;

        // Count active tasks per agent
        let mut task_counts = std::collections::HashMap::new();
        if let Ok(mut stmt) = conn.prepare(
            "SELECT assigned_to, COUNT(*) FROM agent_tasks \
             WHERE project_hash = ?1 AND status IN ('pending', 'in_progress') \
             GROUP BY assigned_to",
        ) {
            if let Ok(rows) = stmt.query_map(params![project_hash], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, usize>(1)?))
            }) {
                for row in rows.flatten() {
                    task_counts.insert(row.0, row.1);
                }
            }
        }

        // Build maps
        let mut children_map: std::collections::HashMap<String, Vec<&Agent>> =
            std::collections::HashMap::new();
        let mut roots = Vec::new();

        for agent in &all_agents {
            match &agent.supervisor_id {
                Some(sup_id) => {
                    children_map
                        .entry(sup_id.clone())
                        .or_default()
                        .push(agent);
                }
                None => {
                    roots.push(agent);
                }
            }
        }

        fn build_node(
            agent: &Agent,
            children_map: &std::collections::HashMap<String, Vec<&Agent>>,
            task_counts: &std::collections::HashMap<String, usize>,
        ) -> HierarchyNode {
            let subordinates = children_map
                .get(&agent.id)
                .map(|children| {
                    children
                        .iter()
                        .map(|c| build_node(c, children_map, task_counts))
                        .collect()
                })
                .unwrap_or_default();

            HierarchyNode {
                id: agent.id.clone(),
                name: agent.name.clone(),
                role: agent.role.clone(),
                mode: agent.coordination_mode.as_str().to_string(),
                team: agent.team.clone(),
                subordinates,
                active_tasks: *task_counts.get(&agent.id).unwrap_or(&0),
            }
        }

        let tree: Vec<HierarchyNode> = roots
            .iter()
            .map(|r| build_node(r, &children_map, &task_counts))
            .collect();

        Ok(tree)
    }
}

/// Partial update for agents.
pub struct AgentUpdate {
    pub name: Option<String>,
    pub role: Option<String>,
    pub description: Option<String>,
    pub supervisor_id: Option<Option<String>>,
    pub coordination_mode: Option<String>,
    pub team: Option<Option<String>>,
    pub specializations: Option<Vec<String>>,
    pub capabilities: Option<Vec<String>>,
    pub thread_mode: Option<String>,
    pub report_to: Option<String>,
    pub custom_role: Option<String>,
    pub workspace_path: Option<String>,
    pub full_permissions: Option<bool>,
}

/// Hierarchy tree node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HierarchyNode {
    pub id: String,
    pub name: String,
    pub role: String,
    pub mode: String,
    pub team: Option<String>,
    pub subordinates: Vec<HierarchyNode>,
    pub active_tasks: usize,
}

// -- Trait for optional() --

trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, AgentStatus, CoordinationMode, ThreadMode};
    use crate::test_helpers::setup_registry_db;

    const PH: &str = "test-project-hash";

    fn insert_project(conn: &Connection) {
        let now = time_utils::to_sqlite(&time_utils::now());
        conn.execute(
            "INSERT INTO projects (hash, path, name, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![PH, "/tmp/test-project", "test-project", now],
        ).unwrap();
    }

    fn make_agent(id: &str) -> Agent {
        let now = chrono::Utc::now();
        Agent {
            id: id.to_string(),
            project_hash: PH.to_string(),
            name: id.to_string(),
            description: String::new(),
            role: "programmer".to_string(),
            capabilities: vec![],
            status: AgentStatus::Active,
            last_seen: now,
            registered_at: now,
            supervisor_id: None,
            coordination_mode: CoordinationMode::Autonomous,
            team: None,
            specializations: vec![],
            thread_mode: ThreadMode::Normal,
            current_activity: String::new(),
            report_to: None,
            custom_role: None,
            workspace_path: String::new(),
            full_permissions: false,
        }
    }

    #[test]
    fn test_topology_fields_persist() {
        let conn = setup_registry_db();
        insert_project(&conn);
        let mut agent = make_agent("coder1");
        agent.report_to = Some("cor".to_string());
        agent.custom_role = Some("senior-dev".to_string());
        AgentRegistry::register(&conn, &agent).unwrap();

        let got = AgentRegistry::get(&conn, "coder1", PH).unwrap().unwrap();
        assert_eq!(got.report_to, Some("cor".to_string()));
        assert_eq!(got.custom_role, Some("senior-dev".to_string()));
    }

    #[test]
    fn test_delete_cascades_report_to() {
        let conn = setup_registry_db();
        insert_project(&conn);

        // Register cor + coder1 that reports to cor
        AgentRegistry::register(&conn, &make_agent("cor")).unwrap();
        let mut coder = make_agent("coder1");
        coder.report_to = Some("cor".to_string());
        AgentRegistry::register(&conn, &coder).unwrap();

        // Also register coder2 so cor isn't the last agent
        AgentRegistry::register(&conn, &make_agent("coder2")).unwrap();

        // Delete cor → coder1.report_to should be nullified
        AgentRegistry::delete(&conn, "cor", PH).unwrap();
        let got = AgentRegistry::get(&conn, "coder1", PH).unwrap().unwrap();
        assert_eq!(got.report_to, None, "report_to should be nullified after target deleted");
    }

    #[test]
    fn test_rename_propagates_report_to() {
        let conn = setup_registry_db();
        insert_project(&conn);

        AgentRegistry::register(&conn, &make_agent("old-cor")).unwrap();
        let mut coder = make_agent("coder1");
        coder.report_to = Some("old-cor".to_string());
        AgentRegistry::register(&conn, &coder).unwrap();

        // Rename old-cor → new-cor
        AgentRegistry::rename(&conn, "old-cor", "new-cor", PH).unwrap();

        let got = AgentRegistry::get(&conn, "coder1", PH).unwrap().unwrap();
        assert_eq!(got.report_to, Some("new-cor".to_string()), "report_to should follow rename");
    }

    #[test]
    fn test_option_string_null_roundtrip() {
        let conn = setup_registry_db();
        insert_project(&conn);
        let agent = make_agent("agent-null");
        // report_to and custom_role are None by default
        AgentRegistry::register(&conn, &agent).unwrap();

        let got = AgentRegistry::get(&conn, "agent-null", PH).unwrap().unwrap();
        assert_eq!(got.report_to, None);
        assert_eq!(got.custom_role, None);
    }
}
