//! Discovery -- find agents by capability or specialization.

use crate::agent::Agent;
use crate::{AiError, AiResult};
use rusqlite::{params, Connection};

pub struct Discovery;

impl Discovery {
    /// Find agents by capability (substring match in JSON array), filtered by project.
    pub fn find_by_capability(conn: &Connection, capability: &str, project_hash: &str) -> AiResult<Vec<Agent>> {
        let pattern = format!("%\"{}\"%" , capability.to_lowercase());
        let mut stmt = conn
            .prepare(
                "SELECT * FROM agents WHERE status != 'offline' \
                 AND project_hash = ?2 AND LOWER(capabilities) LIKE ?1",
            )
            .map_err(|e| AiError::Storage(e.to_string()))?;

        let agents = stmt
            .query_map(params![pattern, project_hash], crate::registry::registry::agent_from_row)
            .map_err(|e| AiError::Storage(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(agents)
    }

    /// Find agents by specialization (substring match in JSON array), filtered by project.
    pub fn find_by_specialization(conn: &Connection, spec: &str, project_hash: &str) -> AiResult<Vec<Agent>> {
        let pattern = format!("%\"{}\"%" , spec.to_lowercase());
        let mut stmt = conn
            .prepare(
                "SELECT * FROM agents WHERE status != 'offline' \
                 AND project_hash = ?2 AND LOWER(specializations) LIKE ?1",
            )
            .map_err(|e| AiError::Storage(e.to_string()))?;

        let agents = stmt
            .query_map(params![pattern, project_hash], crate::registry::registry::agent_from_row)
            .map_err(|e| AiError::Storage(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(agents)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{AgentStatus, CoordinationMode, ThreadMode};
    use crate::registry::registry::AgentRegistry;
    use crate::storage::migrations;

    fn setup_db() -> Connection {
        let conn = Connection::open(":memory:").unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .unwrap();
        migrations::migrate_registry_db(&conn).unwrap();
        conn
    }

    fn insert_project(conn: &Connection, ph: &str) {
        let now = crate::time_utils::to_sqlite(&crate::time_utils::now());
        conn.execute(
            "INSERT INTO projects (hash, path, name, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![ph, format!("/tmp/{}", ph), ph, now],
        )
        .unwrap();
    }

    fn register_with_caps(conn: &Connection, id: &str, ph: &str, caps: Vec<String>) {
        let now = chrono::Utc::now();
        let agent = Agent {
            id: id.into(),
            project_hash: ph.into(),
            name: id.into(),
            description: String::new(),
            role: "dev".into(),
            capabilities: caps,
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
            expected_model: None,
        };
        AgentRegistry::register(conn, &agent).unwrap();
    }

    // T-B1-storage: find_by_capability isolates across projects
    #[test]
    fn test_find_by_capability_cross_project() {
        let conn = setup_db();
        insert_project(&conn, "proj-x");
        insert_project(&conn, "proj-y");
        register_with_caps(&conn, "agent-x", "proj-x", vec!["rust".into(), "coding".into()]);
        register_with_caps(&conn, "agent-y", "proj-y", vec!["coding".into(), "python".into()]);

        // Query project X for "coding" → only agent-x
        let result = Discovery::find_by_capability(&conn, "coding", "proj-x").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "agent-x");

        // Query project Y for "rust" → 0 results (rust agent is in project X)
        let result_y = Discovery::find_by_capability(&conn, "rust", "proj-y").unwrap();
        assert_eq!(result_y.len(), 0, "Capability from project X must be invisible in project Y");
    }
}
