use ai_smartness::{id_gen, time_utils};
use ai_smartness::thread::{OriginType, Thread, ThreadMessage, ThreadStatus};
use ai_smartness::AiResult;
use ai_smartness::storage::threads::ThreadStorage;

use super::{
    optional_bool, optional_f64, optional_str, optional_usize, required_array, required_str,
    ToolContext,
};

pub fn handle_thread_create(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let title = required_str(params, "title")?;
    let content = required_str(params, "content")?;
    let topics: Vec<String> = params
        .get("topics")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let importance = optional_f64(params, "importance").unwrap_or(0.5);
    let now = time_utils::now();
    let thread_id = id_gen::thread_id();

    let thread = Thread {
        id: thread_id.clone(),
        title: title.clone(),
        status: ThreadStatus::Active,
        summary: None,
        origin_type: OriginType::Prompt,
        parent_id: None,
        child_ids: vec![],
        weight: 0.5,
        importance,
        importance_manually_set: false,
        relevance_score: 1.0,
        activation_count: 1,
        split_locked: false,
        split_locked_until: None,
        topics,
        tags: vec![],
        labels: vec![],
        drift_history: vec![],
        ratings: vec![],
        work_context: None,
        injection_stats: None,
        embedding: None,
        created_at: now,
        last_active: now,
    };
    ThreadStorage::insert(ctx.agent_conn, &thread)?;

    let msg = ThreadMessage {
        thread_id: thread_id.clone(),
        msg_id: id_gen::message_id(),
        content,
        source: "manual".into(),
        source_type: "user".into(),
        timestamp: now,
        metadata: serde_json::json!({}),
    };
    ThreadStorage::add_message(ctx.agent_conn, &msg)?;

    Ok(serde_json::json!({
        "thread_id": thread_id,
        "title": title,
        "status": "active",
    }))
}

pub fn handle_thread_rm(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let thread_id = required_str(params, "thread_id")?;
    ThreadStorage::delete(ctx.agent_conn, &thread_id)?;
    Ok(serde_json::json!({"deleted": thread_id}))
}

pub fn handle_thread_rm_batch(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let ids = required_array(params, "thread_ids")?;
    let count = ThreadStorage::delete_batch(ctx.agent_conn, &ids)?;
    Ok(serde_json::json!({"deleted": count}))
}

pub fn handle_thread_list(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let status_str = optional_str(params, "status").unwrap_or_else(|| "active".into());
    let limit = optional_usize(params, "limit").unwrap_or(30);
    let offset = optional_usize(params, "offset").unwrap_or(0);
    let status: ThreadStatus = status_str.parse().unwrap_or(ThreadStatus::Active);

    let threads = ThreadStorage::list_by_status(ctx.agent_conn, &status)?;
    let total = threads.len();
    let page: Vec<serde_json::Value> = threads
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|t| thread_json(&t))
        .collect();

    Ok(serde_json::json!({"threads": page, "total": total, "offset": offset, "limit": limit}))
}

pub fn handle_thread_search(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let query = required_str(params, "query")?;
    let threads = ThreadStorage::search(ctx.agent_conn, &query)?;
    let results: Vec<serde_json::Value> = threads.iter().map(thread_json).collect();
    Ok(serde_json::json!({"threads": results, "count": results.len()}))
}

pub fn handle_thread_activate(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let ids = required_array(params, "thread_ids")?;
    let confirm = optional_bool(params, "confirm").unwrap_or(false);

    if !confirm {
        let preview: Vec<serde_json::Value> = ids
            .iter()
            .filter_map(|id| ThreadStorage::get(ctx.agent_conn, id).ok().flatten())
            .map(|t| {
                serde_json::json!({"id": t.id, "title": t.title, "current_status": t.status.as_str()})
            })
            .collect();
        return Ok(serde_json::json!({"dry_run": true, "threads": preview}));
    }

    let mut count = 0;
    for id in &ids {
        if let Ok(Some(t)) = ThreadStorage::get(ctx.agent_conn, id) {
            if t.status != ThreadStatus::Active {
                ThreadStorage::update_status(ctx.agent_conn, id, ThreadStatus::Active)?;
                if t.weight < 0.3 {
                    ThreadStorage::update_weight(ctx.agent_conn, id, 0.3)?;
                }
                count += 1;
            }
        }
    }
    Ok(serde_json::json!({"activated": count}))
}

pub fn handle_thread_suspend(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let ids = required_array(params, "thread_ids")?;
    let confirm = optional_bool(params, "confirm").unwrap_or(false);

    if !confirm {
        let preview: Vec<serde_json::Value> = ids
            .iter()
            .filter_map(|id| ThreadStorage::get(ctx.agent_conn, id).ok().flatten())
            .map(|t| {
                serde_json::json!({"id": t.id, "title": t.title, "current_status": t.status.as_str()})
            })
            .collect();
        return Ok(serde_json::json!({"dry_run": true, "threads": preview}));
    }

    let mut count = 0;
    for id in &ids {
        ThreadStorage::update_status(ctx.agent_conn, id, ThreadStatus::Suspended)?;
        count += 1;
    }
    Ok(serde_json::json!({"suspended": count}))
}

pub fn handle_reactivate(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let id = required_str(params, "thread_id")?;
    let t = ThreadStorage::get(ctx.agent_conn, &id)?
        .ok_or_else(|| ai_smartness::AiError::ThreadNotFound(id.clone()))?;
    ThreadStorage::update_status(ctx.agent_conn, &id, ThreadStatus::Active)?;
    if t.weight < 0.3 {
        ThreadStorage::update_weight(ctx.agent_conn, &id, 0.3)?;
    }
    Ok(serde_json::json!({"reactivated": id, "title": t.title}))
}

// ── Metadata handlers ──

pub fn handle_label(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let id = required_str(params, "thread_id")?;
    let labels = required_array(params, "labels")?;
    let mode = optional_str(params, "mode").unwrap_or_else(|| "add".into());

    let mut thread = ThreadStorage::get(ctx.agent_conn, &id)?
        .ok_or_else(|| ai_smartness::AiError::ThreadNotFound(id.clone()))?;

    match mode.as_str() {
        "set" => thread.labels = labels,
        "remove" => thread.labels.retain(|l| !labels.contains(l)),
        _ => {
            for l in labels {
                if !thread.labels.contains(&l) {
                    thread.labels.push(l);
                }
            }
        }
    }
    ThreadStorage::update(ctx.agent_conn, &thread)?;
    Ok(serde_json::json!({"thread_id": id, "labels": thread.labels}))
}

pub fn handle_labels_suggest(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let _proposed = required_str(params, "label")?;
    let threads = ThreadStorage::list_all(ctx.agent_conn)?;
    let mut freq: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for t in &threads {
        for l in &t.labels {
            *freq.entry(l.clone()).or_insert(0) += 1;
        }
    }
    let mut labels: Vec<(String, usize)> = freq.into_iter().collect();
    labels.sort_by(|a, b| b.1.cmp(&a.1));
    let suggestions: Vec<serde_json::Value> = labels
        .into_iter()
        .take(20)
        .map(|(l, c)| serde_json::json!({"label": l, "count": c}))
        .collect();
    Ok(serde_json::json!({"labels": suggestions}))
}

pub fn handle_rename(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let id = required_str(params, "thread_id")?;
    let title = required_str(params, "new_title")?;
    let mut thread = ThreadStorage::get(ctx.agent_conn, &id)?
        .ok_or_else(|| ai_smartness::AiError::ThreadNotFound(id.clone()))?;
    thread.title = title.clone();
    ThreadStorage::update(ctx.agent_conn, &thread)?;
    Ok(serde_json::json!({"thread_id": id, "new_title": title}))
}

pub fn handle_rename_batch(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let ops = params
        .get("operations")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ai_smartness::AiError::InvalidInput("Missing operations".into()))?;
    let mut renamed = 0;
    for op in ops {
        let id = op.get("thread_id").and_then(|v| v.as_str()).unwrap_or("");
        let title = op.get("new_title").and_then(|v| v.as_str()).unwrap_or("");
        if !id.is_empty() && !title.is_empty() {
            if let Ok(Some(mut t)) = ThreadStorage::get(ctx.agent_conn, id) {
                t.title = title.to_string();
                let _ = ThreadStorage::update(ctx.agent_conn, &t);
                renamed += 1;
            }
        }
    }
    Ok(serde_json::json!({"renamed": renamed}))
}

pub fn handle_rate_importance(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let id = required_str(params, "thread_id")?;
    let score = optional_f64(params, "score").unwrap_or(0.5).clamp(0.0, 1.0);
    ThreadStorage::update_importance(ctx.agent_conn, &id, score, true)?;
    let half_life_hours = 18.0 + (score * 150.0);
    Ok(serde_json::json!({"thread_id": id, "importance": score, "effective_half_life_hours": half_life_hours}))
}

pub fn handle_rate_context(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let id = required_str(params, "thread_id")?;
    let useful = optional_bool(params, "useful").unwrap_or(true);
    let mut thread = ThreadStorage::get(ctx.agent_conn, &id)?
        .ok_or_else(|| ai_smartness::AiError::ThreadNotFound(id.clone()))?;
    let delta = if useful { 0.1 } else { -0.15 };
    thread.relevance_score = (thread.relevance_score + delta).clamp(0.0, 1.0);
    ThreadStorage::update(ctx.agent_conn, &thread)?;
    Ok(serde_json::json!({"thread_id": id, "useful": useful, "relevance_score": thread.relevance_score}))
}

fn thread_json(t: &ai_smartness::thread::Thread) -> serde_json::Value {
    serde_json::json!({
        "id": t.id,
        "title": t.title,
        "status": t.status.as_str(),
        "weight": t.weight,
        "importance": t.importance,
        "topics": t.topics,
        "labels": t.labels,
        "last_active": t.last_active.to_rfc3339(),
    })
}
