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
                topics, tags, labels, drift_history,
                work_context, ratings, injection_stats, embedding,
                created_at, last_active
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7,
                ?8, ?9, ?10, ?11,
                ?12, ?13, ?14,
                ?15, ?16, ?17, ?18,
                ?19, ?20, ?21, ?22,
                ?23, ?24
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
                topics = ?15, tags = ?16, labels = ?17, drift_history = ?18,
                work_context = ?19, ratings = ?20, injection_stats = ?21,
                embedding = ?22, last_active = ?23
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
        let pattern = format!("%{}%", query);
        let mut stmt = conn
            .prepare(
                "SELECT * FROM threads
                 WHERE title LIKE ?1 OR topics LIKE ?1 OR labels LIKE ?1 OR summary LIKE ?1
                 ORDER BY weight DESC",
            )
            .map_err(|e| AiError::Storage(e.to_string()))?;

        let threads = stmt
            .query_map(params![pattern], thread_from_row)
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
                .prepare("SELECT * FROM threads WHERE topics LIKE ?1 AND status = 'active'")
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
                .prepare("SELECT * FROM threads WHERE labels LIKE ?1 AND status = 'active'")
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
        conn.execute(
            "INSERT INTO thread_messages (id, thread_id, content, source, source_type, timestamp, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                msg.msg_id,
                msg.thread_id,
                msg.content,
                msg.source,
                msg.source_type,
                time_utils::to_sqlite(&msg.timestamp),
                serde_json::to_string(&msg.metadata).unwrap_or_else(|_| "{}".into()),
            ],
        )
        .map_err(|e| AiError::Storage(format!("Insert message failed: {}", e)))?;
        tracing::debug!(thread_id = %msg.thread_id, msg_id = %msg.msg_id, "Message added");

        // Update thread last_active
        let now = time_utils::to_sqlite(&time_utils::now());
        conn.execute(
            "UPDATE threads SET last_active = ?1, activation_count = activation_count + 1 WHERE id = ?2",
            params![now, msg.thread_id],
        )
        .map_err(|e| AiError::Storage(format!("Update thread last_active failed: {}", e)))?;

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
