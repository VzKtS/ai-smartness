use crate::time_utils;
use crate::message::{Attachment, Message, MessagePriority, MessageStatus};
use crate::{AiError, AiResult};
use rusqlite::{params, Connection, Row};

/// Storage pour le mode MCP messaging (table dans shared.db)
pub struct McpMessages;

fn mcp_msg_from_row(row: &Row) -> rusqlite::Result<Message> {
    let priority_str: String = row.get("priority")?;
    let status_str: String = row.get("status")?;
    let created_str: String = row.get("created_at")?;
    let expires_str: Option<String> = row.get("expires_at")?;
    let read_str: Option<String> = row.get("read_at")?;

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
        content: {
            let payload: String = row.get("payload")?;
            payload
        },
        priority: priority_str
            .parse()
            .unwrap_or(MessagePriority::Normal),
        status: status_str
            .parse()
            .unwrap_or(MessageStatus::Pending),
        created_at: time_utils::from_sqlite(&created_str).unwrap_or_else(|_| chrono::Utc::now()),
        ttl_expiry: expires_str
            .and_then(|s| time_utils::from_sqlite(&s).ok())
            .unwrap_or_else(|| chrono::Utc::now() + chrono::Duration::hours(24)),
        read_at: read_str.and_then(|s| time_utils::from_sqlite(&s).ok()),
        acked_at: None,
        attachments,
    })
}

impl McpMessages {
    pub fn send(conn: &Connection, msg: &Message) -> AiResult<()> {
        let attachments_json = serde_json::to_string(&msg.attachments)
            .unwrap_or_else(|_| "[]".to_string());

        conn.execute(
            "INSERT INTO mcp_messages (id, from_agent, to_agent, msg_type, subject, payload, priority, status, created_at, expires_at, attachments)
             VALUES (?1, ?2, ?3, 'request', ?4, ?5, ?6, 'pending', ?7, ?8, ?9)",
            params![
                msg.id,
                msg.from_agent,
                msg.to_agent,
                msg.subject,
                msg.content,
                msg.priority.as_str(),
                time_utils::to_sqlite(&msg.created_at),
                time_utils::to_sqlite(&msg.ttl_expiry),
                attachments_json,
            ],
        )
        .map_err(|e| AiError::Storage(format!("Send MCP message failed: {}", e)))?;
        Ok(())
    }

    pub fn inbox(conn: &Connection, agent_id: &str) -> AiResult<Vec<Message>> {
        let mut stmt = conn
            .prepare(
                "SELECT * FROM mcp_messages
                 WHERE (to_agent = ?1 OR to_agent = '*') AND status = 'pending'
                 ORDER BY created_at ASC",
            )
            .map_err(|e| AiError::Storage(e.to_string()))?;

        let messages = stmt
            .query_map(params![agent_id], mcp_msg_from_row)
            .map_err(|e| AiError::Storage(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(messages)
    }

    pub fn reply(conn: &Connection, reply_to: &str, msg: &Message) -> AiResult<()> {
        let attachments_json = serde_json::to_string(&msg.attachments)
            .unwrap_or_else(|_| "[]".to_string());

        conn.execute(
            "INSERT INTO mcp_messages (id, from_agent, to_agent, msg_type, subject, payload, priority, status, reply_to, created_at, expires_at, attachments)
             VALUES (?1, ?2, ?3, 'response', ?4, ?5, ?6, 'pending', ?7, ?8, ?9, ?10)",
            params![
                msg.id,
                msg.from_agent,
                msg.to_agent,
                msg.subject,
                msg.content,
                msg.priority.as_str(),
                reply_to,
                time_utils::to_sqlite(&msg.created_at),
                time_utils::to_sqlite(&msg.ttl_expiry),
                attachments_json,
            ],
        )
        .map_err(|e| AiError::Storage(format!("Reply MCP message failed: {}", e)))?;
        Ok(())
    }

    pub fn broadcast(conn: &Connection, msg: &Message) -> AiResult<()> {
        let attachments_json = serde_json::to_string(&msg.attachments)
            .unwrap_or_else(|_| "[]".to_string());

        conn.execute(
            "INSERT INTO mcp_messages (id, from_agent, to_agent, msg_type, subject, payload, priority, status, created_at, expires_at, attachments)
             VALUES (?1, ?2, '*', 'broadcast', ?3, ?4, ?5, 'pending', ?6, ?7, ?8)",
            params![
                msg.id,
                msg.from_agent,
                msg.subject,
                msg.content,
                msg.priority.as_str(),
                time_utils::to_sqlite(&msg.created_at),
                time_utils::to_sqlite(&msg.ttl_expiry),
                attachments_json,
            ],
        )
        .map_err(|e| AiError::Storage(format!("Broadcast MCP message failed: {}", e)))?;
        Ok(())
    }

    pub fn ack(conn: &Connection, msg_id: &str) -> AiResult<()> {
        let now = time_utils::to_sqlite(&time_utils::now());
        conn.execute(
            "UPDATE mcp_messages SET status = 'read', read_at = ?1 WHERE id = ?2",
            params![now, msg_id],
        )
        .map_err(|e| AiError::Storage(format!("Ack MCP message failed: {}", e)))?;
        Ok(())
    }

    pub fn expire_stale(conn: &Connection) -> AiResult<usize> {
        let now = time_utils::to_sqlite(&time_utils::now());
        let count = conn
            .execute(
                "UPDATE mcp_messages SET status = 'expired'
                 WHERE expires_at IS NOT NULL AND expires_at < ?1 AND status = 'pending'",
                params![now],
            )
            .map_err(|e| AiError::Storage(e.to_string()))?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{Message, MessagePriority, MessageStatus};
    use crate::test_helpers::setup_shared_db;

    fn make_msg(id: &str, from: &str, to: &str) -> Message {
        let now = time_utils::now();
        Message {
            id: id.to_string(),
            from_agent: from.to_string(),
            to_agent: to.to_string(),
            subject: "test".to_string(),
            content: "hello".to_string(),
            priority: MessagePriority::Normal,
            status: MessageStatus::Pending,
            created_at: now,
            ttl_expiry: now + chrono::Duration::hours(24),
            read_at: None,
            acked_at: None,
            attachments: vec![],
        }
    }

    #[test]
    fn test_ack_sets_status_read() {
        let conn = setup_shared_db();
        let msg = make_msg("m1", "alice", "bob");
        McpMessages::send(&conn, &msg).unwrap();

        McpMessages::ack(&conn, "m1").unwrap();

        let status: String = conn.query_row(
            "SELECT status FROM mcp_messages WHERE id = 'm1'", [], |r| r.get(0),
        ).unwrap();
        assert_eq!(status, "read");

        let read_at: Option<String> = conn.query_row(
            "SELECT read_at FROM mcp_messages WHERE id = 'm1'", [], |r| r.get(0),
        ).unwrap();
        assert!(read_at.is_some(), "read_at should be set after ack");
    }

    #[test]
    fn test_inbox_returns_pending_only() {
        let conn = setup_shared_db();
        McpMessages::send(&conn, &make_msg("m1", "alice", "bob")).unwrap();
        McpMessages::send(&conn, &make_msg("m2", "alice", "bob")).unwrap();

        // Ack m1 → status='read'
        McpMessages::ack(&conn, "m1").unwrap();

        let inbox = McpMessages::inbox(&conn, "bob").unwrap();
        assert_eq!(inbox.len(), 1);
        assert_eq!(inbox[0].id, "m2");
    }

    #[test]
    fn test_inbox_includes_broadcast() {
        let conn = setup_shared_db();
        McpMessages::send(&conn, &make_msg("m1", "alice", "bob")).unwrap();
        McpMessages::broadcast(&conn, &make_msg("m2", "alice", "*")).unwrap();

        let inbox = McpMessages::inbox(&conn, "bob").unwrap();
        assert_eq!(inbox.len(), 2);
    }

    #[test]
    fn test_expire_stale_pending_only() {
        let conn = setup_shared_db();
        let past = "2020-01-01T00:00:00Z";
        let now_str = time_utils::to_sqlite(&time_utils::now());

        // Insert expired pending message
        conn.execute(
            "INSERT INTO mcp_messages (id, from_agent, to_agent, msg_type, subject, payload, priority, status, created_at, expires_at)
             VALUES ('m1', 'a', 'b', 'request', 's', 'p', 'normal', 'pending', ?1, ?2)",
            params![now_str, past],
        ).unwrap();

        // Insert expired but already read message (should NOT be expired again)
        conn.execute(
            "INSERT INTO mcp_messages (id, from_agent, to_agent, msg_type, subject, payload, priority, status, created_at, expires_at)
             VALUES ('m2', 'a', 'b', 'request', 's', 'p', 'normal', 'read', ?1, ?2)",
            params![now_str, past],
        ).unwrap();

        let count = McpMessages::expire_stale(&conn).unwrap();
        assert_eq!(count, 1, "Only pending messages should be expired");

        let s: String = conn.query_row(
            "SELECT status FROM mcp_messages WHERE id = 'm1'", [], |r| r.get(0),
        ).unwrap();
        assert_eq!(s, "expired");
    }
}
