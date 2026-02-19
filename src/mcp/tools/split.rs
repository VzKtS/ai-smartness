use ai_smartness::{id_gen, time_utils};
use ai_smartness::thread::{OriginType, Thread, ThreadStatus};
use ai_smartness::AiResult;
use ai_smartness::storage::threads::ThreadStorage;

use super::{optional_bool, required_str, ToolContext};

pub fn handle_split(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let thread_id = required_str(params, "thread_id")?;
    let confirm = optional_bool(params, "confirm").unwrap_or(false);

    let thread = ThreadStorage::get(ctx.agent_conn, &thread_id)?
        .ok_or_else(|| ai_smartness::AiError::ThreadNotFound(thread_id.clone()))?;

    if !confirm {
        let messages = ThreadStorage::get_messages(ctx.agent_conn, &thread_id)?;
        let msg_list: Vec<serde_json::Value> = messages
            .iter()
            .map(|m| {
                let preview = if m.content.len() > 200 {
                    format!("{}...", &m.content[..200])
                } else {
                    m.content.clone()
                };
                serde_json::json!({
                    "id": m.msg_id,
                    "content": preview,
                    "source": m.source,
                    "timestamp": m.timestamp.to_rfc3339(),
                })
            })
            .collect();
        return Ok(serde_json::json!({
            "thread_id": thread_id,
            "title": thread.title,
            "messages": msg_list,
            "instruction": "Provide message_groups and titles, call with confirm=true",
        }));
    }

    let message_groups = params
        .get("message_groups")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ai_smartness::AiError::InvalidInput("Missing message_groups".into()))?;
    let titles = params
        .get("titles")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ai_smartness::AiError::InvalidInput("Missing titles".into()))?;

    if message_groups.len() != titles.len() {
        return Err(ai_smartness::AiError::InvalidInput(
            "message_groups and titles must have same length".into(),
        ));
    }

    let all_messages = ThreadStorage::get_messages(ctx.agent_conn, &thread_id)?;
    let mut new_threads = Vec::new();
    let now = time_utils::now();

    for (i, _group) in message_groups.iter().enumerate() {
        let title = titles
            .get(i)
            .and_then(|v| v.as_str())
            .unwrap_or("Split thread");
        let new_id = id_gen::thread_id();

        let new_thread = Thread {
            id: new_id.clone(),
            title: title.to_string(),
            status: ThreadStatus::Active,
            summary: None,
            origin_type: OriginType::Split,
            parent_id: Some(thread_id.clone()),
            child_ids: vec![],
            weight: thread.weight * 0.8,
            importance: thread.importance,
            importance_manually_set: false,
            relevance_score: thread.relevance_score,
            activation_count: 1,
            split_locked: false,
            split_locked_until: None,
            topics: thread.topics.clone(),
            tags: vec![],
            labels: thread.labels.clone(),
            drift_history: vec![],
            ratings: vec![],
            work_context: None,
            injection_stats: None,
            embedding: None,
            created_at: now,
            last_active: now,
        };
        ThreadStorage::insert(ctx.agent_conn, &new_thread)?;

        if let Some(msg_ids) = message_groups.get(i).and_then(|v| v.as_array()) {
            for mid_val in msg_ids {
                if let Some(mid) = mid_val.as_str() {
                    if let Some(msg) = all_messages.iter().find(|m| m.msg_id == mid) {
                        let mut new_msg = msg.clone();
                        new_msg.thread_id = new_id.clone();
                        let _ = ThreadStorage::add_message(ctx.agent_conn, &new_msg);
                    }
                }
            }
        }

        new_threads.push(serde_json::json!({"thread_id": new_id, "title": title}));
    }

    // Lock original
    let mut locked = thread;
    locked.split_locked = true;
    ThreadStorage::update(ctx.agent_conn, &locked)?;

    Ok(serde_json::json!({
        "original_thread_id": thread_id,
        "new_threads": new_threads,
    }))
}

pub fn handle_split_unlock(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let id = required_str(params, "thread_id")?;
    let mut thread = ThreadStorage::get(ctx.agent_conn, &id)?
        .ok_or_else(|| ai_smartness::AiError::ThreadNotFound(id.clone()))?;
    thread.split_locked = false;
    thread.split_locked_until = None;
    ThreadStorage::update(ctx.agent_conn, &thread)?;
    Ok(serde_json::json!({"thread_id": id, "split_locked": false}))
}
