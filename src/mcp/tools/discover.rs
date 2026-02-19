use ai_smartness::time_utils;
use ai_smartness::shared::Subscription;
use ai_smartness::AiResult;
use ai_smartness::storage::shared_storage::SharedStorage;

use super::{optional_array, optional_str, optional_usize, required_str, ToolContext};

pub fn handle_discover(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let topics = optional_array(params, "topics").unwrap_or_default();
    let _agent_filter = optional_str(params, "agent_id");

    let shared = SharedStorage::discover(ctx.shared_conn, &topics)?;

    let results: Vec<serde_json::Value> = shared
        .iter()
        .map(|s| {
            serde_json::json!({
                "shared_id": s.shared_id,
                "thread_id": s.thread_id,
                "owner_agent": s.owner_agent,
                "title": s.title,
                "topics": s.topics,
                "published_at": s.published_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(serde_json::json!({"shared": results, "count": results.len()}))
}

pub fn handle_subscribe(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let shared_id = required_str(params, "shared_id")?;

    let sub = Subscription {
        shared_id: shared_id.clone(),
        subscriber_agent: ctx.agent_id.to_string(),
        subscribed_at: time_utils::now(),
        last_synced: None,
    };

    SharedStorage::subscribe(ctx.shared_conn, &sub)?;
    Ok(serde_json::json!({"subscribed": shared_id}))
}

pub fn handle_unsubscribe(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let shared_id = required_str(params, "shared_id")?;
    SharedStorage::unsubscribe(ctx.shared_conn, &shared_id, ctx.agent_id)?;
    Ok(serde_json::json!({"unsubscribed": shared_id}))
}

pub fn handle_recommend(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let limit = optional_usize(params, "limit").unwrap_or(5);

    // Recommend shared threads the agent hasn't subscribed to yet
    let all_shared = SharedStorage::discover(ctx.shared_conn, &[])?;
    let my_subs = SharedStorage::list_subscriptions(ctx.shared_conn, ctx.agent_id)?;
    let sub_ids: std::collections::HashSet<String> =
        my_subs.iter().map(|s| s.shared_id.clone()).collect();

    let recommendations: Vec<serde_json::Value> = all_shared
        .iter()
        .filter(|s| !sub_ids.contains(&s.shared_id) && s.owner_agent != ctx.agent_id)
        .take(limit)
        .map(|s| {
            serde_json::json!({
                "shared_id": s.shared_id,
                "title": s.title,
                "owner": s.owner_agent,
                "topics": s.topics,
            })
        })
        .collect();

    Ok(serde_json::json!({"recommendations": recommendations, "count": recommendations.len()}))
}

pub fn handle_sync(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let shared_id = optional_str(params, "shared_id");

    if let Some(sid) = shared_id {
        SharedStorage::update_sync(ctx.shared_conn, &sid, ctx.agent_id)?;
        Ok(serde_json::json!({"synced": sid}))
    } else {
        let subs = SharedStorage::list_subscriptions(ctx.shared_conn, ctx.agent_id)?;
        let mut synced = 0;
        for sub in &subs {
            let _ = SharedStorage::update_sync(ctx.shared_conn, &sub.shared_id, ctx.agent_id);
            synced += 1;
        }
        Ok(serde_json::json!({"synced_all": synced}))
    }
}
