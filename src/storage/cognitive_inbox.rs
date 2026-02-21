use crate::time_utils;
use crate::message::{Attachment, Message, MessagePriority, MessageStatus};
use crate::{AiError, AiResult};
use rusqlite::{params, Connection, Row};

pub struct CognitiveInbox;

fn message_from_row(row: &Row) -> rusqlite::Result<Message> {
    let priority_str: String = row.get("priority")?;
    let status_str: String = row.get("status")?;
    let created_str: String = row.get("created_at")?;
    let ttl_str: Option<String> = row.get("ttl_expiry")?;
    let read_str: Option<String> = row.get("read_at")?;
    let acked_str: Option<String> = row.get("acked_at")?;

    // Backward-compatible: column may not exist pre-V3
    let attachments_str: Option<String> = row.get("attachments").unwrap_or(None);
    let attachments: Vec<Attachment> = attachments_str
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    Ok(Message {
        id: row.get("id")?,
        from_agent: row.get("from_agent")?,
        to_agent: row.get("to_agent")?,
        subject: row.get("subject")?,
        content: row.get("content")?,
        priority: priority_str
            .parse()
            .unwrap_or(MessagePriority::Normal),
        status: status_str
            .parse()
            .unwrap_or(MessageStatus::Pending),
        created_at: time_utils::from_sqlite(&created_str).unwrap_or_else(|_| chrono::Utc::now()),
        ttl_expiry: ttl_str
            .and_then(|s| time_utils::from_sqlite(&s).ok())
            .unwrap_or_else(|| chrono::Utc::now() + chrono::Duration::hours(24)),
        read_at: read_str.and_then(|s| time_utils::from_sqlite(&s).ok()),
        acked_at: acked_str.and_then(|s| time_utils::from_sqlite(&s).ok()),
        attachments,
    })
}

impl CognitiveInbox {
    /// Insert un message dans la cognitive inbox de l'agent
    pub fn send(conn: &Connection, msg: &Message) -> AiResult<()> {
        let attachments_json = serde_json::to_string(&msg.attachments)
            .unwrap_or_else(|_| "[]".to_string());

        conn.execute(
            "INSERT INTO cognitive_inbox (id, from_agent, to_agent, subject, content, priority, ttl_expiry, status, created_at, attachments)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                msg.id,
                msg.from_agent,
                msg.to_agent,
                msg.subject,
                msg.content,
                msg.priority.as_str(),
                time_utils::to_sqlite(&msg.ttl_expiry),
                msg.status.as_str(),
                time_utils::to_sqlite(&msg.created_at),
                attachments_json,
            ],
        )
        .map_err(|e| AiError::Storage(format!("Send cognitive message failed: {}", e)))?;
        tracing::info!(from = %msg.from_agent, to = %msg.to_agent, subject = %msg.subject, "Cognitive message sent");
        Ok(())
    }

    /// Peek at pending messages without marking them read.
    /// Used by inject hook on non-wake prompts to show content without consuming it.
    pub fn peek_pending(conn: &Connection, agent_id: &str) -> AiResult<Vec<Message>> {
        let mut stmt = conn
            .prepare(
                "SELECT * FROM cognitive_inbox
                 WHERE to_agent = ?1 AND status = 'pending'
                 ORDER BY created_at ASC",
            )
            .map_err(|e| AiError::Storage(e.to_string()))?;

        let messages: Vec<Message> = stmt
            .query_map(params![agent_id], message_from_row)
            .map_err(|e| AiError::Storage(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        tracing::debug!(agent = %agent_id, count = messages.len(), "Cognitive messages peeked (not consumed)");
        Ok(messages)
    }

    /// Lit les messages pending et les marque read (consomme)
    pub fn read_pending(conn: &Connection, agent_id: &str) -> AiResult<Vec<Message>> {
        let now = time_utils::to_sqlite(&time_utils::now());

        // Mark as read
        conn.execute(
            "UPDATE cognitive_inbox SET status = 'read', read_at = ?1
             WHERE to_agent = ?2 AND status = 'pending'",
            params![now, agent_id],
        )
        .map_err(|e| AiError::Storage(e.to_string()))?;

        // Return all read (just marked)
        let mut stmt = conn
            .prepare(
                "SELECT * FROM cognitive_inbox
                 WHERE to_agent = ?1 AND status = 'read' AND read_at = ?2
                 ORDER BY created_at ASC",
            )
            .map_err(|e| AiError::Storage(e.to_string()))?;

        let messages: Vec<Message> = stmt
            .query_map(params![agent_id, now], message_from_row)
            .map_err(|e| AiError::Storage(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        tracing::debug!(agent = %agent_id, count = messages.len(), "Cognitive messages read");

        Ok(messages)
    }

    /// Acquitte un message (marque acked_at)
    pub fn ack(conn: &Connection, msg_id: &str) -> AiResult<()> {
        let now = time_utils::to_sqlite(&time_utils::now());
        conn.execute(
            "UPDATE cognitive_inbox SET status = 'acked', acked_at = ?1 WHERE id = ?2",
            params![now, msg_id],
        )
        .map_err(|e| AiError::Storage(format!("Ack message failed: {}", e)))?;
        Ok(())
    }

    /// Deplace les messages expires vers dead_letters
    pub fn expire_stale(conn: &Connection) -> AiResult<usize> {
        let now = time_utils::to_sqlite(&time_utils::now());

        let tx = conn.unchecked_transaction()
            .map_err(|e| AiError::Storage(format!("Begin transaction failed: {}", e)))?;

        // Move expired to dead_letters (including attachments)
        let count: usize = tx
            .execute(
                "INSERT INTO dead_letters (id, from_agent, to_agent, subject, content, priority, original_ttl, expired_at, created_at, attachments)
                 SELECT id, from_agent, to_agent, subject, content, priority, ttl_expiry, ?1, created_at, attachments
                 FROM cognitive_inbox
                 WHERE ttl_expiry IS NOT NULL AND ttl_expiry < ?1 AND status != 'acked'",
                params![now],
            )
            .map_err(|e| AiError::Storage(e.to_string()))?;

        // Delete expired from inbox
        tx.execute(
            "DELETE FROM cognitive_inbox
             WHERE ttl_expiry IS NOT NULL AND ttl_expiry < ?1 AND status != 'acked'",
            params![now],
        )
        .map_err(|e| AiError::Storage(e.to_string()))?;

        tx.commit()
            .map_err(|e| AiError::Storage(format!("Commit failed: {}", e)))?;

        if count > 0 {
            tracing::debug!(expired_count = count, "Cognitive messages expired");
        }

        Ok(count)
    }

    /// Lit les dead letters (pour audit)
    pub fn list_dead_letters(conn: &Connection) -> AiResult<Vec<Message>> {
        let mut stmt = conn
            .prepare(
                "SELECT id, from_agent, to_agent, subject, content, priority,
                        original_ttl as ttl_expiry, 'expired' as status,
                        created_at, NULL as read_at, NULL as acked_at, attachments
                 FROM dead_letters ORDER BY expired_at DESC",
            )
            .map_err(|e| AiError::Storage(e.to_string()))?;

        let messages = stmt
            .query_map([], message_from_row)
            .map_err(|e| AiError::Storage(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(messages)
    }

    /// Compte les messages pending
    pub fn count_pending(conn: &Connection, agent_id: &str) -> AiResult<usize> {
        let c: usize = conn
            .query_row(
                "SELECT COUNT(*) FROM cognitive_inbox WHERE to_agent = ?1 AND status = 'pending'",
                params![agent_id],
                |r| r.get(0),
            )
            .map_err(|e| AiError::Storage(e.to_string()))?;
        Ok(c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::setup_agent_db;

    #[test]
    fn test_expire_stale_atomic() {
        let conn = setup_agent_db();
        let past = "2020-01-01T00:00:00Z";
        let now_str = time_utils::to_sqlite(&time_utils::now());

        // Insert a message with expired TTL
        conn.execute(
            "INSERT INTO cognitive_inbox (id, from_agent, to_agent, subject, content, priority, ttl_expiry, status, created_at, attachments)
             VALUES ('m1', 'agent-a', 'agent-b', 'test', 'hello', 'normal', ?1, 'pending', ?2, '[]')",
            params![past, now_str],
        ).unwrap();

        let expired = CognitiveInbox::expire_stale(&conn).unwrap();
        assert_eq!(expired, 1);

        // Message should be gone from inbox AND present in dead_letters (atomic)
        let inbox_count: usize = conn.query_row(
            "SELECT COUNT(*) FROM cognitive_inbox", [], |r| r.get(0),
        ).unwrap();
        assert_eq!(inbox_count, 0);

        let dead_count: usize = conn.query_row(
            "SELECT COUNT(*) FROM dead_letters", [], |r| r.get(0),
        ).unwrap();
        assert_eq!(dead_count, 1);
    }
}
