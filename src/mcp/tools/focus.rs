use ai_smartness::{id_gen, time_utils};
use ai_smartness::thread::{OriginType, Thread, ThreadMessage, ThreadStatus};
use ai_smartness::AiResult;
use ai_smartness::storage::threads::ThreadStorage;

use super::{optional_array, optional_f64, optional_str, required_str, ToolContext};

pub fn handle_focus(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let topic = required_str(params, "topic")?;
    let weight = optional_f64(params, "weight").unwrap_or(0.8).clamp(0.0, 1.0);

    let focus_id = format!("focus_{}", topic.replace(' ', "_"));
    let now = time_utils::now();

    if let Some(mut existing) = ThreadStorage::get(ctx.agent_conn, &focus_id)? {
        existing.weight = weight;
        existing.last_active = now;
        ThreadStorage::update(ctx.agent_conn, &existing)?;
    } else {
        let thread = Thread {
            id: focus_id,
            title: format!("Focus: {}", topic),
            status: ThreadStatus::Active,
            summary: None,
            origin_type: OriginType::Prompt,
            parent_id: None,
            child_ids: vec![],
            weight,
            importance: 1.0,
            importance_manually_set: true,
            relevance_score: 1.0,
            activation_count: 1,
            split_locked: false,
            split_locked_until: None,
            topics: vec![topic.clone()],
            tags: vec!["__focus__".into()],
            labels: vec!["focus".into()],
            drift_history: vec![],
            ratings: vec![],
            work_context: None,
            injection_stats: None,
            embedding: None,
            created_at: now,
            last_active: now,
        };
        ThreadStorage::insert(ctx.agent_conn, &thread)?;
    }

    Ok(serde_json::json!({"topic": topic, "weight": weight, "status": "focused"}))
}

pub fn handle_unfocus(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    if let Some(topic) = optional_str(params, "topic") {
        let focus_id = format!("focus_{}", topic.replace(' ', "_"));
        let _ = ThreadStorage::delete(ctx.agent_conn, &focus_id);
        Ok(serde_json::json!({"unfocused": topic}))
    } else {
        let all = ThreadStorage::list_all(ctx.agent_conn)?;
        let mut cleared = 0;
        for t in &all {
            if t.tags.contains(&"__focus__".to_string()) {
                let _ = ThreadStorage::delete(ctx.agent_conn, &t.id);
                cleared += 1;
            }
        }
        Ok(serde_json::json!({"cleared_all": true, "count": cleared}))
    }
}

pub fn handle_pin(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let content = required_str(params, "content")?;
    let title = optional_str(params, "title")
        .unwrap_or_else(|| content.chars().take(50).collect::<String>());
    let topics = optional_array(params, "topics").unwrap_or_default();
    let weight_boost = optional_f64(params, "weight_boost").unwrap_or(0.3).clamp(0.0, 0.5);

    let pin_id = id_gen::thread_id();
    let now = time_utils::now();

    let thread = Thread {
        id: pin_id.clone(),
        title,
        status: ThreadStatus::Active,
        summary: None,
        origin_type: OriginType::Prompt,
        parent_id: None,
        child_ids: vec![],
        weight: 1.0 + weight_boost,
        importance: 1.0,
        importance_manually_set: true,
        relevance_score: 1.0,
        activation_count: 1,
        split_locked: false,
        split_locked_until: None,
        topics,
        tags: vec!["__pin__".into()],
        labels: vec!["pin".into()],
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
        thread_id: pin_id.clone(),
        msg_id: id_gen::message_id(),
        content,
        source: "pin".into(),
        source_type: "user".into(),
        timestamp: now,
        metadata: serde_json::json!({}),
    };
    ThreadStorage::add_message(ctx.agent_conn, &msg)?;

    Ok(serde_json::json!({"pin_id": pin_id, "status": "pinned"}))
}
