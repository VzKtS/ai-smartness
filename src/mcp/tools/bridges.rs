use ai_smartness::AiResult;
use ai_smartness::storage::bridges::BridgeStorage;

use super::{optional_bool, optional_str, required_array, required_str, ToolContext};

pub fn handle_bridges(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let thread_id = optional_str(params, "thread_id");

    let bridges = if let Some(ref tid) = thread_id {
        BridgeStorage::list_for_thread(ctx.agent_conn, tid)?
    } else {
        BridgeStorage::list_all(ctx.agent_conn)?
    };

    let results: Vec<serde_json::Value> = bridges
        .iter()
        .map(|b| {
            serde_json::json!({
                "id": b.id,
                "source_id": b.source_id,
                "target_id": b.target_id,
                "relation": b.relation_type.as_str(),
                "status": b.status.as_str(),
                "weight": b.weight,
                "use_count": b.use_count,
                "created_at": b.created_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(serde_json::json!({"bridges": results, "count": results.len()}))
}

pub fn handle_bridge_analysis(
    _params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let all = BridgeStorage::list_all(ctx.agent_conn)?;
    let total = all.len();
    let active = all.iter().filter(|b| b.status.as_str() == "active").count();
    let weak = all.iter().filter(|b| b.weight < 0.1).count();
    let avg_weight = if total > 0 {
        all.iter().map(|b| b.weight).sum::<f64>() / total as f64
    } else {
        0.0
    };

    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for b in &all {
        *counts.entry(b.source_id.clone()).or_insert(0) += 1;
        *counts.entry(b.target_id.clone()).or_insert(0) += 1;
    }
    let mut top: Vec<(String, usize)> = counts.into_iter().collect();
    top.sort_by(|a, b| b.1.cmp(&a.1));
    top.truncate(5);

    Ok(serde_json::json!({
        "total": total,
        "active": active,
        "weak": weak,
        "avg_weight": (avg_weight * 1000.0).round() / 1000.0,
        "most_connected": top.iter().map(|(id, c)| serde_json::json!({"thread_id": id, "bridges": c})).collect::<Vec<_>>(),
    }))
}

pub fn handle_bridge_scan_orphans(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let confirm = optional_bool(params, "confirm").unwrap_or(false);
    let orphans = BridgeStorage::scan_orphans(ctx.agent_conn)?;

    if !confirm {
        let preview: Vec<serde_json::Value> = orphans
            .iter()
            .map(|b| serde_json::json!({"id": b.id, "source_id": b.source_id, "target_id": b.target_id}))
            .collect();
        return Ok(serde_json::json!({"dry_run": true, "orphans": preview, "count": preview.len()}));
    }

    let ids: Vec<String> = orphans.iter().map(|b| b.id.clone()).collect();
    let deleted = BridgeStorage::delete_batch(ctx.agent_conn, &ids)?;
    Ok(serde_json::json!({"deleted": deleted}))
}

pub fn handle_bridge_kill(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let id = required_str(params, "bridge_id")?;
    BridgeStorage::delete(ctx.agent_conn, &id)?;
    Ok(serde_json::json!({"deleted": id}))
}

pub fn handle_bridge_kill_batch(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let ids = required_array(params, "bridge_ids")?;
    let deleted = BridgeStorage::delete_batch(ctx.agent_conn, &ids)?;
    Ok(serde_json::json!({"deleted": deleted}))
}
