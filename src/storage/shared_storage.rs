use crate::time_utils;
use crate::shared::{SharedThread, SharedVisibility, Subscription};
use crate::{AiError, AiResult};
use rusqlite::{params, Connection, Row};

pub struct SharedStorage;

fn shared_from_row(row: &Row) -> rusqlite::Result<SharedThread> {
    let vis_str: String = row.get("visibility")?;
    let published_str: String = row.get("published_at")?;
    let updated_str: Option<String> = row.get("updated_at")?;
    let topics_json: String = row.get("topics")?;
    let allowed_json: String = row.get("allowed_agents")?;

    Ok(SharedThread {
        shared_id: row.get("shared_id")?,
        thread_id: row.get("source_thread_id")?,
        owner_agent: row.get("owner_agent")?,
        title: row.get("title")?,
        topics: serde_json::from_str(&topics_json).unwrap_or_default(),
        visibility: match vis_str.as_str() {
            "restricted" => SharedVisibility::Restricted,
            _ => SharedVisibility::Network,
        },
        allowed_agents: serde_json::from_str(&allowed_json).unwrap_or_default(),
        published_at: time_utils::from_sqlite(&published_str)
            .unwrap_or_else(|_| chrono::Utc::now()),
        updated_at: updated_str.and_then(|s| time_utils::from_sqlite(&s).ok()),
    })
}

fn sub_from_row(row: &Row) -> rusqlite::Result<Subscription> {
    let sub_str: String = row.get("subscribed_at")?;
    let sync_str: Option<String> = row.get("last_synced")?;

    Ok(Subscription {
        shared_id: row.get("shared_id")?,
        subscriber_agent: row.get("subscriber_agent")?,
        subscribed_at: time_utils::from_sqlite(&sub_str).unwrap_or_else(|_| chrono::Utc::now()),
        last_synced: sync_str.and_then(|s| time_utils::from_sqlite(&s).ok()),
    })
}

impl SharedStorage {
    pub fn publish(conn: &Connection, shared: &SharedThread) -> AiResult<()> {
        let now = time_utils::to_sqlite(&time_utils::now());
        conn.execute(
            "INSERT OR REPLACE INTO shared_threads
                (shared_id, source_thread_id, owner_agent, title, topics, visibility, allowed_agents, published_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                shared.shared_id,
                shared.thread_id,
                shared.owner_agent,
                shared.title,
                serde_json::to_string(&shared.topics).unwrap_or_else(|_| "[]".into()),
                match shared.visibility {
                    SharedVisibility::Restricted => "restricted",
                    SharedVisibility::Network => "network",
                },
                serde_json::to_string(&shared.allowed_agents).unwrap_or_else(|_| "[]".into()),
                now,
                now,
            ],
        )
        .map_err(|e| AiError::Storage(format!("Publish shared thread failed: {}", e)))?;
        Ok(())
    }

    pub fn unpublish(conn: &Connection, shared_id: &str) -> AiResult<()> {
        conn.execute(
            "DELETE FROM shared_threads WHERE shared_id = ?1",
            params![shared_id],
        )
        .map_err(|e| AiError::Storage(format!("Unpublish failed: {}", e)))?;
        Ok(())
    }

    pub fn subscribe(conn: &Connection, sub: &Subscription) -> AiResult<()> {
        // Validate that the shared thread exists before attempting subscription.
        // This prevents FK constraint violations when subscribing to non-existent shared_ids.
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM shared_threads WHERE shared_id = ?1",
                params![&sub.shared_id],
                |r| r.get(0),
            )
            .map_err(|e| AiError::Storage(format!("Failed to check shared thread existence: {}", e)))?;

        if !exists {
            return Err(AiError::InvalidInput(format!(
                "Cannot subscribe: shared thread '{}' does not exist",
                sub.shared_id
            )));
        }

        let now = time_utils::to_sqlite(&time_utils::now());
        conn.execute(
            "INSERT OR REPLACE INTO subscriptions (id, shared_id, subscriber_agent, subscribed_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                crate::id_gen::message_id(),
                sub.shared_id,
                sub.subscriber_agent,
                now,
            ],
        )
        .map_err(|e| AiError::Storage(format!("Subscribe failed: {}", e)))?;
        Ok(())
    }

    pub fn unsubscribe(conn: &Connection, shared_id: &str, agent_id: &str) -> AiResult<()> {
        conn.execute(
            "DELETE FROM subscriptions WHERE shared_id = ?1 AND subscriber_agent = ?2",
            params![shared_id, agent_id],
        )
        .map_err(|e| AiError::Storage(format!("Unsubscribe failed: {}", e)))?;
        Ok(())
    }

    pub fn list_published(conn: &Connection) -> AiResult<Vec<SharedThread>> {
        let mut stmt = conn
            .prepare("SELECT * FROM shared_threads ORDER BY published_at DESC")
            .map_err(|e| AiError::Storage(e.to_string()))?;

        let threads = stmt
            .query_map([], shared_from_row)
            .map_err(|e| AiError::Storage(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(threads)
    }

    pub fn list_subscriptions(
        conn: &Connection,
        agent_id: &str,
    ) -> AiResult<Vec<Subscription>> {
        let mut stmt = conn
            .prepare("SELECT * FROM subscriptions WHERE subscriber_agent = ?1")
            .map_err(|e| AiError::Storage(e.to_string()))?;

        let subs = stmt
            .query_map(params![agent_id], sub_from_row)
            .map_err(|e| AiError::Storage(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(subs)
    }

    pub fn discover(conn: &Connection, topics: &[String], agent_id: Option<&str>) -> AiResult<Vec<SharedThread>> {
        if topics.is_empty() {
            // Empty-topics path: list all published, optionally filtered by owner
            if let Some(id) = agent_id {
                let mut stmt = conn
                    .prepare("SELECT * FROM shared_threads WHERE owner_agent = ?1 ORDER BY published_at DESC")
                    .map_err(|e| AiError::Storage(e.to_string()))?;
                let threads = stmt
                    .query_map(params![id], shared_from_row)
                    .map_err(|e| AiError::Storage(e.to_string()))?
                    .filter_map(|r| r.ok())
                    .collect();
                return Ok(threads);
            }
            return Self::list_published(conn);
        }

        let mut conditions = Vec::new();
        let mut params_list: Vec<String> = Vec::new();
        for topic in topics {
            let idx = params_list.len() + 1;
            params_list.push(format!("%\"{}\"%", topic));
            conditions.push(format!("LOWER(topics) LIKE ?{idx}"));
        }

        // agent_id filter applies in both paths
        let agent_clause = if let Some(id) = agent_id {
            let idx = params_list.len() + 1;
            let clause = format!(" AND owner_agent = ?{idx}");
            params_list.push(id.to_string());
            clause
        } else {
            String::new()
        };

        let sql = format!(
            "SELECT * FROM shared_threads WHERE ({}) AND visibility = 'network'{}",
            conditions.join(" OR "),
            agent_clause
        );

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| AiError::Storage(e.to_string()))?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = params_list
            .iter()
            .map(|s| s as &dyn rusqlite::types::ToSql)
            .collect();
        let threads: Vec<SharedThread> = stmt
            .query_map(param_refs.as_slice(), shared_from_row)
            .map_err(|e| AiError::Storage(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(threads)
    }

    pub fn get(conn: &Connection, shared_id: &str) -> AiResult<Option<SharedThread>> {
        let mut stmt = conn
            .prepare("SELECT * FROM shared_threads WHERE shared_id = ?1")
            .map_err(|e| AiError::Storage(e.to_string()))?;

        let result = stmt
            .query_row(params![shared_id], shared_from_row)
            .optional()
            .map_err(|e| AiError::Storage(e.to_string()))?;

        Ok(result)
    }

    /// Count shared entries for a given source thread.
    pub fn count_by_thread_id(conn: &Connection, thread_id: &str) -> AiResult<usize> {
        let c: usize = conn.query_row(
            "SELECT COUNT(*) FROM shared_threads WHERE source_thread_id = ?1",
            params![thread_id],
            |r| r.get(0),
        )
        .map_err(|e| AiError::Storage(e.to_string()))?;
        Ok(c)
    }

    pub fn update_sync(conn: &Connection, shared_id: &str, agent_id: &str) -> AiResult<()> {
        let now = time_utils::to_sqlite(&time_utils::now());
        conn.execute(
            "UPDATE subscriptions SET last_synced = ?1 WHERE shared_id = ?2 AND subscriber_agent = ?3",
            params![now, shared_id, agent_id],
        )
        .map_err(|e| AiError::Storage(format!("Update sync failed: {}", e)))?;
        Ok(())
    }
}

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
    use crate::test_helpers::setup_shared_db;
    use crate::time_utils;

    fn insert_shared(conn: &Connection, id: &str, topics: &str, visibility: &str) {
        insert_shared_owned(conn, id, topics, visibility, "agent-a");
    }

    fn insert_shared_owned(conn: &Connection, id: &str, topics: &str, visibility: &str, owner: &str) {
        let now = time_utils::to_sqlite(&time_utils::now());
        conn.execute(
            "INSERT INTO shared_threads (shared_id, source_thread_id, owner_agent, title, topics, visibility, published_at, updated_at)
             VALUES (?1, ?2, ?3, 'test', ?4, ?5, ?6, ?6)",
            params![id, format!("src-{id}"), owner, topics, visibility, now],
        ).unwrap();
    }

    #[test]
    fn test_discover_batched() {
        let conn = setup_shared_db();
        insert_shared(&conn, "s1", "[\"rust\", \"sqlite\"]", "network");
        insert_shared(&conn, "s2", "[\"python\"]", "network");
        insert_shared(&conn, "s3", "[\"rust\"]", "private");

        let results = SharedStorage::discover(&conn, &["rust".into(), "python".into()], None).unwrap();
        // s1 and s2 match topics + network, s3 is private so excluded
        assert_eq!(results.len(), 2);
        let ids: Vec<&str> = results.iter().map(|r| r.shared_id.as_str()).collect();
        assert!(ids.contains(&"s1"));
        assert!(ids.contains(&"s2"));
    }

    #[test]
    fn test_subscribe_validates_shared_thread_exists() {
        let conn = setup_shared_db();
        insert_shared(&conn, "s1", "[\"test\"]", "network");

        // Subscription to existing shared thread should succeed
        let sub = Subscription {
            shared_id: "s1".to_string(),
            subscriber_agent: "agent-a".to_string(),
            subscribed_at: time_utils::now(),
            last_synced: None,
        };
        let result = SharedStorage::subscribe(&conn, &sub);
        assert!(result.is_ok(), "Should succeed for existing shared thread");

        // Subscription to non-existent shared thread should fail (FK constraint validation)
        let bad_sub = Subscription {
            shared_id: "s999".to_string(),
            subscriber_agent: "agent-b".to_string(),
            subscribed_at: time_utils::now(),
            last_synced: None,
        };
        let result = SharedStorage::subscribe(&conn, &bad_sub);
        assert!(result.is_err(), "Should fail for non-existent shared thread");
        assert!(
            result.unwrap_err().to_string().contains("does not exist"),
            "Error should mention non-existent shared thread"
        );
    }

    // T-B4: discover filters by owner_agent when agent_id = Some
    #[test]
    fn test_discover_filters_by_agent() {
        let conn = setup_shared_db();
        insert_shared_owned(&conn, "s1", "[\"rust\"]", "network", "agent-a");
        insert_shared_owned(&conn, "s2", "[\"rust\"]", "network", "agent-b");

        // Filter by agent-a → only s1
        let results = SharedStorage::discover(&conn, &[], Some("agent-a")).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].shared_id, "s1");

        // No filter → both
        let results_all = SharedStorage::discover(&conn, &[], None).unwrap();
        assert_eq!(results_all.len(), 2);
    }

    #[test]
    fn test_batch_subscribe_to_valid_threads() {
        let conn = setup_shared_db();
        // Create 10 shared threads
        for i in 1..=10 {
            insert_shared(&conn, &format!("s{}", i), "[\"test\"]", "network");
        }

        // Subscribe agent to all 10 threads — should all succeed
        for i in 1..=10 {
            let sub = Subscription {
                shared_id: format!("s{}", i),
                subscriber_agent: "agent-x".to_string(),
                subscribed_at: time_utils::now(),
                last_synced: None,
            };
            let result = SharedStorage::subscribe(&conn, &sub);
            assert!(
                result.is_ok(),
                "Subscription to s{} failed: {:?}",
                i,
                result.err()
            );
        }

        // Verify all subscriptions were recorded
        let subs = SharedStorage::list_subscriptions(&conn, "agent-x").unwrap();
        assert_eq!(
            subs.len(),
            10,
            "Should have exactly 10 subscriptions, got {}",
            subs.len()
        );
    }
}
