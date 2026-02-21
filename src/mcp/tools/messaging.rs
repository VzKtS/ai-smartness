use std::path::Path;

use ai_smartness::{id_gen, time_utils};
use ai_smartness::constants::{MAX_ATTACHMENT_SIZE_BYTES, MAX_ATTACHMENTS_PER_MESSAGE, MAX_TOTAL_ATTACHMENT_BYTES};
use ai_smartness::message::{Attachment, Message, MessagePriority, MessageStatus};
use ai_smartness::{AiError, AiResult};
use ai_smartness::storage::cognitive_inbox::CognitiveInbox;
use ai_smartness::storage::database::{self, ConnectionRole};
use ai_smartness::storage::mcp_messages::McpMessages;
use ai_smartness::storage::migrations;
use ai_smartness::storage::path_utils;
use ai_smartness::registry::registry::AgentRegistry;

use super::{optional_array, optional_str, optional_usize, required_str, ToolContext};

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

/// Resolve file paths into inlined attachments.
/// Graceful: skips files that fail (not found, binary, too large) with warnings.
/// Returns (valid_attachments, warning_lines).
fn resolve_attachments(paths: &[String]) -> AiResult<(Vec<Attachment>, Vec<String>)> {
    if paths.len() > MAX_ATTACHMENTS_PER_MESSAGE {
        return Err(AiError::InvalidInput(format!(
            "Too many attachments: {} (max {})",
            paths.len(), MAX_ATTACHMENTS_PER_MESSAGE
        )));
    }

    let mut attachments = Vec::with_capacity(paths.len());
    let mut warnings = Vec::new();
    let mut total_bytes: usize = 0;

    for path_str in paths {
        let path = Path::new(path_str);

        if !path.exists() {
            warnings.push(format!("[Attachment skipped: {} — file not found]", path_str));
            continue;
        }
        if !path.is_file() {
            warnings.push(format!("[Attachment skipped: {} — not a file]", path_str));
            continue;
        }

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                warnings.push(format!("[Attachment skipped: {} — {}]", path_str, e));
                continue;
            }
        };

        let size = content.len();

        if size > MAX_ATTACHMENT_SIZE_BYTES {
            warnings.push(format!(
                "[Attachment skipped: {} — too large ({} bytes, max {})]",
                path_str, size, MAX_ATTACHMENT_SIZE_BYTES
            ));
            continue;
        }

        if total_bytes + size > MAX_TOTAL_ATTACHMENT_BYTES {
            warnings.push(format!(
                "[Attachment skipped: {} — total size limit exceeded ({})]",
                path_str, MAX_TOTAL_ATTACHMENT_BYTES
            ));
            continue;
        }

        total_bytes += size;

        let filename = path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path_str.clone());

        attachments.push(Attachment {
            filename,
            content,
            original_size: size,
        });
    }

    Ok((attachments, warnings))
}

// ── Cognitive inbox (ai-smartness) ──

pub fn handle_msg_focus(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let target = required_str(params, "target_agent_id")?;
    let from = required_str(params, "from_agent")?;
    let subject = required_str(params, "subject")?;
    let mut content = required_str(params, "content")?;
    let priority_str = optional_str(params, "priority").unwrap_or_else(|| "normal".into());
    let ttl_minutes = optional_usize(params, "ttl_minutes").unwrap_or(1440);

    // Resolve file attachments
    let attachment_paths = optional_array(params, "attachments").unwrap_or_default();
    let (attachments, warnings) = if attachment_paths.is_empty() {
        (vec![], vec![])
    } else {
        resolve_attachments(&attachment_paths)?
    };
    // Append warnings to content so receiver sees skipped files
    for w in &warnings {
        content.push('\n');
        content.push_str(w);
    }

    let priority: MessagePriority = priority_str.parse().unwrap_or(MessagePriority::Normal);
    let now = time_utils::now();
    let ttl = now + chrono::Duration::minutes(ttl_minutes as i64);

    let att_count = attachments.len();
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
        attachments,
    };

    // Write to the TARGET agent's DB so the receiver's inject hook sees it
    let target_db = path_utils::agent_db_path(ctx.project_hash, &target);
    let target_conn = database::open_connection(&target_db, ConnectionRole::Mcp)?;
    migrations::migrate_agent_db(&target_conn)?;
    CognitiveInbox::send(&target_conn, &msg)?;

    emit_wake_signal(&target, &msg.from_agent, &msg.subject, "cognitive");
    Ok(serde_json::json!({"sent": true, "message_id": msg.id, "target": target, "attachments": att_count}))
}

pub fn handle_msg_ack(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let thread_id = optional_str(params, "thread_id");
    let msg_ref = optional_str(params, "msg_ref");

    let ack_id = match thread_id.or(msg_ref) {
        Some(id) => id,
        None => {
            CognitiveInbox::ack_latest(ctx.agent_conn, ctx.agent_id)?
                .ok_or_else(|| ai_smartness::AiError::InvalidInput("No unacked messages found".into()))?
        }
    };

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
    let mut payload = optional_str(params, "payload").unwrap_or_default();
    let priority_str = optional_str(params, "priority").unwrap_or_else(|| "normal".into());
    let effective_agent = optional_str(params, "agent_id")
        .unwrap_or_else(|| ctx.agent_id.to_string());

    // Resolve file attachments
    let attachment_paths = optional_array(params, "attachments").unwrap_or_default();
    let (attachments, warnings) = if attachment_paths.is_empty() {
        (vec![], vec![])
    } else {
        resolve_attachments(&attachment_paths)?
    };
    for w in &warnings {
        payload.push('\n');
        payload.push_str(w);
    }

    let priority: MessagePriority = priority_str.parse().unwrap_or(MessagePriority::Normal);
    let now = time_utils::now();

    let att_count = attachments.len();
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
        attachments,
    };

    McpMessages::send(ctx.shared_conn, &msg)?;
    emit_wake_signal(&to, &msg.from_agent, &msg.subject, "inbox");
    Ok(serde_json::json!({"sent": true, "message_id": msg.id, "attachments": att_count}))
}

pub fn handle_msg_broadcast(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let subject = required_str(params, "subject")?;
    let mut payload = optional_str(params, "payload").unwrap_or_default();
    let priority_str = optional_str(params, "priority").unwrap_or_else(|| "normal".into());

    // Resolve file attachments
    let attachment_paths = optional_array(params, "attachments").unwrap_or_default();
    let (attachments, warnings) = if attachment_paths.is_empty() {
        (vec![], vec![])
    } else {
        resolve_attachments(&attachment_paths)?
    };
    for w in &warnings {
        payload.push('\n');
        payload.push_str(w);
    }

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
        attachments,
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
            let mut obj = serde_json::json!({
                "id": m.id,
                "from": m.from_agent,
                "subject": m.subject,
                "content": m.content,
                "priority": m.priority.as_str(),
                "created_at": m.created_at.to_rfc3339(),
            });
            if !m.attachments.is_empty() {
                obj["attachments"] = serde_json::json!(
                    m.attachments.iter().map(|a| {
                        serde_json::json!({
                            "filename": a.filename,
                            "content": a.content,
                            "size": a.original_size,
                        })
                    }).collect::<Vec<_>>()
                );
            }
            obj
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
    let mut payload = params
        .get("payload")
        .map(|v| serde_json::to_string(v).unwrap_or_default())
        .unwrap_or_default();

    // Resolve recipient: look up who sent the original message
    let original_sender: String = ctx.shared_conn.query_row(
        "SELECT from_agent FROM mcp_messages WHERE id = ?1",
        rusqlite::params![message_id],
        |row| row.get(0),
    ).map_err(|_| ai_smartness::AiError::InvalidInput(format!("Original message {} not found", message_id)))?;

    // Resolve file attachments for reply
    let attachment_paths = optional_array(params, "attachments").unwrap_or_default();
    let (attachments, warnings) = if attachment_paths.is_empty() {
        (vec![], vec![])
    } else {
        resolve_attachments(&attachment_paths)?
    };
    for w in &warnings {
        payload.push('\n');
        payload.push_str(w);
    }

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
        attachments,
    };

    McpMessages::reply(ctx.shared_conn, &message_id, &reply)?;

    emit_wake_signal(&original_sender, &effective_agent, &reply.subject, "inbox");

    Ok(serde_json::json!({"replied": true, "reply_id": reply.id}))
}
