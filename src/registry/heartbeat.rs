//! Heartbeat -- agent liveness tracking.

use crate::time_utils;
use crate::{AiError, AiResult};
use chrono::Utc;
use rusqlite::{params, Connection};

/// Heartbeat configuration — agent liveness thresholds.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HeartbeatConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub alive_threshold_secs: u64,
    pub idle_threshold_secs: u64,
    pub offline_threshold_secs: u64,
}

fn default_enabled() -> bool { true }

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            alive_threshold_secs: 300,    // 5 min
            idle_threshold_secs: 300,     // 5 min
            offline_threshold_secs: 1800, // 30 min
        }
    }
}

pub struct Heartbeat;

impl Heartbeat {
    /// Update heartbeat for an agent. Optionally sets `current_activity`.
    pub fn update(conn: &Connection, agent_id: &str, project_hash: &str, activity: Option<&str>) -> AiResult<()> {
        let now = time_utils::to_sqlite(&time_utils::now());
        tracing::info!(agent = agent_id, project = project_hash, "Heartbeat update");
        let affected = match activity {
            Some(act) => conn.execute(
                "UPDATE agents SET last_seen = ?1, status = 'active', current_activity = ?4 \
                 WHERE id = ?2 AND project_hash = ?3",
                params![now, agent_id, project_hash, act],
            ),
            None => conn.execute(
                "UPDATE agents SET last_seen = ?1, status = 'active' \
                 WHERE id = ?2 AND project_hash = ?3",
                params![now, agent_id, project_hash],
            ),
        }
        .map_err(|e| AiError::Storage(format!("Heartbeat update failed: {}", e)))?;

        if affected == 0 {
            tracing::warn!(agent = agent_id, "Heartbeat update: agent not found");
            return Err(AiError::InvalidInput(format!(
                "Agent '{}' not found",
                agent_id
            )));
        }
        Ok(())
    }

    /// Check if an agent is alive (seen within threshold).
    pub fn is_alive(
        conn: &Connection,
        agent_id: &str,
        project_hash: &str,
        config: &HeartbeatConfig,
    ) -> AiResult<bool> {
        let seen_str: Option<String> = conn
            .query_row(
                "SELECT last_seen FROM agents WHERE id = ?1 AND project_hash = ?2",
                params![agent_id, project_hash],
                |r| r.get(0),
            )
            .ok();

        match seen_str {
            Some(s) => {
                if let Ok(seen) = time_utils::from_sqlite(&s) {
                    let age_secs = (Utc::now() - seen).num_seconds();
                    let alive = age_secs < config.alive_threshold_secs as i64;
                    tracing::debug!(agent = agent_id, age_secs, threshold = config.alive_threshold_secs, alive, "Heartbeat is_alive check");
                    Ok(alive)
                } else {
                    tracing::warn!(agent = agent_id, last_seen = %s, "Heartbeat: invalid timestamp");
                    Ok(false)
                }
            }
            None => {
                tracing::debug!(agent = agent_id, "Heartbeat: agent not found in DB");
                Ok(false)
            }
        }
    }

    /// Mark stale agents (idle/offline based on config thresholds).
    /// Returns count of updated agents.
    pub fn mark_stale(conn: &Connection, config: &HeartbeatConfig) -> AiResult<usize> {
        let now = Utc::now();
        let mut updated = 0;

        let mut stmt = conn
            .prepare(
                "SELECT id, project_hash, last_seen FROM agents \
                 WHERE status IN ('active', 'idle')",
            )
            .map_err(|e| AiError::Storage(e.to_string()))?;

        let agents: Vec<(String, String, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .map_err(|e| AiError::Storage(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        tracing::info!(
            agent_count = agents.len(),
            idle_threshold = config.idle_threshold_secs,
            offline_threshold = config.offline_threshold_secs,
            "Heartbeat mark_stale starting"
        );

        for (id, ph, seen_str) in &agents {
            if let Ok(seen) = time_utils::from_sqlite(seen_str) {
                let age_secs = (now - seen).num_seconds();
                if age_secs >= config.offline_threshold_secs as i64 {
                    tracing::info!(agent = %id, age_secs, "Heartbeat: marking offline");
                    conn.execute(
                        "UPDATE agents SET status = 'offline' WHERE id = ?1 AND project_hash = ?2",
                        params![id, ph],
                    )
                    .map_err(|e| AiError::Storage(e.to_string()))?;
                    updated += 1;
                } else if age_secs >= config.idle_threshold_secs as i64 {
                    tracing::info!(agent = %id, age_secs, "Heartbeat: marking idle");
                    conn.execute(
                        "UPDATE agents SET status = 'idle' WHERE id = ?1 AND project_hash = ?2",
                        params![id, ph],
                    )
                    .map_err(|e| AiError::Storage(e.to_string()))?;
                    updated += 1;
                }
            }
        }

        if updated > 0 {
            tracing::info!(updated, "Heartbeat mark_stale complete");
        }
        Ok(updated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, AgentStatus, CoordinationMode, ThreadMode};
    use crate::registry::registry::AgentRegistry;
    use crate::test_helpers::setup_registry_db;

    const PH: &str = "test-ph-hb";
    const AGENT: &str = "test-agent-hb";

    fn register_test_agent(conn: &Connection) {
        let now = time_utils::to_sqlite(&time_utils::now());
        conn.execute(
            "INSERT INTO projects (hash, path, name, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![PH, "/tmp/test", "test", now],
        ).unwrap();

        let now_dt = Utc::now();
        let agent = Agent {
            id: AGENT.to_string(),
            project_hash: PH.to_string(),
            name: "Test Agent".to_string(),
            description: String::new(),
            role: "programmer".to_string(),
            capabilities: vec![],
            status: AgentStatus::Active,
            last_seen: now_dt,
            registered_at: now_dt,
            supervisor_id: None,
            coordination_mode: CoordinationMode::Autonomous,
            team: None,
            specializations: vec![],
            thread_mode: ThreadMode::Normal,
            current_activity: String::new(),
            report_to: None,
            custom_role: None,
            workspace_path: String::new(),
        };
        AgentRegistry::register(conn, &agent).unwrap();
    }

    #[test]
    fn test_heartbeat_update_with_activity() {
        let conn = setup_registry_db();
        register_test_agent(&conn);

        Heartbeat::update(&conn, AGENT, PH, Some("tool:ai_recall")).unwrap();

        let activity: String = conn.query_row(
            "SELECT current_activity FROM agents WHERE id = ?1 AND project_hash = ?2",
            params![AGENT, PH],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(activity, "tool:ai_recall");
    }

    #[test]
    fn test_heartbeat_update_none_preserves_activity() {
        let conn = setup_registry_db();
        register_test_agent(&conn);

        Heartbeat::update(&conn, AGENT, PH, Some("tool:ai_merge")).unwrap();
        Heartbeat::update(&conn, AGENT, PH, None).unwrap();

        let activity: String = conn.query_row(
            "SELECT current_activity FROM agents WHERE id = ?1 AND project_hash = ?2",
            params![AGENT, PH],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(activity, "tool:ai_merge", "Activity preserved when None");
    }
}
