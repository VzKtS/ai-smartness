//! Discovery -- find agents by capability or specialization.

use crate::agent::Agent;
use crate::{AiError, AiResult};
use rusqlite::{params, Connection};

pub struct Discovery;

impl Discovery {
    /// Find agents by capability (substring match in JSON array).
    pub fn find_by_capability(conn: &Connection, capability: &str) -> AiResult<Vec<Agent>> {
        let pattern = format!("%\"{}\"%" , capability.to_lowercase());
        let mut stmt = conn
            .prepare(
                "SELECT * FROM agents WHERE status != 'offline' AND LOWER(capabilities) LIKE ?1",
            )
            .map_err(|e| AiError::Storage(e.to_string()))?;

        let agents = stmt
            .query_map(params![pattern], crate::registry::registry::agent_from_row)
            .map_err(|e| AiError::Storage(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(agents)
    }

    /// Find agents by specialization (substring match in JSON array).
    pub fn find_by_specialization(conn: &Connection, spec: &str) -> AiResult<Vec<Agent>> {
        let pattern = format!("%\"{}\"%" , spec.to_lowercase());
        let mut stmt = conn
            .prepare(
                "SELECT * FROM agents WHERE status != 'offline' AND LOWER(specializations) LIKE ?1",
            )
            .map_err(|e| AiError::Storage(e.to_string()))?;

        let agents = stmt
            .query_map(params![pattern], crate::registry::registry::agent_from_row)
            .map_err(|e| AiError::Storage(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(agents)
    }
}
