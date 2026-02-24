use crate::time_utils;
use crate::bridge::{BridgeStatus, BridgeType, ThinkBridge};
use crate::{AiError, AiResult};
use rusqlite::{params, Connection, Row};

pub struct BridgeStorage;

// ── Row mapping ──

fn bridge_from_row(row: &Row) -> rusqlite::Result<ThinkBridge> {
    let relation_str: String = row.get("relation_type")?;
    let status_str: String = row.get("status")?;
    let created_str: String = row.get("created_at")?;
    let reinforced_str: Option<String> = row.get("last_reinforced")?;
    let concepts_json: String = row.get("shared_concepts")?;

    Ok(ThinkBridge {
        id: row.get("id")?,
        source_id: row.get("source_id")?,
        target_id: row.get("target_id")?,
        relation_type: relation_str
            .parse()
            .unwrap_or(BridgeType::Extends),
        reason: row.get("reason")?,
        shared_concepts: serde_json::from_str(&concepts_json).unwrap_or_default(),
        weight: row.get("weight")?,
        confidence: row.get("confidence")?,
        status: status_str
            .parse()
            .unwrap_or(BridgeStatus::Active),
        propagated_from: row.get("propagated_from")?,
        propagation_depth: row.get("propagation_depth")?,
        created_by: row.get("created_by")?,
        use_count: row.get("use_count")?,
        created_at: time_utils::from_sqlite(&created_str).unwrap_or_else(|_| chrono::Utc::now()),
        last_reinforced: reinforced_str.and_then(|s| time_utils::from_sqlite(&s).ok()),
    })
}

// ── CRUD ──

impl BridgeStorage {
    pub fn insert(conn: &Connection, bridge: &ThinkBridge) -> AiResult<()> {
        conn.execute(
            "INSERT INTO bridges (
                id, source_id, target_id, relation_type, reason, shared_concepts,
                confidence, weight, status, propagated_from, propagation_depth,
                created_by, use_count, created_at, last_reinforced
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6,
                ?7, ?8, ?9, ?10, ?11,
                ?12, ?13, ?14, ?15
            )",
            params![
                bridge.id,
                bridge.source_id,
                bridge.target_id,
                bridge.relation_type.as_str(),
                bridge.reason,
                serde_json::to_string(&bridge.shared_concepts).unwrap_or_else(|_| "[]".into()),
                bridge.confidence,
                bridge.weight,
                bridge.status.as_str(),
                bridge.propagated_from,
                bridge.propagation_depth,
                bridge.created_by,
                bridge.use_count,
                time_utils::to_sqlite(&bridge.created_at),
                bridge
                    .last_reinforced
                    .map(|dt| time_utils::to_sqlite(&dt)),
            ],
        )
        .map_err(|e| AiError::Storage(format!("Insert bridge failed: {}", e)))?;
        tracing::debug!(bridge_id = %bridge.id, source = %bridge.source_id, target = %bridge.target_id, relation = %bridge.relation_type.as_str(), "Bridge inserted");
        Ok(())
    }

    pub fn get(conn: &Connection, id: &str) -> AiResult<Option<ThinkBridge>> {
        let mut stmt = conn
            .prepare("SELECT * FROM bridges WHERE id = ?1")
            .map_err(|e| AiError::Storage(e.to_string()))?;

        let result = stmt
            .query_row(params![id], bridge_from_row)
            .optional()
            .map_err(|e| AiError::Storage(e.to_string()))?;

        Ok(result)
    }

    pub fn update(conn: &Connection, bridge: &ThinkBridge) -> AiResult<()> {
        conn.execute(
            "UPDATE bridges SET
                relation_type = ?2, reason = ?3, shared_concepts = ?4,
                confidence = ?5, weight = ?6, status = ?7,
                propagated_from = ?8, propagation_depth = ?9,
                use_count = ?10, last_reinforced = ?11
            WHERE id = ?1",
            params![
                bridge.id,
                bridge.relation_type.as_str(),
                bridge.reason,
                serde_json::to_string(&bridge.shared_concepts).unwrap_or_else(|_| "[]".into()),
                bridge.confidence,
                bridge.weight,
                bridge.status.as_str(),
                bridge.propagated_from,
                bridge.propagation_depth,
                bridge.use_count,
                bridge
                    .last_reinforced
                    .map(|dt| time_utils::to_sqlite(&dt)),
            ],
        )
        .map_err(|e| AiError::Storage(format!("Update bridge failed: {}", e)))?;
        Ok(())
    }

    pub fn delete(conn: &Connection, id: &str) -> AiResult<()> {
        conn.execute("DELETE FROM bridges WHERE id = ?1", params![id])
            .map_err(|e| AiError::Storage(format!("Delete bridge failed: {}", e)))?;
        Ok(())
    }

    /// Delete all bridges where source_id or target_id matches the given thread.
    pub fn delete_for_thread(conn: &Connection, thread_id: &str) -> AiResult<usize> {
        let deleted = conn
            .execute(
                "DELETE FROM bridges WHERE source_id = ?1 OR target_id = ?1",
                params![thread_id],
            )
            .map_err(|e| AiError::Storage(format!("Delete bridges for thread failed: {}", e)))?;
        Ok(deleted)
    }

    pub fn list_for_thread(conn: &Connection, thread_id: &str) -> AiResult<Vec<ThinkBridge>> {
        let mut stmt = conn
            .prepare(
                "SELECT * FROM bridges WHERE source_id = ?1 OR target_id = ?1 ORDER BY weight DESC",
            )
            .map_err(|e| AiError::Storage(e.to_string()))?;

        let bridges = stmt
            .query_map(params![thread_id], bridge_from_row)
            .map_err(|e| AiError::Storage(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(bridges)
    }

    pub fn list_active(conn: &Connection) -> AiResult<Vec<ThinkBridge>> {
        let mut stmt = conn
            .prepare("SELECT * FROM bridges WHERE status = 'active' ORDER BY weight DESC")
            .map_err(|e| AiError::Storage(e.to_string()))?;

        let bridges = stmt
            .query_map([], bridge_from_row)
            .map_err(|e| AiError::Storage(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(bridges)
    }

    pub fn list_by_status(conn: &Connection, status: BridgeStatus) -> AiResult<Vec<ThinkBridge>> {
        let mut stmt = conn
            .prepare("SELECT * FROM bridges WHERE status = ?1 ORDER BY weight DESC")
            .map_err(|e| AiError::Storage(e.to_string()))?;

        let bridges = stmt
            .query_map(params![status.as_str()], bridge_from_row)
            .map_err(|e| AiError::Storage(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(bridges)
    }

    pub fn list_all(conn: &Connection) -> AiResult<Vec<ThinkBridge>> {
        let mut stmt = conn
            .prepare("SELECT * FROM bridges WHERE weight > 0 ORDER BY weight DESC")
            .map_err(|e| AiError::Storage(e.to_string()))?;

        let bridges = stmt
            .query_map([], bridge_from_row)
            .map_err(|e| AiError::Storage(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(bridges)
    }

    pub fn update_weight(conn: &Connection, id: &str, weight: f64) -> AiResult<()> {
        conn.execute(
            "UPDATE bridges SET weight = ?1 WHERE id = ?2",
            params![weight, id],
        )
        .map_err(|e| AiError::Storage(format!("Update bridge weight failed: {}", e)))?;
        Ok(())
    }

    pub fn reinforce_weight(conn: &Connection, id: &str, weight: f64) -> AiResult<()> {
        let now = time_utils::to_sqlite(&time_utils::now());
        conn.execute(
            "UPDATE bridges SET weight = ?1, last_reinforced = ?2 WHERE id = ?3",
            params![weight, now, id],
        )
        .map_err(|e| AiError::Storage(format!("Reinforce bridge weight failed: {}", e)))?;
        Ok(())
    }

    pub fn update_last_reinforced(
        conn: &Connection,
        id: &str,
        when: chrono::DateTime<chrono::Utc>,
    ) -> AiResult<()> {
        conn.execute(
            "UPDATE bridges SET last_reinforced = ?1 WHERE id = ?2",
            params![time_utils::to_sqlite(&when), id],
        )
        .map_err(|e| AiError::Storage(format!("Update bridge last_reinforced failed: {}", e)))?;
        Ok(())
    }

    pub fn update_status(conn: &Connection, id: &str, status: BridgeStatus) -> AiResult<()> {
        conn.execute(
            "UPDATE bridges SET status = ?1 WHERE id = ?2",
            params![status.as_str(), id],
        )
        .map_err(|e| AiError::Storage(format!("Update bridge status failed: {}", e)))?;
        Ok(())
    }

    pub fn increment_use(conn: &Connection, id: &str) -> AiResult<()> {
        let now = time_utils::to_sqlite(&time_utils::now());
        conn.execute(
            "UPDATE bridges SET use_count = use_count + 1, last_reinforced = ?1 WHERE id = ?2",
            params![now, id],
        )
        .map_err(|e| AiError::Storage(format!("Increment bridge use failed: {}", e)))?;
        Ok(())
    }

    pub fn scan_orphans(conn: &Connection) -> AiResult<Vec<ThinkBridge>> {
        let mut stmt = conn
            .prepare(
                "SELECT b.* FROM bridges b
                 LEFT JOIN threads t1 ON b.source_id = t1.id
                 LEFT JOIN threads t2 ON b.target_id = t2.id
                 WHERE t1.id IS NULL OR t2.id IS NULL",
            )
            .map_err(|e| AiError::Storage(e.to_string()))?;

        let orphans = stmt
            .query_map([], bridge_from_row)
            .map_err(|e| AiError::Storage(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(orphans)
    }

    pub fn delete_batch(conn: &Connection, ids: &[String]) -> AiResult<usize> {
        if ids.is_empty() {
            return Ok(0);
        }
        if ids.len() > 500 {
            return ids.chunks(500).try_fold(0, |acc, chunk| {
                let chunk_vec: Vec<String> = chunk.to_vec();
                Self::delete_batch(conn, &chunk_vec).map(|n| acc + n)
            });
        }

        let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("?{i}")).collect();
        let sql = format!("DELETE FROM bridges WHERE id IN ({})", placeholders.join(", "));
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = ids
            .iter()
            .map(|s| s as &dyn rusqlite::types::ToSql)
            .collect();
        let deleted = conn
            .execute(&sql, param_refs.as_slice())
            .map_err(|e| AiError::Storage(e.to_string()))?;
        Ok(deleted)
    }

    /// Bulk delete all bridges with a given status.
    /// Returns the number of bridges deleted.
    pub fn delete_by_status(conn: &Connection, status: &BridgeStatus) -> AiResult<usize> {
        let deleted = conn
            .execute(
                "DELETE FROM bridges WHERE status = ?1",
                params![status.as_str()],
            )
            .map_err(|e| AiError::Storage(format!("Delete bridges by status failed: {}", e)))?;
        Ok(deleted)
    }

    /// Count bridges with a given status (lightweight).
    pub fn count_by_status(conn: &Connection, status: &BridgeStatus) -> AiResult<usize> {
        let c: usize = conn
            .query_row(
                "SELECT COUNT(*) FROM bridges WHERE status = ?1",
                params![status.as_str()],
                |r| r.get(0),
            )
            .map_err(|e| AiError::Storage(e.to_string()))?;
        Ok(c)
    }

    pub fn count(conn: &Connection) -> AiResult<usize> {
        let c: usize = conn
            .query_row("SELECT COUNT(*) FROM bridges", [], |r| r.get(0))
            .map_err(|e| AiError::Storage(e.to_string()))?;
        Ok(c)
    }
}

// ── Trait for optional() ──

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
    use crate::test_helpers::{setup_agent_db, setup_agent_db_no_fk, ThreadBuilder, BridgeBuilder};
    use crate::storage::threads::ThreadStorage;

    fn setup_two_threads(conn: &Connection) -> (String, String) {
        let t1 = ThreadBuilder::new().id("t1").build();
        let t2 = ThreadBuilder::new().id("t2").build();
        ThreadStorage::insert(conn, &t1).unwrap();
        ThreadStorage::insert(conn, &t2).unwrap();
        ("t1".to_string(), "t2".to_string())
    }

    #[test]
    fn test_insert_and_get() {
        let conn = setup_agent_db();
        let (s, t) = setup_two_threads(&conn);
        let bridge = BridgeBuilder::new().id("b1").source_id(&s).target_id(&t).build();
        BridgeStorage::insert(&conn, &bridge).unwrap();
        let got = BridgeStorage::get(&conn, "b1").unwrap().unwrap();
        assert_eq!(got.id, "b1");
        assert_eq!(got.source_id, "t1");
        assert_eq!(got.target_id, "t2");
        assert_eq!(got.relation_type, BridgeType::Extends);
    }

    #[test]
    fn test_get_nonexistent() {
        let conn = setup_agent_db();
        assert!(BridgeStorage::get(&conn, "nope").unwrap().is_none());
    }

    #[test]
    fn test_update() {
        let conn = setup_agent_db();
        let (s, t) = setup_two_threads(&conn);
        let mut bridge = BridgeBuilder::new().id("b1").source_id(&s).target_id(&t).build();
        BridgeStorage::insert(&conn, &bridge).unwrap();
        bridge.weight = 0.5;
        bridge.reason = "Updated reason".to_string();
        BridgeStorage::update(&conn, &bridge).unwrap();
        let got = BridgeStorage::get(&conn, "b1").unwrap().unwrap();
        assert!((got.weight - 0.5).abs() < 0.001);
        assert_eq!(got.reason, "Updated reason");
    }

    #[test]
    fn test_delete() {
        let conn = setup_agent_db();
        let (s, t) = setup_two_threads(&conn);
        BridgeStorage::insert(&conn, &BridgeBuilder::new().id("b1").source_id(&s).target_id(&t).build()).unwrap();
        BridgeStorage::delete(&conn, "b1").unwrap();
        assert!(BridgeStorage::get(&conn, "b1").unwrap().is_none());
    }

    #[test]
    fn test_delete_for_thread_bilateral() {
        let conn = setup_agent_db();
        let t3 = ThreadBuilder::new().id("t3").build();
        let (s, t) = setup_two_threads(&conn);
        ThreadStorage::insert(&conn, &t3).unwrap();
        // Bridge where t1 is source
        BridgeStorage::insert(&conn, &BridgeBuilder::new().id("b1").source_id(&s).target_id(&t).build()).unwrap();
        // Bridge where t1 is target
        BridgeStorage::insert(&conn, &BridgeBuilder::new().id("b2").source_id("t3").target_id(&s).build()).unwrap();
        // Both should be deleted
        let deleted = BridgeStorage::delete_for_thread(&conn, "t1").unwrap();
        assert_eq!(deleted, 2);
        assert_eq!(BridgeStorage::count(&conn).unwrap(), 0);
    }

    #[test]
    fn test_list_for_thread() {
        let conn = setup_agent_db();
        let t3 = ThreadBuilder::new().id("t3").build();
        let (s, t) = setup_two_threads(&conn);
        ThreadStorage::insert(&conn, &t3).unwrap();
        BridgeStorage::insert(&conn, &BridgeBuilder::new().id("b1").source_id(&s).target_id(&t).build()).unwrap();
        BridgeStorage::insert(&conn, &BridgeBuilder::new().id("b2").source_id(&s).target_id("t3").build()).unwrap();
        BridgeStorage::insert(&conn, &BridgeBuilder::new().id("b3").source_id(&t).target_id("t3").build()).unwrap();
        // t1 is in b1 (source) and b2 (source) = 2
        assert_eq!(BridgeStorage::list_for_thread(&conn, "t1").unwrap().len(), 2);
        // t3 is in b2 (target) and b3 (target) = 2
        assert_eq!(BridgeStorage::list_for_thread(&conn, "t3").unwrap().len(), 2);
    }

    #[test]
    fn test_list_active_and_by_status() {
        let conn = setup_agent_db();
        let (s, t) = setup_two_threads(&conn);
        BridgeStorage::insert(&conn, &BridgeBuilder::new().id("b1").source_id(&s).target_id(&t).build()).unwrap();
        BridgeStorage::insert(&conn, &BridgeBuilder::new().id("b2").source_id(&s).target_id(&t).status(BridgeStatus::Weak).build()).unwrap();
        assert_eq!(BridgeStorage::list_active(&conn).unwrap().len(), 1);
        assert_eq!(BridgeStorage::list_by_status(&conn, BridgeStatus::Weak).unwrap().len(), 1);
    }

    #[test]
    fn test_update_weight_and_status() {
        let conn = setup_agent_db();
        let (s, t) = setup_two_threads(&conn);
        BridgeStorage::insert(&conn, &BridgeBuilder::new().id("b1").source_id(&s).target_id(&t).build()).unwrap();
        BridgeStorage::update_weight(&conn, "b1", 0.3).unwrap();
        BridgeStorage::update_status(&conn, "b1", BridgeStatus::Weak).unwrap();
        let got = BridgeStorage::get(&conn, "b1").unwrap().unwrap();
        assert!((got.weight - 0.3).abs() < 0.001);
        assert_eq!(got.status, BridgeStatus::Weak);
    }

    #[test]
    fn test_increment_use() {
        let conn = setup_agent_db();
        let (s, t) = setup_two_threads(&conn);
        BridgeStorage::insert(&conn, &BridgeBuilder::new().id("b1").source_id(&s).target_id(&t).weight(0.5).build()).unwrap();
        BridgeStorage::increment_use(&conn, "b1").unwrap();
        let got = BridgeStorage::get(&conn, "b1").unwrap().unwrap();
        assert_eq!(got.use_count, 1);
        assert!(got.last_reinforced.is_some());
        // weight should remain unchanged — increment_use does NOT boost weight (known finding #2)
        assert!((got.weight - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_scan_orphans() {
        // Insert valid bridge, then delete target thread with FK off to create orphan
        let conn = setup_agent_db_no_fk();
        let t1 = ThreadBuilder::new().id("t1").build();
        let t2 = ThreadBuilder::new().id("t2").build();
        ThreadStorage::insert(&conn, &t1).unwrap();
        ThreadStorage::insert(&conn, &t2).unwrap();
        BridgeStorage::insert(&conn, &BridgeBuilder::new().id("b1").source_id("t1").target_id("t2").build()).unwrap();
        // Delete t2 directly — FK is off so bridge won't cascade
        conn.execute("DELETE FROM threads WHERE id = 't2'", []).unwrap();
        let orphans = BridgeStorage::scan_orphans(&conn).unwrap();
        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0].id, "b1");
    }

    #[test]
    fn test_delete_batch_and_count_by_status() {
        let conn = setup_agent_db();
        let (s, t) = setup_two_threads(&conn);
        for i in 0..3 {
            BridgeStorage::insert(&conn, &BridgeBuilder::new().id(&format!("b{}", i)).source_id(&s).target_id(&t).build()).unwrap();
        }
        let deleted = BridgeStorage::delete_batch(&conn, &["b0".into(), "b1".into()]).unwrap();
        assert_eq!(deleted, 2);
        assert_eq!(BridgeStorage::count(&conn).unwrap(), 1);
        assert_eq!(BridgeStorage::count_by_status(&conn, &BridgeStatus::Active).unwrap(), 1);
    }

    #[test]
    fn test_bridge_delete_batch() {
        let conn = setup_agent_db();
        let (s, t) = setup_two_threads(&conn);
        for i in 0..5 {
            BridgeStorage::insert(&conn, &BridgeBuilder::new().id(&format!("b{}", i)).source_id(&s).target_id(&t).build()).unwrap();
        }
        // Single query should delete all 3
        let deleted = BridgeStorage::delete_batch(&conn, &["b0".into(), "b2".into(), "b4".into()]).unwrap();
        assert_eq!(deleted, 3);
        assert_eq!(BridgeStorage::count(&conn).unwrap(), 2);
        // Empty input returns 0
        assert_eq!(BridgeStorage::delete_batch(&conn, &[]).unwrap(), 0);
    }
}
