use ai_smartness::{id_gen, time_utils};
use ai_smartness::message::{Message, MessagePriority, MessageStatus};
use ai_smartness::AiResult;
use ai_smartness::storage::cognitive_inbox::CognitiveInbox;
use ai_smartness::storage::mcp_messages::McpMessages;
use ai_smartness::storage::path_utils;
use ai_smartness::registry::registry::AgentRegistry;

use super::{optional_str, optional_usize, required_str, ToolContext};

/// Write a wake signal file so the VSCode extension can wake the target agent.
/// `mode`: "cognitive" or "inbox" — tells the extension which prompt to inject.
pub(crate) fn emit_wake_signal(target_agent: &str, from_agent: &str, subject: &str, mode: &str) {
    let signal_path = path_utils::wake_signal_path(target_agent);
    let signal = serde_json::json!({
        "agent_id": target_agent,
        "from": from_agent,
        "message": subject,
        "mode": mode,
        "timestamp": time_utils::now().to_rfc3339(),
        "acknowledged": false
    });
    // Best-effort: don't fail the message send if signal write fails
    let _ = std::fs::create_dir_all(path_utils::wake_signals_dir());
    let _ = std::fs::write(&signal_path, signal.to_string());
}

// ── Cognitive inbox (ai-smartness) ──

pub fn handle_msg_focus(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let target = required_str(params, "target_agent_id")?;
    let from = required_str(params, "from_agent")?;
    let subject = required_str(params, "subject")?;
    let content = required_str(params, "content")?;
    let priority_str = optional_str(params, "priority").unwrap_or_else(|| "normal".into());
    let ttl_minutes = optional_usize(params, "ttl_minutes").unwrap_or(1440);

    let priority: MessagePriority = priority_str.parse().unwrap_or(MessagePriority::Normal);
    let now = time_utils::now();
    let ttl = now + chrono::Duration::minutes(ttl_minutes as i64);

    let msg = Message {
        id: id_gen::message_id(),
        from_agent: from,
        to_agent: target.clone(),
        subject,
        content,
        priority,
        status: MessageStatus::Pending,
        created_at: now,
        ttl_expiry: ttl,
        read_at: None,
        acked_at: None,
    };

    CognitiveInbox::send(ctx.agent_conn, &msg)?;
    emit_wake_signal(&target, &msg.from_agent, &msg.subject, "cognitive");
    Ok(serde_json::json!({"sent": true, "message_id": msg.id, "target": target}))
}

pub fn handle_msg_ack(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let thread_id = optional_str(params, "thread_id");
    let msg_ref = optional_str(params, "msg_ref");

    let ack_id = thread_id
        .or(msg_ref)
        .ok_or_else(|| ai_smartness::AiError::InvalidInput("Need thread_id or msg_ref".into()))?;

    CognitiveInbox::ack(ctx.agent_conn, &ack_id)?;
    Ok(serde_json::json!({"acked": ack_id}))
}

// ── MCP messaging (mcp-smartness-com) ──

pub fn handle_msg_send(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let to = required_str(params, "to")?;
    let subject = required_str(params, "subject")?;
    let payload = optional_str(params, "payload").unwrap_or_default();
    let priority_str = optional_str(params, "priority").unwrap_or_else(|| "normal".into());
    let effective_agent = optional_str(params, "agent_id")
        .unwrap_or_else(|| ctx.agent_id.to_string());

    let priority: MessagePriority = priority_str.parse().unwrap_or(MessagePriority::Normal);
    let now = time_utils::now();

    let msg = Message {
        id: id_gen::message_id(),
        from_agent: effective_agent,
        to_agent: to.clone(),
        subject,
        content: payload,
        priority,
        status: MessageStatus::Pending,
        created_at: now,
        ttl_expiry: now + chrono::Duration::hours(24),
        read_at: None,
        acked_at: None,
    };

    McpMessages::send(ctx.shared_conn, &msg)?;
    emit_wake_signal(&to, &msg.from_agent, &msg.subject, "inbox");
    Ok(serde_json::json!({"sent": true, "message_id": msg.id}))
}

pub fn handle_msg_broadcast(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let subject = required_str(params, "subject")?;
    let payload = optional_str(params, "payload").unwrap_or_default();
    let priority_str = optional_str(params, "priority").unwrap_or_else(|| "normal".into());

    let priority: MessagePriority = priority_str.parse().unwrap_or(MessagePriority::Normal);
    let now = time_utils::now();

    let msg = Message {
        id: id_gen::message_id(),
        from_agent: ctx.agent_id.to_string(),
        to_agent: "*".to_string(),
        subject,
        content: payload,
        priority,
        status: MessageStatus::Pending,
        created_at: now,
        ttl_expiry: now + chrono::Duration::hours(24),
        read_at: None,
        acked_at: None,
    };

    McpMessages::broadcast(ctx.shared_conn, &msg)?;

    // Wake all agents in the project (except sender)
    if let Ok(agents) = AgentRegistry::list(ctx.registry_conn, Some(ctx.project_hash), None, None) {
        for agent in &agents {
            if agent.id != ctx.agent_id {
                emit_wake_signal(&agent.id, ctx.agent_id, &msg.subject, "inbox");
            }
        }
    }

    Ok(serde_json::json!({"broadcast": true, "message_id": msg.id}))
}

pub fn handle_msg_inbox(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let _limit = optional_usize(params, "limit").unwrap_or(10);
    let effective_agent = optional_str(params, "agent_id")
        .unwrap_or_else(|| ctx.agent_id.to_string());
    let messages = McpMessages::inbox(ctx.shared_conn, &effective_agent)?;

    let results: Vec<serde_json::Value> = messages
        .iter()
        .map(|m| {
            serde_json::json!({
                "id": m.id,
                "from": m.from_agent,
                "subject": m.subject,
                "content": m.content,
                "priority": m.priority.as_str(),
                "created_at": m.created_at.to_rfc3339(),
            })
        })
        .collect();

    // Mark all fetched messages as read so they don't accumulate
    for m in &messages {
        McpMessages::ack(ctx.shared_conn, &m.id).ok();
    }

    Ok(serde_json::json!({"messages": results, "count": results.len()}))
}

pub fn handle_msg_reply(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let message_id = required_str(params, "message_id")?;
    let effective_agent = optional_str(params, "agent_id")
        .unwrap_or_else(|| ctx.agent_id.to_string());
    let payload = params
        .get("payload")
        .map(|v| serde_json::to_string(v).unwrap_or_default())
        .unwrap_or_default();

    // Resolve recipient: look up who sent the original message
    let original_sender: String = ctx.shared_conn.query_row(
        "SELECT from_agent FROM mcp_messages WHERE id = ?1",
        rusqlite::params![message_id],
        |row| row.get(0),
    ).map_err(|_| ai_smartness::AiError::InvalidInput(format!("Original message {} not found", message_id)))?;

    let now = time_utils::now();
    let reply = Message {
        id: id_gen::message_id(),
        from_agent: effective_agent.clone(),
        to_agent: original_sender.clone(),
        subject: format!("Re: {}", message_id),
        content: payload,
        priority: MessagePriority::Normal,
        status: MessageStatus::Pending,
        created_at: now,
        ttl_expiry: now + chrono::Duration::hours(24),
        read_at: None,
        acked_at: None,
    };

    McpMessages::reply(ctx.shared_conn, &message_id, &reply)?;

    emit_wake_signal(&original_sender, &effective_agent, &reply.subject, "inbox");

    Ok(serde_json::json!({"replied": true, "reply_id": reply.id}))
}
