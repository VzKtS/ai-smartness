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

    pub fn discover(conn: &Connection, topics: &[String]) -> AiResult<Vec<SharedThread>> {
        if topics.is_empty() {
            return Self::list_published(conn);
        }

        let mut all = Vec::new();
        for topic in topics {
            let pattern = format!("%\"{}\"%" , topic);
            let mut stmt = conn
                .prepare(
                    "SELECT * FROM shared_threads WHERE topics LIKE ?1 AND visibility = 'network'",
                )
                .map_err(|e| AiError::Storage(e.to_string()))?;
            let threads: Vec<SharedThread> = stmt
                .query_map(params![pattern], shared_from_row)
                .map_err(|e| AiError::Storage(e.to_string()))?
                .filter_map(|r| r.ok())
                .collect();
            for t in threads {
                if !all.iter().any(|e: &SharedThread| e.shared_id == t.shared_id) {
                    all.push(t);
                }
            }
        }
        Ok(all)
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
