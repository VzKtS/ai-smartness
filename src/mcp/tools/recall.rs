use ai_smartness::AiResult;
use ai_smartness::config::EngramConfig;
use ai_smartness::intelligence::engram_retriever::EngramRetriever;
use ai_smartness::storage::beat::BeatState;
use ai_smartness::storage::bridges::BridgeStorage;
use ai_smartness::storage::path_utils;

use super::{optional_str, required_str, ToolContext};

pub fn handle_recall(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let query = required_str(params, "query")?;
    let label_filter = optional_str(params, "label");

    let engram = EngramRetriever::new(ctx.agent_conn, EngramConfig::default())?;
    let mut threads = engram.search(ctx.agent_conn, &query, 10)?;

    if let Some(ref label) = label_filter {
        threads.retain(|t| t.labels.iter().any(|l| l.contains(label.as_str())));
    }

    threads.truncate(10);

    let mut bridges_out = Vec::new();
    for t in &threads {
        if let Ok(bs) = BridgeStorage::list_for_thread(ctx.agent_conn, &t.id) {
            for b in bs.into_iter().take(3) {
                bridges_out.push(serde_json::json!({
                    "id": b.id,
                    "source_id": b.source_id,
                    "target_id": b.target_id,
                    "relation": b.relation_type.as_str(),
                    "weight": b.weight,
                }));
            }
        }
    }

    let threads_json: Vec<serde_json::Value> = threads
        .iter()
        .map(|t| {
            serde_json::json!({
                "id": t.id,
                "title": t.title,
                "status": t.status.as_str(),
                "weight": t.weight,
                "importance": t.importance,
                "topics": t.topics,
                "labels": t.labels,
                "summary": t.summary,
                "last_active": t.last_active.to_rfc3339(),
            })
        })
        .collect();

    // Update last_recall_beat in beat state
    let agent_data = path_utils::agent_data_dir(ctx.project_hash, ctx.agent_id);
    let mut beat_state = BeatState::load(&agent_data);
    beat_state.last_recall_beat = beat_state.beat;
    beat_state.save(&agent_data);

    Ok(serde_json::json!({
        "threads": threads_json,
        "bridges": bridges_out,
        "count": threads_json.len(),
    }))
}
