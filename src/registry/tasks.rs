//! Agent Task Storage -- CRUD for agent tasks with dependency validation.

use crate::time_utils;
use crate::agent::{AgentTask, TaskPriority, TaskStatus};
use crate::{AiError, AiResult};
use rusqlite::{params, Connection, Row};

pub struct AgentTaskStorage;

fn task_from_row(row: &Row) -> rusqlite::Result<AgentTask> {
    let priority_str: String = row.get("priority")?;
    let status_str: String = row.get("status")?;
    let created_str: String = row.get("created_at")?;
    let updated_str: String = row.get("updated_at")?;
    let deadline_str: Option<String> = row.get("deadline")?;
    let deps_json: String = row.get("dependencies")?;

    Ok(AgentTask {
        id: row.get("id")?,
        project_hash: row.get("project_hash")?,
        assigned_to: row.get("assigned_to")?,
        assigned_by: row.get("assigned_by")?,
        title: row.get("title")?,
        description: row
            .get::<_, Option<String>>("description")?
            .unwrap_or_default(),
        priority: priority_str.parse().unwrap_or(TaskPriority::Normal),
        status: status_str.parse().unwrap_or(TaskStatus::Pending),
        created_at: time_utils::from_sqlite(&created_str).unwrap_or_else(|_| chrono::Utc::now()),
        updated_at: time_utils::from_sqlite(&updated_str).unwrap_or_else(|_| chrono::Utc::now()),
        deadline: deadline_str.and_then(|s| time_utils::from_sqlite(&s).ok()),
        dependencies: serde_json::from_str(&deps_json).unwrap_or_default(),
        result: row.get("result")?,
    })
}

impl AgentTaskStorage {
    /// Create a task with dependency and assignee validation.
    pub fn create_task(conn: &Connection, task: &AgentTask) -> AiResult<()> {
        // Verify assigned_to agent exists
        let agent_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM agents WHERE id = ?1 AND project_hash = ?2 AND status != 'offline'",
                params![task.assigned_to, task.project_hash],
                |r| r.get(0),
            )
            .unwrap_or(false);

        if !agent_exists {
            return Err(AiError::InvalidInput(format!(
                "Agent '{}' not found or offline",
                task.assigned_to
            )));
        }

        // Verify dependencies exist
        for dep_id in &task.dependencies {
            let dep_exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM agent_tasks WHERE id = ?1",
                    params![dep_id],
                    |r| r.get(0),
                )
                .unwrap_or(false);

            if !dep_exists {
                return Err(AiError::InvalidInput(format!(
                    "Dependency task '{}' not found",
                    dep_id
                )));
            }
        }

        let now = time_utils::to_sqlite(&time_utils::now());
        conn.execute(
            "INSERT INTO agent_tasks (
                id, project_hash, assigned_to, assigned_by, title, description,
                priority, status, created_at, updated_at, deadline, dependencies, result
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                task.id,
                task.project_hash,
                task.assigned_to,
                task.assigned_by,
                task.title,
                task.description,
                task.priority.as_str(),
                task.status.as_str(),
                now,
                now,
                task.deadline.map(|d| time_utils::to_sqlite(&d)),
                serde_json::to_string(&task.dependencies).unwrap_or_else(|_| "[]".into()),
                task.result,
            ],
        )
        .map_err(|e| AiError::Storage(format!("Create task failed: {}", e)))?;

        Ok(())
    }

    pub fn get_task(conn: &Connection, task_id: &str, project_hash: &str) -> AiResult<Option<AgentTask>> {
        let mut stmt = conn
            .prepare("SELECT * FROM agent_tasks WHERE id = ?1 AND project_hash = ?2")
            .map_err(|e| AiError::Storage(e.to_string()))?;

        let result = stmt
            .query_row(params![task_id, project_hash], task_from_row)
            .optional()
            .map_err(|e| AiError::Storage(e.to_string()))?;

        Ok(result)
    }

    /// List tasks with optional filters, ordered by priority then creation date.
    pub fn list_tasks(
        conn: &Connection,
        project_hash: &str,
        status_filter: Option<&str>,
        priority_filter: Option<&str>,
    ) -> AiResult<Vec<AgentTask>> {
        let mut sql = String::from("SELECT * FROM agent_tasks WHERE project_hash = ?1");
        let mut values: Vec<String> = vec![project_hash.to_string()];

        if let Some(sf) = status_filter {
            values.push(sf.to_string());
            sql.push_str(&format!(" AND status = ?{}", values.len()));
        }
        if let Some(pf) = priority_filter {
            values.push(pf.to_string());
            sql.push_str(&format!(" AND priority = ?{}", values.len()));
        }

        sql.push_str(
            " ORDER BY CASE priority \
             WHEN 'critical' THEN 0 WHEN 'high' THEN 1 \
             WHEN 'normal' THEN 2 WHEN 'low' THEN 3 END, created_at ASC",
        );

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| AiError::Storage(e.to_string()))?;

        let params: Vec<&dyn rusqlite::ToSql> =
            values.iter().map(|v| v as &dyn rusqlite::ToSql).collect();

        let tasks = stmt
            .query_map(params.as_slice(), task_from_row)
            .map_err(|e| AiError::Storage(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(tasks)
    }

    /// List tasks assigned to a specific agent.
    pub fn list_tasks_for_agent(
        conn: &Connection,
        agent_id: &str,
        project_hash: &str,
    ) -> AiResult<Vec<AgentTask>> {
        let mut stmt = conn
            .prepare(
                "SELECT * FROM agent_tasks WHERE assigned_to = ?1 AND project_hash = ?2 \
                 ORDER BY CASE priority \
                 WHEN 'critical' THEN 0 WHEN 'high' THEN 1 \
                 WHEN 'normal' THEN 2 WHEN 'low' THEN 3 END, created_at ASC",
            )
            .map_err(|e| AiError::Storage(e.to_string()))?;

        let tasks = stmt
            .query_map(params![agent_id, project_hash], task_from_row)
            .map_err(|e| AiError::Storage(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(tasks)
    }

    /// Update task status with dependency validation for InProgress transitions.
    pub fn update_task_status(
        conn: &Connection,
        task_id: &str,
        new_status: TaskStatus,
        result: Option<&str>,
    ) -> AiResult<()> {
        // Validate dependencies if transitioning to InProgress
        if new_status == TaskStatus::InProgress {
            let deps_json: String = conn
                .query_row(
                    "SELECT dependencies FROM agent_tasks WHERE id = ?1",
                    params![task_id],
                    |r| r.get(0),
                )
                .map_err(|e| AiError::Storage(e.to_string()))?;

            let deps: Vec<String> = serde_json::from_str(&deps_json).unwrap_or_default();
            for dep_id in &deps {
                let dep_status: String = conn
                    .query_row(
                        "SELECT status FROM agent_tasks WHERE id = ?1",
                        params![dep_id],
                        |r| r.get(0),
                    )
                    .map_err(|e| AiError::Storage(e.to_string()))?;

                if dep_status != "completed" {
                    return Err(AiError::InvalidInput(format!(
                        "Cannot start task: dependency '{}' is not completed (status: {})",
                        dep_id, dep_status
                    )));
                }
            }
        }

        let now = time_utils::to_sqlite(&time_utils::now());
        conn.execute(
            "UPDATE agent_tasks SET status = ?1, result = ?2, updated_at = ?3 WHERE id = ?4",
            params![new_status.as_str(), result, now, task_id],
        )
        .map_err(|e| AiError::Storage(format!("Update task status failed: {}", e)))?;

        Ok(())
    }

    /// Delete a task (validates no active dependents).
    pub fn delete_task(conn: &Connection, task_id: &str) -> AiResult<()> {
        let pattern = format!("%\"{}\"%" , task_id);
        let dependents: usize = conn
            .query_row(
                "SELECT COUNT(*) FROM agent_tasks WHERE dependencies LIKE ?1 AND status != 'completed'",
                params![pattern],
                |r| r.get(0),
            )
            .unwrap_or(0);

        if dependents > 0 {
            return Err(AiError::InvalidInput(format!(
                "Cannot delete task '{}': {} active tasks depend on it",
                task_id, dependents
            )));
        }

        conn.execute("DELETE FROM agent_tasks WHERE id = ?1", params![task_id])
            .map_err(|e| AiError::Storage(format!("Delete task failed: {}", e)))?;

        Ok(())
    }
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
