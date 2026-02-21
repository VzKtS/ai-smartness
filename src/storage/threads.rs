use crate::time_utils;
use crate::thread::{
    InjectionStats, OriginType, Thread, ThreadMessage, ThreadStatus, WorkContext,
};
use crate::{AiError, AiResult};
use rusqlite::{params, Connection, Row};

pub struct ThreadStorage;

// ── Row mapping ──

fn thread_from_row(row: &Row) -> rusqlite::Result<Thread> {
    let status_str: String = row.get("status")?;
    let origin_str: String = row.get("origin_type")?;
    let created_str: String = row.get("created_at")?;
    let last_active_str: String = row.get("last_active")?;
    let child_ids_json: String = row.get("child_ids")?;
    let topics_json: String = row.get("topics")?;
    let tags_json: String = row.get("tags")?;
    let labels_json: String = row.get("labels")?;
    let concepts_json: String = row.get("concepts")?;
    let drift_json: String = row.get("drift_history")?;
    let ratings_json: String = row.get("ratings")?;
    let work_context_json: Option<String> = row.get("work_context")?;
    let injection_stats_json: Option<String> = row.get("injection_stats")?;
    let embedding_blob: Option<Vec<u8>> = row.get("embedding")?;
    let split_locked_until_str: Option<String> = row.get("split_locked_until")?;

    Ok(Thread {
        id: row.get("id")?,
        title: row.get("title")?,
        status: status_str
            .parse()
            .unwrap_or(ThreadStatus::Active),
        summary: row.get("summary")?,
        origin_type: origin_str
            .parse()
            .unwrap_or(OriginType::Prompt),
        parent_id: row.get("parent_id")?,
        child_ids: serde_json::from_str(&child_ids_json).unwrap_or_default(),
        weight: row.get("weight")?,
        importance: row.get("importance")?,
        importance_manually_set: row.get::<_, i32>("importance_manually_set")? != 0,
        relevance_score: row.get("relevance_score")?,
        activation_count: row.get::<_, u32>("activation_count")?,
        split_locked: row.get::<_, i32>("split_locked")? != 0,
        split_locked_until: split_locked_until_str
            .and_then(|s| time_utils::from_sqlite(&s).ok()),
        topics: serde_json::from_str(&topics_json).unwrap_or_default(),
        tags: serde_json::from_str(&tags_json).unwrap_or_default(),
        labels: serde_json::from_str(&labels_json).unwrap_or_default(),
        concepts: serde_json::from_str(&concepts_json).unwrap_or_default(),
        drift_history: serde_json::from_str(&drift_json).unwrap_or_default(),
        ratings: serde_json::from_str(&ratings_json).unwrap_or_default(),
        work_context: work_context_json
            .and_then(|s| serde_json::from_str::<WorkContext>(&s).ok()),
        injection_stats: injection_stats_json
            .and_then(|s| serde_json::from_str::<InjectionStats>(&s).ok()),
        embedding: embedding_blob.map(|blob| {
            blob.chunks_exact(4)
                .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect()
        }),
        created_at: time_utils::from_sqlite(&created_str).unwrap_or_else(|_| chrono::Utc::now()),
        last_active: time_utils::from_sqlite(&last_active_str)
            .unwrap_or_else(|_| chrono::Utc::now()),
    })
}

fn message_from_row(row: &Row) -> rusqlite::Result<ThreadMessage> {
    let ts_str: String = row.get("timestamp")?;
    Ok(ThreadMessage {
        thread_id: row.get("thread_id")?,
        msg_id: row.get("id")?,
        content: row.get("content")?,
        source: row.get("source")?,
        source_type: row.get("source_type")?,
        timestamp: time_utils::from_sqlite(&ts_str).unwrap_or_else(|_| chrono::Utc::now()),
        metadata: {
            let s: String = row.get("metadata")?;
            serde_json::from_str(&s).unwrap_or(serde_json::Value::Object(Default::default()))
        },
        is_truncated: row.get("is_truncated").unwrap_or(false),
    })
}

// ── CRUD ──

impl ThreadStorage {
    pub fn insert(conn: &Connection, thread: &Thread) -> AiResult<()> {
        let embedding_blob = thread.embedding.as_ref().map(|v| {
            v.iter()
                .flat_map(|f| f.to_le_bytes())
                .collect::<Vec<u8>>()
        });

        conn.execute(
            "INSERT INTO threads (
                id, title, status, summary, origin_type, parent_id, child_ids,
                weight, importance, importance_manually_set, relevance_score,
                activation_count, split_locked, split_locked_until,
                topics, tags, labels, concepts, drift_history,
                work_context, ratings, injection_stats, embedding,
                created_at, last_active
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7,
                ?8, ?9, ?10, ?11,
                ?12, ?13, ?14,
                ?15, ?16, ?17, ?18, ?19,
                ?20, ?21, ?22, ?23,
                ?24, ?25
            )",
            params![
                thread.id,
                thread.title,
                thread.status.as_str(),
                thread.summary,
                thread.origin_type.as_str(),
                thread.parent_id,
                serde_json::to_string(&thread.child_ids).unwrap_or_else(|_| "[]".into()),
                thread.weight,
                thread.importance,
                thread.importance_manually_set as i32,
                thread.relevance_score,
                thread.activation_count,
                thread.split_locked as i32,
                thread
                    .split_locked_until
                    .map(|dt| time_utils::to_sqlite(&dt)),
                serde_json::to_string(&thread.topics).unwrap_or_else(|_| "[]".into()),
                serde_json::to_string(&thread.tags).unwrap_or_else(|_| "[]".into()),
                serde_json::to_string(&thread.labels).unwrap_or_else(|_| "[]".into()),
                serde_json::to_string(&thread.concepts).unwrap_or_else(|_| "[]".into()),
                serde_json::to_string(&thread.drift_history).unwrap_or_else(|_| "[]".into()),
                thread
                    .work_context
                    .as_ref()
                    .and_then(|wc| serde_json::to_string(wc).ok()),
                serde_json::to_string(&thread.ratings).unwrap_or_else(|_| "[]".into()),
                thread
                    .injection_stats
                    .as_ref()
                    .and_then(|is| serde_json::to_string(is).ok()),
                embedding_blob,
                time_utils::to_sqlite(&thread.created_at),
                time_utils::to_sqlite(&thread.last_active),
            ],
        )
        .map_err(|e| AiError::Storage(format!("Insert thread failed: {}", e)))?;
        tracing::debug!(thread_id = %thread.id, "Thread inserted");
        Ok(())
    }

    pub fn get(conn: &Connection, id: &str) -> AiResult<Option<Thread>> {
        let mut stmt = conn
            .prepare("SELECT * FROM threads WHERE id = ?1")
            .map_err(|e| AiError::Storage(e.to_string()))?;

        let result = stmt
            .query_row(params![id], thread_from_row)
            .optional()
            .map_err(|e| AiError::Storage(e.to_string()))?;

        Ok(result)
    }

    pub fn update(conn: &Connection, thread: &Thread) -> AiResult<()> {
        let embedding_blob = thread.embedding.as_ref().map(|v| {
            v.iter()
                .flat_map(|f| f.to_le_bytes())
                .collect::<Vec<u8>>()
        });

        conn.execute(
            "UPDATE threads SET
                title = ?2, status = ?3, summary = ?4, origin_type = ?5,
                parent_id = ?6, child_ids = ?7,
                weight = ?8, importance = ?9, importance_manually_set = ?10,
                relevance_score = ?11, activation_count = ?12,
                split_locked = ?13, split_locked_until = ?14,
                topics = ?15, tags = ?16, labels = ?17, concepts = ?18,
                drift_history = ?19,
                work_context = ?20, ratings = ?21, injection_stats = ?22,
                embedding = ?23, last_active = ?24
            WHERE id = ?1",
            params![
                thread.id,
                thread.title,
                thread.status.as_str(),
                thread.summary,
                thread.origin_type.as_str(),
                thread.parent_id,
                serde_json::to_string(&thread.child_ids).unwrap_or_else(|_| "[]".into()),
                thread.weight,
                thread.importance,
                thread.importance_manually_set as i32,
                thread.relevance_score,
                thread.activation_count,
                thread.split_locked as i32,
                thread
                    .split_locked_until
                    .map(|dt| time_utils::to_sqlite(&dt)),
                serde_json::to_string(&thread.topics).unwrap_or_else(|_| "[]".into()),
                serde_json::to_string(&thread.tags).unwrap_or_else(|_| "[]".into()),
                serde_json::to_string(&thread.labels).unwrap_or_else(|_| "[]".into()),
                serde_json::to_string(&thread.concepts).unwrap_or_else(|_| "[]".into()),
                serde_json::to_string(&thread.drift_history).unwrap_or_else(|_| "[]".into()),
                thread
                    .work_context
                    .as_ref()
                    .and_then(|wc| serde_json::to_string(wc).ok()),
                serde_json::to_string(&thread.ratings).unwrap_or_else(|_| "[]".into()),
                thread
                    .injection_stats
                    .as_ref()
                    .and_then(|is| serde_json::to_string(is).ok()),
                embedding_blob,
                time_utils::to_sqlite(&thread.last_active),
            ],
        )
        .map_err(|e| AiError::Storage(format!("Update thread failed: {}", e)))?;
        Ok(())
    }

    pub fn delete(conn: &Connection, id: &str) -> AiResult<()> {
        conn.execute("DELETE FROM threads WHERE id = ?1", params![id])
            .map_err(|e| AiError::Storage(format!("Delete thread failed: {}", e)))?;
        Ok(())
    }

    pub fn list_active(conn: &Connection) -> AiResult<Vec<Thread>> {
        Self::list_by_status(conn, &ThreadStatus::Active)
    }

    pub fn list_by_status(conn: &Connection, status: &ThreadStatus) -> AiResult<Vec<Thread>> {
        let mut stmt = conn
            .prepare("SELECT * FROM threads WHERE status = ?1 ORDER BY weight DESC")
            .map_err(|e| AiError::Storage(e.to_string()))?;

        let threads = stmt
            .query_map(params![status.as_str()], thread_from_row)
            .map_err(|e| AiError::Storage(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(threads)
    }

    pub fn list_all(conn: &Connection) -> AiResult<Vec<Thread>> {
        let mut stmt = conn
            .prepare("SELECT * FROM threads ORDER BY weight DESC")
            .map_err(|e| AiError::Storage(e.to_string()))?;

        let threads = stmt
            .query_map([], thread_from_row)
            .map_err(|e| AiError::Storage(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(threads)
    }

    pub fn search(conn: &Connection, query: &str) -> AiResult<Vec<Thread>> {
        // Tokenise query into words — search each word individually.
        // For JSON fields (topics, labels), use %"word"% to match inside arrays.
        // For plain-text fields (title, summary), use %word%.
        let words: Vec<&str> = query
            .split_whitespace()
            .filter(|w| w.len() >= 2)
            .collect();

        if words.is_empty() {
            return Ok(Vec::new());
        }

        // Build SQL: for each word, check title/summary (plain) + topics/labels (JSON)
        let mut conditions = Vec::new();
        let mut params_list: Vec<String> = Vec::new();

        for word in &words {
            let lower = word.to_lowercase();
            let idx_plain = params_list.len() + 1;
            params_list.push(format!("%{}%", lower));
            let idx_json = params_list.len() + 1;
            params_list.push(format!("%\"{}\"%", lower)); // matches inside JSON arrays

            conditions.push(format!(
                "(LOWER(title) LIKE ?{idx_plain} OR LOWER(summary) LIKE ?{idx_plain} \
                 OR LOWER(topics) LIKE ?{idx_json} OR LOWER(labels) LIKE ?{idx_json} \
                 OR LOWER(concepts) LIKE ?{idx_json})"
            ));
        }

        let sql = format!(
            "SELECT * FROM threads WHERE {} ORDER BY weight DESC",
            conditions.join(" OR ")
        );

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| AiError::Storage(e.to_string()))?;

        let param_refs: Vec<&dyn rusqlite::types::ToSql> = params_list
            .iter()
            .map(|s| s as &dyn rusqlite::types::ToSql)
            .collect();

        let threads: Vec<Thread> = stmt
            .query_map(param_refs.as_slice(), thread_from_row)
            .map_err(|e| AiError::Storage(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(threads)
    }

    pub fn count(conn: &Connection) -> AiResult<usize> {
        let c: usize = conn
            .query_row("SELECT COUNT(*) FROM threads", [], |r| r.get(0))
            .map_err(|e| AiError::Storage(e.to_string()))?;
        Ok(c)
    }

    pub fn count_by_status(conn: &Connection, status: &ThreadStatus) -> AiResult<usize> {
        let c: usize = conn
            .query_row(
                "SELECT COUNT(*) FROM threads WHERE status = ?1",
                params![status.as_str()],
                |r| r.get(0),
            )
            .map_err(|e| AiError::Storage(e.to_string()))?;
        Ok(c)
    }

    pub fn update_status(conn: &Connection, id: &str, status: ThreadStatus) -> AiResult<()> {
        let now = time_utils::to_sqlite(&time_utils::now());
        conn.execute(
            "UPDATE threads SET status = ?1, last_active = ?2 WHERE id = ?3",
            params![status.as_str(), now, id],
        )
        .map_err(|e| AiError::Storage(format!("Update status failed: {}", e)))?;
        tracing::debug!(thread_id = %id, status = %status.as_str(), "Thread status updated");
        Ok(())
    }

    pub fn update_weight(conn: &Connection, id: &str, weight: f64) -> AiResult<()> {
        conn.execute(
            "UPDATE threads SET weight = ?1 WHERE id = ?2",
            params![weight, id],
        )
        .map_err(|e| AiError::Storage(format!("Update weight failed: {}", e)))?;
        Ok(())
    }

    pub fn update_importance(
        conn: &Connection,
        id: &str,
        importance: f64,
        manually_set: bool,
    ) -> AiResult<()> {
        conn.execute(
            "UPDATE threads SET importance = ?1, importance_manually_set = ?2 WHERE id = ?3",
            params![importance, manually_set as i32, id],
        )
        .map_err(|e| AiError::Storage(format!("Update importance failed: {}", e)))?;
        Ok(())
    }

    pub fn update_embedding(conn: &Connection, id: &str, embedding: &[f32]) -> AiResult<()> {
        let blob: Vec<u8> = embedding
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();
        conn.execute(
            "UPDATE threads SET embedding = ?1 WHERE id = ?2",
            params![blob, id],
        )
        .map_err(|e| AiError::Storage(format!("Update embedding failed: {}", e)))?;
        Ok(())
    }

    pub fn search_by_topics(conn: &Connection, topics: &[String]) -> AiResult<Vec<Thread>> {
        // Use LIKE for each topic in the JSON array
        let mut all = Vec::new();
        for topic in topics {
            let pattern = format!("%\"{}\"%" , topic);
            let mut stmt = conn
                .prepare("SELECT * FROM threads WHERE topics LIKE ?1")
                .map_err(|e| AiError::Storage(e.to_string()))?;
            let threads: Vec<Thread> = stmt
                .query_map(params![pattern], thread_from_row)
                .map_err(|e| AiError::Storage(e.to_string()))?
                .filter_map(|r| r.ok())
                .collect();
            for t in threads {
                if !all.iter().any(|existing: &Thread| existing.id == t.id) {
                    all.push(t);
                }
            }
        }
        Ok(all)
    }

    pub fn search_by_labels(conn: &Connection, labels: &[String]) -> AiResult<Vec<Thread>> {
        let mut all = Vec::new();
        for label in labels {
            let pattern = format!("%\"{}\"%" , label);
            let mut stmt = conn
                .prepare("SELECT * FROM threads WHERE labels LIKE ?1")
                .map_err(|e| AiError::Storage(e.to_string()))?;
            let threads: Vec<Thread> = stmt
                .query_map(params![pattern], thread_from_row)
                .map_err(|e| AiError::Storage(e.to_string()))?
                .filter_map(|r| r.ok())
                .collect();
            for t in threads {
                if !all.iter().any(|existing: &Thread| existing.id == t.id) {
                    all.push(t);
                }
            }
        }
        Ok(all)
    }

    /// List all distinct labels across active threads.
    pub fn list_all_labels(conn: &Connection) -> AiResult<Vec<String>> {
        let mut stmt = conn
            .prepare("SELECT labels FROM threads WHERE status = 'active' AND labels != '[]'")
            .map_err(|e| AiError::Storage(e.to_string()))?;

        let mut label_set = std::collections::HashSet::new();
        let rows: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| AiError::Storage(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        for json_str in &rows {
            if let Ok(labels) = serde_json::from_str::<Vec<String>>(json_str) {
                for label in labels {
                    label_set.insert(label);
                }
            }
        }

        let mut labels: Vec<String> = label_set.into_iter().collect();
        labels.sort();
        Ok(labels)
    }

    /// List all distinct topics across active threads.
    pub fn list_all_topics(conn: &Connection) -> AiResult<Vec<String>> {
        let mut stmt = conn
            .prepare("SELECT topics FROM threads WHERE status = 'active' AND topics != '[]'")
            .map_err(|e| AiError::Storage(e.to_string()))?;

        let mut topic_set = std::collections::HashSet::new();
        let rows: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| AiError::Storage(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        for json_str in &rows {
            if let Ok(topics) = serde_json::from_str::<Vec<String>>(json_str) {
                for topic in topics {
                    topic_set.insert(topic);
                }
            }
        }

        let mut topics: Vec<String> = topic_set.into_iter().collect();
        topics.sort();
        Ok(topics)
    }

    // ── Messages ──

    pub fn message_count(conn: &Connection, thread_id: &str) -> AiResult<usize> {
        let c: usize = conn
            .query_row(
                "SELECT COUNT(*) FROM thread_messages WHERE thread_id = ?1",
                params![thread_id],
                |r| r.get(0),
            )
            .map_err(|e| AiError::Storage(e.to_string()))?;
        Ok(c)
    }

    pub fn add_message(conn: &Connection, msg: &ThreadMessage) -> AiResult<()> {
        let tx = conn.unchecked_transaction()
            .map_err(|e| AiError::Storage(format!("Begin transaction failed: {}", e)))?;

        tx.execute(
            "INSERT INTO thread_messages (id, thread_id, content, source, source_type, timestamp, metadata, is_truncated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                msg.msg_id,
                msg.thread_id,
                msg.content,
                msg.source,
                msg.source_type,
                time_utils::to_sqlite(&msg.timestamp),
                serde_json::to_string(&msg.metadata).unwrap_or_else(|_| "{}".into()),
                msg.is_truncated,
            ],
        )
        .map_err(|e| AiError::Storage(format!("Insert message failed: {}", e)))?;

        // Update thread last_active
        let now = time_utils::to_sqlite(&time_utils::now());
        tx.execute(
            "UPDATE threads SET last_active = ?1, activation_count = activation_count + 1 WHERE id = ?2",
            params![now, msg.thread_id],
        )
        .map_err(|e| AiError::Storage(format!("Update thread last_active failed: {}", e)))?;

        tx.commit()
            .map_err(|e| AiError::Storage(format!("Commit failed: {}", e)))?;

        tracing::debug!(thread_id = %msg.thread_id, msg_id = %msg.msg_id, "Message added");
        Ok(())
    }

    pub fn get_messages(conn: &Connection, thread_id: &str) -> AiResult<Vec<ThreadMessage>> {
        let mut stmt = conn
            .prepare(
                "SELECT * FROM thread_messages WHERE thread_id = ?1 ORDER BY timestamp ASC",
            )
            .map_err(|e| AiError::Storage(e.to_string()))?;

        let messages = stmt
            .query_map(params![thread_id], message_from_row)
            .map_err(|e| AiError::Storage(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(messages)
    }

    pub fn delete_messages(conn: &Connection, thread_id: &str) -> AiResult<()> {
        conn.execute(
            "DELETE FROM thread_messages WHERE thread_id = ?1",
            params![thread_id],
        )
        .map_err(|e| AiError::Storage(format!("Delete messages failed: {}", e)))?;
        Ok(())
    }

    // ── Batch operations ──

    pub fn delete_batch(conn: &Connection, ids: &[String]) -> AiResult<usize> {
        let mut count = 0;
        for id in ids {
            let affected = conn
                .execute("DELETE FROM threads WHERE id = ?1", params![id])
                .map_err(|e| AiError::Storage(e.to_string()))?;
            count += affected;
        }
        Ok(count)
    }

    /// Bulk delete all threads with a given status. Also removes their messages.
    /// Returns the number of threads deleted.
    pub fn delete_by_status(conn: &Connection, status: &ThreadStatus) -> AiResult<usize> {
        let tx = conn.unchecked_transaction()
            .map_err(|e| AiError::Storage(format!("Begin transaction failed: {}", e)))?;

        // Delete messages for threads with this status first
        tx.execute(
            "DELETE FROM thread_messages WHERE thread_id IN (SELECT id FROM threads WHERE status = ?1)",
            params![status.as_str()],
        )
        .map_err(|e| AiError::Storage(format!("Delete messages by status failed: {}", e)))?;
        // Delete the threads
        let deleted = tx
            .execute(
                "DELETE FROM threads WHERE status = ?1",
                params![status.as_str()],
            )
            .map_err(|e| AiError::Storage(format!("Delete threads by status failed: {}", e)))?;

        tx.commit()
            .map_err(|e| AiError::Storage(format!("Commit failed: {}", e)))?;
        Ok(deleted)
    }

    pub fn update_status_batch(
        conn: &Connection,
        ids: &[String],
        status: ThreadStatus,
    ) -> AiResult<usize> {
        let now = time_utils::to_sqlite(&time_utils::now());
        let mut count = 0;
        for id in ids {
            let affected = conn
                .execute(
                    "UPDATE threads SET status = ?1, last_active = ?2 WHERE id = ?3",
                    params![status.as_str(), now, id],
                )
                .map_err(|e| AiError::Storage(e.to_string()))?;
            count += affected;
        }
        Ok(count)
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
    use crate::test_helpers::{setup_agent_db, ThreadBuilder, ThreadMessageBuilder};

    #[test]
    fn test_insert_and_get() {
        let conn = setup_agent_db();
        let thread = ThreadBuilder::new().id("t1").title("Rust testing").build();
        ThreadStorage::insert(&conn, &thread).unwrap();
        let got = ThreadStorage::get(&conn, "t1").unwrap().unwrap();
        assert_eq!(got.id, "t1");
        assert_eq!(got.title, "Rust testing");
        assert_eq!(got.status, ThreadStatus::Active);
    }

    #[test]
    fn test_get_nonexistent() {
        let conn = setup_agent_db();
        let got = ThreadStorage::get(&conn, "nonexistent").unwrap();
        assert!(got.is_none());
    }

    #[test]
    fn test_update() {
        let conn = setup_agent_db();
        let mut thread = ThreadBuilder::new().id("t1").title("Original").weight(1.0).build();
        ThreadStorage::insert(&conn, &thread).unwrap();
        thread.title = "Updated".to_string();
        thread.weight = 0.5;
        ThreadStorage::update(&conn, &thread).unwrap();
        let got = ThreadStorage::get(&conn, "t1").unwrap().unwrap();
        assert_eq!(got.title, "Updated");
        assert!((got.weight - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_delete() {
        let conn = setup_agent_db();
        let thread = ThreadBuilder::new().id("t1").build();
        ThreadStorage::insert(&conn, &thread).unwrap();
        ThreadStorage::delete(&conn, "t1").unwrap();
        assert!(ThreadStorage::get(&conn, "t1").unwrap().is_none());
    }

    #[test]
    fn test_list_active_and_by_status() {
        let conn = setup_agent_db();
        ThreadStorage::insert(&conn, &ThreadBuilder::new().id("a1").build()).unwrap();
        ThreadStorage::insert(&conn, &ThreadBuilder::new().id("a2").build()).unwrap();
        ThreadStorage::insert(&conn, &ThreadBuilder::new().id("s1").status(ThreadStatus::Suspended).build()).unwrap();
        assert_eq!(ThreadStorage::list_active(&conn).unwrap().len(), 2);
        assert_eq!(ThreadStorage::list_by_status(&conn, &ThreadStatus::Suspended).unwrap().len(), 1);
    }

    #[test]
    fn test_count_and_count_by_status() {
        let conn = setup_agent_db();
        ThreadStorage::insert(&conn, &ThreadBuilder::new().id("a1").build()).unwrap();
        ThreadStorage::insert(&conn, &ThreadBuilder::new().id("a2").build()).unwrap();
        ThreadStorage::insert(&conn, &ThreadBuilder::new().id("s1").status(ThreadStatus::Suspended).build()).unwrap();
        assert_eq!(ThreadStorage::count(&conn).unwrap(), 3);
        assert_eq!(ThreadStorage::count_by_status(&conn, &ThreadStatus::Active).unwrap(), 2);
    }

    #[test]
    fn test_update_status() {
        let conn = setup_agent_db();
        ThreadStorage::insert(&conn, &ThreadBuilder::new().id("t1").build()).unwrap();
        ThreadStorage::update_status(&conn, "t1", ThreadStatus::Suspended).unwrap();
        let got = ThreadStorage::get(&conn, "t1").unwrap().unwrap();
        assert_eq!(got.status, ThreadStatus::Suspended);
    }

    #[test]
    fn test_update_weight() {
        let conn = setup_agent_db();
        ThreadStorage::insert(&conn, &ThreadBuilder::new().id("t1").weight(1.0).build()).unwrap();
        ThreadStorage::update_weight(&conn, "t1", 0.3).unwrap();
        let got = ThreadStorage::get(&conn, "t1").unwrap().unwrap();
        assert!((got.weight - 0.3).abs() < 0.001);
    }

    #[test]
    fn test_update_embedding_blob_roundtrip() {
        let conn = setup_agent_db();
        ThreadStorage::insert(&conn, &ThreadBuilder::new().id("t1").build()).unwrap();
        let embedding = vec![1.0f32, 2.0, 3.0, -0.5];
        ThreadStorage::update_embedding(&conn, "t1", &embedding).unwrap();
        let got = ThreadStorage::get(&conn, "t1").unwrap().unwrap();
        let got_emb = got.embedding.unwrap();
        assert_eq!(got_emb.len(), 4);
        assert!((got_emb[0] - 1.0).abs() < 0.001);
        assert!((got_emb[3] - (-0.5)).abs() < 0.001);
    }

    #[test]
    fn test_search_by_title() {
        let conn = setup_agent_db();
        ThreadStorage::insert(&conn, &ThreadBuilder::new().id("t1").title("Rust programming guide").build()).unwrap();
        ThreadStorage::insert(&conn, &ThreadBuilder::new().id("t2").title("Python basics").build()).unwrap();
        let results = ThreadStorage::search(&conn, "rust").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "t1");
    }

    #[test]
    fn test_search_by_topics() {
        let conn = setup_agent_db();
        ThreadStorage::insert(&conn, &ThreadBuilder::new().id("t1").title("Some title").topics(vec!["database", "sqlite"]).build()).unwrap();
        let results = ThreadStorage::search(&conn, "sqlite").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "t1");
    }

    #[test]
    fn test_messages_crud_and_last_active_update() {
        let conn = setup_agent_db();
        let thread = ThreadBuilder::new().id("t1").build();
        ThreadStorage::insert(&conn, &thread).unwrap();
        let original = ThreadStorage::get(&conn, "t1").unwrap().unwrap();
        let original_count = original.activation_count;

        let msg = ThreadMessageBuilder::new("t1").content("Hello world").build();
        ThreadStorage::add_message(&conn, &msg).unwrap();

        assert_eq!(ThreadStorage::message_count(&conn, "t1").unwrap(), 1);
        let messages = ThreadStorage::get_messages(&conn, "t1").unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "Hello world");

        // Verify add_message updated last_active + activation_count (pub finding)
        let updated = ThreadStorage::get(&conn, "t1").unwrap().unwrap();
        assert_eq!(updated.activation_count, original_count + 1);

        ThreadStorage::delete_messages(&conn, "t1").unwrap();
        assert_eq!(ThreadStorage::message_count(&conn, "t1").unwrap(), 0);
    }

    #[test]
    fn test_delete_batch_and_update_status_batch() {
        let conn = setup_agent_db();
        for i in 0..5 {
            ThreadStorage::insert(&conn, &ThreadBuilder::new().id(&format!("t{}", i)).build()).unwrap();
        }
        // delete_batch
        let deleted = ThreadStorage::delete_batch(&conn, &["t0".into(), "t1".into()]).unwrap();
        assert_eq!(deleted, 2);
        assert_eq!(ThreadStorage::count(&conn).unwrap(), 3);

        // update_status_batch
        let updated = ThreadStorage::update_status_batch(&conn, &["t2".into(), "t3".into()], ThreadStatus::Archived).unwrap();
        assert_eq!(updated, 2);
        assert_eq!(ThreadStorage::count_by_status(&conn, &ThreadStatus::Archived).unwrap(), 2);
    }

    #[test]
    fn test_add_message_atomic() {
        let conn = setup_agent_db();
        let thread = ThreadBuilder::new().id("t1").build();
        ThreadStorage::insert(&conn, &thread).unwrap();

        let msg = ThreadMessageBuilder::new("t1").content("atomic msg").build();
        ThreadStorage::add_message(&conn, &msg).unwrap();

        // Both the message insert AND last_active update should have committed
        assert_eq!(ThreadStorage::message_count(&conn, "t1").unwrap(), 1);
        let updated = ThreadStorage::get(&conn, "t1").unwrap().unwrap();
        assert_eq!(updated.activation_count, thread.activation_count + 1);
    }

    #[test]
    fn test_delete_by_status_atomic() {
        let conn = setup_agent_db();
        ThreadStorage::insert(&conn, &ThreadBuilder::new().id("a1").status(ThreadStatus::Archived).build()).unwrap();
        ThreadStorage::insert(&conn, &ThreadBuilder::new().id("a2").status(ThreadStatus::Archived).build()).unwrap();
        ThreadStorage::insert(&conn, &ThreadBuilder::new().id("k1").build()).unwrap();

        // Add messages to archived threads
        ThreadStorage::add_message(&conn, &ThreadMessageBuilder::new("a1").content("m1").build()).unwrap();
        ThreadStorage::add_message(&conn, &ThreadMessageBuilder::new("a2").content("m2").build()).unwrap();

        let deleted = ThreadStorage::delete_by_status(&conn, &ThreadStatus::Archived).unwrap();
        assert_eq!(deleted, 2);

        // Both threads AND their messages should be gone atomically
        assert_eq!(ThreadStorage::count(&conn).unwrap(), 1);
        assert_eq!(ThreadStorage::message_count(&conn, "a1").unwrap(), 0);
        assert_eq!(ThreadStorage::message_count(&conn, "a2").unwrap(), 0);
        // Active thread untouched
        assert!(ThreadStorage::get(&conn, "k1").unwrap().is_some());
    }
}
