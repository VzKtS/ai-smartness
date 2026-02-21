use ai_smartness::{id_gen, time_utils};
use ai_smartness::shared::{SharedThread, SharedVisibility};
use ai_smartness::AiResult;
use ai_smartness::storage::shared_storage::SharedStorage;
use ai_smartness::storage::threads::ThreadStorage;

use super::{optional_bool, optional_str, required_str, ToolContext};

pub fn handle_share(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let thread_id = required_str(params, "thread_id")?;
    let visibility = optional_str(params, "visibility").unwrap_or_else(|| "network".into());

    let mut thread = ThreadStorage::get(ctx.agent_conn, &thread_id)?
        .ok_or_else(|| ai_smartness::AiError::ThreadNotFound(thread_id.clone()))?;

    // Tag thread as shared to protect from decay/archive
    if !thread.tags.contains(&"__shared__".to_string()) {
        thread.tags.push("__shared__".to_string());
        ThreadStorage::update(ctx.agent_conn, &thread)?;
    }

    let shared_id = id_gen::message_id();
    let vis = match visibility.as_str() {
        "restricted" => SharedVisibility::Restricted,
        _ => SharedVisibility::Network,
    };

    let allowed_agents: Vec<String> = params
        .get("allowed_agents")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let shared = SharedThread {
        shared_id: shared_id.clone(),
        thread_id: thread_id.clone(),
        owner_agent: ctx.agent_id.to_string(),
        title: thread.title.clone(),
        topics: thread.topics.clone(),
        visibility: vis,
        allowed_agents,
        published_at: time_utils::now(),
        updated_at: None,
    };

    SharedStorage::publish(ctx.shared_conn, &shared)?;
    Ok(serde_json::json!({"shared_id": shared_id, "thread_id": thread_id}))
}

pub fn handle_unshare(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let shared_id = required_str(params, "shared_id")?;

    // Look up source thread before deleting the shared record
    let shared = SharedStorage::get(ctx.shared_conn, &shared_id)?;
    SharedStorage::unpublish(ctx.shared_conn, &shared_id)?;

    // Remove __shared__ tag if no other shared entries reference this thread
    if let Some(shared) = shared {
        let remaining = SharedStorage::count_by_thread_id(ctx.shared_conn, &shared.thread_id)?;
        if remaining == 0 {
            if let Ok(Some(mut thread)) = ThreadStorage::get(ctx.agent_conn, &shared.thread_id) {
                thread.tags.retain(|t| t != "__shared__");
                let _ = ThreadStorage::update(ctx.agent_conn, &thread);
            }
        }
    }

    Ok(serde_json::json!({"unshared": shared_id}))
}

pub fn handle_publish(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let shared_id = required_str(params, "shared_id")?;

    // Re-publish (update) the shared snapshot
    if let Some(mut shared) = SharedStorage::get(ctx.shared_conn, &shared_id)? {
        // Refresh from source thread
        if let Ok(Some(thread)) = ThreadStorage::get(ctx.agent_conn, &shared.thread_id) {
            shared.title = thread.title;
            shared.topics = thread.topics;
        }
        shared.updated_at = Some(time_utils::now());
        SharedStorage::publish(ctx.shared_conn, &shared)?;
        Ok(serde_json::json!({"published": shared_id}))
    } else {
        Err(ai_smartness::AiError::InvalidInput(format!(
            "Shared thread '{}' not found",
            shared_id
        )))
    }
}
