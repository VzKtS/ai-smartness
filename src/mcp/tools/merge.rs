use ai_smartness::AiResult;
use ai_smartness::intelligence::merge_metadata;
use ai_smartness::storage::threads::ThreadStorage;

use super::{parse_object_array, required_str, ToolContext};

pub fn handle_merge(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let survivor_id = required_str(params, "survivor_id")?;
    let absorbed_id = required_str(params, "absorbed_id")?;

    let survivor = ThreadStorage::get(ctx.agent_conn, &survivor_id)?
        .ok_or_else(|| ai_smartness::AiError::ThreadNotFound(survivor_id.clone()))?;
    let absorbed = ThreadStorage::get(ctx.agent_conn, &absorbed_id)?
        .ok_or_else(|| ai_smartness::AiError::ThreadNotFound(absorbed_id.clone()))?;

    // Move messages
    let msgs = ThreadStorage::get_messages(ctx.agent_conn, &absorbed_id)?;
    for mut msg in msgs {
        msg.thread_id = survivor_id.clone();
        let _ = ThreadStorage::add_message(ctx.agent_conn, &msg);
    }

    // Merge metadata with consolidation (dedup + substring removal + caps)
    let mut merged = survivor.clone();
    merge_metadata::consolidate_after_merge(&mut merged, &absorbed);
    ThreadStorage::update(ctx.agent_conn, &merged)?;
    ThreadStorage::delete(ctx.agent_conn, &absorbed_id)?;

    Ok(serde_json::json!({
        "survivor_id": survivor_id,
        "absorbed_id": absorbed_id,
        "survivor_title": survivor.title,
        "absorbed_title": absorbed.title,
    }))
}

pub fn handle_merge_batch(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let ops = params
        .get("operations")
        .and_then(|v| parse_object_array(v))
        .ok_or_else(|| ai_smartness::AiError::InvalidInput("Missing operations".into()))?;

    let mut merged = 0;
    let mut errors = Vec::new();
    for op in &ops {
        let sid = op.get("survivor_id").and_then(|v| v.as_str()).unwrap_or("");
        let aid = op.get("absorbed_id").and_then(|v| v.as_str()).unwrap_or("");
        if sid.is_empty() || aid.is_empty() {
            continue;
        }
        let p = serde_json::json!({"survivor_id": sid, "absorbed_id": aid});
        match handle_merge(&p, ctx) {
            Ok(_) => merged += 1,
            Err(e) => errors.push(format!("{}->{}: {}", sid, aid, e)),
        }
    }
    Ok(serde_json::json!({"merged": merged, "errors": errors}))
}
