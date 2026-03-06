use ai_smartness::{id_gen, time_utils};
use ai_smartness::constants::normalize_concepts;
use ai_smartness::processing::daemon_ipc_client;
use ai_smartness::thread::{OriginType, Thread, ThreadMessage, ThreadStatus};
use ai_smartness::AiResult;
use ai_smartness::registry::registry::AgentRegistry;
use ai_smartness::storage::threads::ThreadStorage;

use rusqlite::Connection;

use super::{
    check_thread_quota, optional_bool, optional_f64, optional_str, optional_usize,
    parse_object_array, parse_string_or_array, required_array, required_str, ToolContext,
};

/// Auto-suspend the lightest active thread to free a slot when quota is reached.
fn evict_lightest_thread(conn: &Connection, active: usize, quota: usize) -> AiResult<()> {
    let threads = ThreadStorage::list_active(conn)?;
    if let Some(lightest) = threads.iter().min_by(|a, b| {
        a.weight.partial_cmp(&b.weight).unwrap_or(std::cmp::Ordering::Equal)
    }) {
        tracing::info!(
            thread_id = %lightest.id,
            weight = lightest.weight,
            title = %lightest.title,
            active = active,
            quota = quota,
            "Auto-evicted lightest thread to free quota slot"
        );
        ThreadStorage::update_status(conn, &lightest.id, ThreadStatus::Suspended)?;
    }
    Ok(())
}

pub fn handle_thread_create(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    // Quota guard — auto-evict lightest thread if at capacity
    let (active, quota) = check_thread_quota(ctx)?;
    if active >= quota {
        evict_lightest_thread(ctx.agent_conn, active, quota)?;
    }

    let title = required_str(params, "title")?;
    let content = required_str(params, "content")?;
    let topics: Vec<String> = params
        .get("topics")
        .and_then(|v| parse_string_or_array(v))
        .unwrap_or_default();
    let importance = optional_f64(params, "importance").unwrap_or(0.5);
    let tags: Vec<String> = params
        .get("tags")
        .and_then(|v| parse_string_or_array(v))
        .unwrap_or_default();

    // Validate system tags (__ prefix): only allow known ones
    const ALLOWED_SYSTEM_TAGS: &[&str] = &["__pin__", "__focus__", "__mind__", "__shared__"];
    for tag in &tags {
        if tag.starts_with("__") && !ALLOWED_SYSTEM_TAGS.contains(&tag.as_str()) {
            return Err(ai_smartness::AiError::InvalidInput(format!(
                "Unknown system tag: '{}'. Allowed: {:?}",
                tag, ALLOWED_SYSTEM_TAGS
            )));
        }
    }

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
        tags,
        labels: vec![],
        concepts: vec![],
        drift_history: vec![],
        ratings: vec![],
        work_context: None,
        injection_stats: None,
        embedding: None,
        extraction_mode: ai_smartness::processing::extractor::ExtractionMode::default(),
        has_truncated_origin: false,
        continuity_parent_id: None,
        subject_coherence: None,
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
        is_truncated: false,
        continuity_from: None,
        continuity_to: None,
    };
    ThreadStorage::add_message(ctx.agent_conn, &msg)?;

    // Auto-chain __mind__ threads: fire-and-forget to daemon (async coherence gate)
    if thread.tags.contains(&"__mind__".to_string()) {
        let _ = daemon_ipc_client::send_method("mind_coherence_chain", serde_json::json!({
            "project_hash": ctx.project_hash,
            "agent_id": ctx.agent_id,
            "thread_id": thread_id,
            "content": msg.content,
        }));
    }

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
    let limit = optional_usize(params, "limit").unwrap_or_else(|| {
        // Use the agent's thread_mode quota as default limit
        AgentRegistry::get(ctx.registry_conn, ctx.agent_id, ctx.project_hash)
            .ok()
            .flatten()
            .map(|a| a.thread_mode.quota())
            .unwrap_or(50)
    });
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

    // Quota guard: evict lightest threads to make room for reactivations
    let (active, quota) = check_thread_quota(ctx)?;
    let to_activate = ids
        .iter()
        .filter_map(|id| ThreadStorage::get(ctx.agent_conn, id).ok().flatten())
        .filter(|t| t.status != ThreadStatus::Active)
        .count();
    if active + to_activate > quota {
        let need = active + to_activate - quota;
        for _ in 0..need {
            evict_lightest_thread(ctx.agent_conn, active, quota)?;
        }
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

    // Quota guard (only if not already active)
    if t.status != ThreadStatus::Active {
        let (active, quota) = check_thread_quota(ctx)?;
        if active >= quota {
            evict_lightest_thread(ctx.agent_conn, active, quota)?;
        }
    }

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
    let mode = optional_str(params, "mode").unwrap_or_else(|| "list".into());

    let thread = ThreadStorage::get(ctx.agent_conn, &id)?
        .ok_or_else(|| ai_smartness::AiError::ThreadNotFound(id.clone()))?;

    match mode.as_str() {
        "list" => {
            Ok(serde_json::json!({"thread_id": id, "labels": thread.labels}))
        }
        "set" | "add" | "remove" => {
            let labels = required_array(params, "labels")?;
            let mut thread = thread;
            match mode.as_str() {
                "set" => thread.labels = labels,
                "add" => {
                    for l in labels {
                        if !thread.labels.contains(&l) {
                            thread.labels.push(l);
                        }
                    }
                }
                "remove" => thread.labels.retain(|l| !labels.contains(l)),
                _ => unreachable!(),
            }
            ThreadStorage::update(ctx.agent_conn, &thread)?;
            Ok(serde_json::json!({"thread_id": id, "labels": thread.labels}))
        }
        _ => Err(ai_smartness::AiError::InvalidInput(
            format!("Unknown label mode: '{}'. Valid: list, set, add, remove", mode)
        )),
    }
}

pub fn handle_concepts(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let id = required_str(params, "thread_id")?;
    let mode = optional_str(params, "mode").unwrap_or_else(|| "list".into());

    let thread = ThreadStorage::get(ctx.agent_conn, &id)?
        .ok_or_else(|| ai_smartness::AiError::ThreadNotFound(id.clone()))?;

    match mode.as_str() {
        "list" => {
            Ok(serde_json::json!({"thread_id": id, "concepts": thread.concepts}))
        }
        "set" | "add" | "remove" => {
            let concepts = required_array(params, "concepts")?;
            let mut thread = thread;
            match mode.as_str() {
                "set" => thread.concepts = normalize_concepts(&concepts),
                "add" => {
                    let mut all = thread.concepts.clone();
                    all.extend(concepts);
                    thread.concepts = normalize_concepts(&all);
                }
                "remove" => {
                    let to_remove: std::collections::HashSet<String> =
                        concepts.into_iter().map(|c| c.to_lowercase()).collect();
                    thread.concepts.retain(|c| !to_remove.contains(&c.to_lowercase()));
                }
                _ => unreachable!(),
            }
            ThreadStorage::update(ctx.agent_conn, &thread)?;
            Ok(serde_json::json!({"thread_id": id, "concepts": thread.concepts}))
        }
        _ => Err(ai_smartness::AiError::InvalidInput(
            format!("Unknown concepts mode: '{}'. Valid: list, set, add, remove", mode)
        )),
    }
}

pub fn handle_backfill_concepts(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let limit = optional_usize(params, "limit").unwrap_or(10);
    let dry_run = optional_bool(params, "dry_run").unwrap_or(false);

    // All active threads are candidates — no filter.
    // When called explicitly by agent/human, re-enrichment is always valid.
    // Prioritize by importance (higher first), take up to limit.
    let all = ThreadStorage::list_active(ctx.agent_conn)?;
    let mut candidates: Vec<&Thread> = all.iter().collect();
    candidates.sort_by(|a, b| b.importance.partial_cmp(&a.importance).unwrap_or(std::cmp::Ordering::Equal));
    let batch: Vec<&Thread> = candidates.into_iter().take(limit).collect();

    if batch.is_empty() {
        return Ok(serde_json::json!({
            "status": "nothing_to_do",
            "message": "No active threads"
        }));
    }

    if dry_run {
        let preview: Vec<serde_json::Value> = batch.iter().map(|t| {
            serde_json::json!({
                "id": &t.id[..8.min(t.id.len())],
                "title": &t.title,
                "topics": &t.topics,
                "labels": &t.labels,
                "has_summary": t.summary.is_some(),
                "has_concepts": !t.concepts.is_empty(),
            })
        }).collect();
        return Ok(serde_json::json!({
            "dry_run": true,
            "candidates": batch.len(),
            "total_active": all.len(),
            "preview": preview
        }));
    }

    // Submit enrichment jobs via IPC → daemon capture queue (async, non-blocking)
    let mut queued = 0usize;
    let mut failed = 0usize;
    let mut results = Vec::new();

    for thread in &batch {
        let ipc_params = serde_json::json!({
            "project_hash": ctx.project_hash,
            "agent_id": ctx.agent_id,
            "thread_id": thread.id,
            "content": "",
        });

        match ai_smartness::processing::daemon_ipc_client::send_method("enrich_thread", ipc_params) {
            Ok(resp) => {
                let was_queued = resp.get("queued").and_then(|v| v.as_bool()).unwrap_or(false);
                if was_queued {
                    queued += 1;
                    results.push(serde_json::json!({
                        "id": &thread.id[..8.min(thread.id.len())],
                        "title": &thread.title,
                        "status": "queued",
                    }));
                } else {
                    failed += 1;
                }
            }
            Err(_) => { failed += 1; }
        }
    }

    Ok(serde_json::json!({
        "queued": queued,
        "failed": failed,
        "total_active": all.len(),
        "results": results
    }))
}

pub fn handle_thread_purge(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let status_str = required_str(params, "status")?;
    let confirm = optional_bool(params, "confirm").unwrap_or(false);
    let status: ThreadStatus = status_str
        .parse()
        .map_err(|e: String| ai_smartness::AiError::InvalidInput(e))?;

    // Safety: never purge active threads
    if status == ThreadStatus::Active {
        return Err(ai_smartness::AiError::InvalidInput(
            "Cannot purge active threads. Suspend them first.".into(),
        ));
    }

    let count = ThreadStorage::count_by_status(ctx.agent_conn, &status)?;

    if !confirm {
        return Ok(serde_json::json!({
            "dry_run": true,
            "status": status.as_str(),
            "count": count,
            "message": format!("Would delete {} {} thread(s). Pass confirm=true to execute.", count, status.as_str())
        }));
    }

    let deleted = ThreadStorage::delete_by_status(ctx.agent_conn, &status)?;
    Ok(serde_json::json!({
        "purged": deleted,
        "status": status.as_str()
    }))
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
        .and_then(|v| parse_object_array(v))
        .ok_or_else(|| ai_smartness::AiError::InvalidInput("Missing operations".into()))?;
    let mut renamed = 0;
    for op in &ops {
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

pub fn handle_mark_used(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let id = required_str(params, "thread_id")?;
    let mut thread = ThreadStorage::get(ctx.agent_conn, &id)?
        .ok_or_else(|| ai_smartness::AiError::ThreadNotFound(id.clone()))?;

    let stats = thread
        .injection_stats
        .get_or_insert_with(ai_smartness::thread::InjectionStats::default);
    stats.record_usage();
    let ratio = stats.usage_ratio();
    let used = stats.used_count;
    let injected = stats.injection_count;

    ThreadStorage::update(ctx.agent_conn, &thread)?;

    Ok(serde_json::json!({
        "thread_id": id,
        "used_count": used,
        "injection_count": injected,
        "usage_ratio": ratio,
    }))
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
        "continuity_parent_id": t.continuity_parent_id,
        "subject_coherence": t.subject_coherence,
    })
}

pub fn handle_continuity_edges(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let action = optional_str(params, "action").unwrap_or_else(|| "list".to_string());

    match action.as_str() {
        "list" => {
            let edges = ThreadStorage::get_continuity_edges(ctx.agent_conn)?;
            let results: Vec<serde_json::Value> = edges
                .iter()
                .map(|(child_id, parent_id, coherence)| {
                    serde_json::json!({
                        "source_id": parent_id,
                        "target_id": child_id,
                        "subject_coherence": coherence,
                    })
                })
                .collect();
            Ok(serde_json::json!({"edges": results, "count": results.len()}))
        }
        "set" => {
            let thread_id = required_str(params, "thread_id")?;
            let parent_id = required_str(params, "parent_id")?;
            let coherence = optional_f64(params, "coherence");
            ThreadStorage::set_continuity_parent(ctx.agent_conn, &thread_id, &parent_id, coherence)?;
            Ok(serde_json::json!({"ok": true, "action": "set", "thread_id": thread_id, "parent_id": parent_id}))
        }
        "unset" => {
            let thread_id = required_str(params, "thread_id")?;
            ThreadStorage::unset_continuity_parent(ctx.agent_conn, &thread_id)?;
            Ok(serde_json::json!({"ok": true, "action": "unset", "thread_id": thread_id}))
        }
        "scan_orphans" => {
            let orphans = ThreadStorage::scan_orphan_continuity(ctx.agent_conn)?;
            Ok(serde_json::json!({"orphans": orphans, "count": orphans.len()}))
        }
        "repair" => {
            let cleaned = ThreadStorage::cleanup_orphan_continuity(ctx.agent_conn)?;
            Ok(serde_json::json!({"ok": true, "action": "repair", "cleaned": cleaned}))
        }
        _ => Err(ai_smartness::AiError::InvalidInput(format!("Unknown continuity action: {}", action))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ai_smartness::agent::{Agent, AgentStatus, CoordinationMode, ThreadMode};
    use ai_smartness::registry::registry::AgentRegistry;
    use ai_smartness::storage::threads::ThreadStorage;
    use ai_smartness::thread::ThreadStatus;
    use rusqlite::{params, Connection};

    const PH: &str = "test-ph";
    const AGENT: &str = "test-agent";

    fn setup_agent_db() -> Connection {
        let conn = Connection::open(":memory:").unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;").unwrap();
        ai_smartness::storage::migrations::migrate_agent_db(&conn).unwrap();
        conn
    }

    fn setup_registry_db() -> Connection {
        let conn = Connection::open(":memory:").unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;").unwrap();
        ai_smartness::storage::migrations::migrate_registry_db(&conn).unwrap();
        conn
    }

    fn setup_shared_db() -> Connection {
        let conn = Connection::open(":memory:").unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;").unwrap();
        ai_smartness::storage::migrations::migrate_shared_db(&conn).unwrap();
        conn
    }

    fn insert_project(conn: &Connection) {
        let now = ai_smartness::time_utils::to_sqlite(&ai_smartness::time_utils::now());
        conn.execute(
            "INSERT INTO projects (hash, path, name, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![PH, "/tmp/test", "test", now],
        ).unwrap();
    }

    fn register_agent(conn: &Connection, mode: ThreadMode) {
        let now = chrono::Utc::now();
        let agent = Agent {
            id: AGENT.to_string(),
            project_hash: PH.to_string(),
            name: AGENT.to_string(),
            description: String::new(),
            role: "programmer".to_string(),
            capabilities: vec![],
            status: AgentStatus::Active,
            last_seen: now,
            registered_at: now,
            supervisor_id: None,
            coordination_mode: CoordinationMode::Autonomous,
            team: None,
            specializations: vec![],
            thread_mode: mode,
            current_activity: String::new(),
            report_to: None,
            custom_role: None,
            workspace_path: String::new(),
            full_permissions: false,
            expected_model: None,
        };
        AgentRegistry::register(conn, &agent).unwrap();
    }

    fn insert_active_threads(conn: &Connection, count: usize) {
        for i in 0..count {
            let t = ai_smartness::thread::Thread {
                id: format!("t-{}", i),
                title: format!("Thread {}", i),
                status: ThreadStatus::Active,
                summary: None,
                origin_type: ai_smartness::thread::OriginType::Prompt,
                parent_id: None,
                child_ids: vec![],
                weight: 0.5,
                importance: 0.5,
                importance_manually_set: false,
                relevance_score: 1.0,
                activation_count: 1,
                split_locked: false,
                split_locked_until: None,
                topics: vec![],
                tags: vec![],
                labels: vec![],
                concepts: vec![],
                drift_history: vec![],
                ratings: vec![],
                work_context: None,
                injection_stats: None,
                embedding: None,
                extraction_mode: ai_smartness::processing::extractor::ExtractionMode::default(),
                has_truncated_origin: false,
                continuity_parent_id: None,
                subject_coherence: None,
                created_at: chrono::Utc::now(),
                last_active: chrono::Utc::now(),
            };
            ThreadStorage::insert(conn, &t).unwrap();
        }
    }

    #[test]
    fn test_create_rejects_at_quota() {
        let agent_conn = setup_agent_db();
        let registry_conn = setup_registry_db();
        let shared_conn = setup_shared_db();
        insert_project(&registry_conn);
        register_agent(&registry_conn, ThreadMode::Light); // quota=15
        insert_active_threads(&agent_conn, 15);

        let ctx = ToolContext {
            agent_conn: &agent_conn,
            registry_conn: &registry_conn,
            shared_conn: &shared_conn,
            project_hash: PH,
            agent_id: AGENT,
        };

        let params = serde_json::json!({"title": "new thread", "content": "hello"});
        let result = handle_thread_create(&params, &ctx);
        assert!(result.is_err(), "Should reject when at quota");
    }

    #[test]
    fn test_create_succeeds_under_quota() {
        let agent_conn = setup_agent_db();
        let registry_conn = setup_registry_db();
        let shared_conn = setup_shared_db();
        insert_project(&registry_conn);
        register_agent(&registry_conn, ThreadMode::Light); // quota=15
        insert_active_threads(&agent_conn, 14);

        let ctx = ToolContext {
            agent_conn: &agent_conn,
            registry_conn: &registry_conn,
            shared_conn: &shared_conn,
            project_hash: PH,
            agent_id: AGENT,
        };

        let params = serde_json::json!({"title": "new thread", "content": "hello"});
        let result = handle_thread_create(&params, &ctx);
        assert!(result.is_ok(), "Should succeed under quota");
    }

    #[test]
    fn test_activate_rejects_exceeding_quota() {
        let agent_conn = setup_agent_db();
        let registry_conn = setup_registry_db();
        let shared_conn = setup_shared_db();
        insert_project(&registry_conn);
        register_agent(&registry_conn, ThreadMode::Light); // quota=15
        insert_active_threads(&agent_conn, 15);

        // Insert a suspended thread to try activating
        let t = ai_smartness::thread::Thread {
            id: "suspended-1".to_string(),
            title: "Suspended".to_string(),
            status: ThreadStatus::Suspended,
            summary: None,
            origin_type: ai_smartness::thread::OriginType::Prompt,
            parent_id: None,
            child_ids: vec![],
            weight: 0.5,
            importance: 0.5,
            importance_manually_set: false,
            relevance_score: 1.0,
            activation_count: 1,
            split_locked: false,
            split_locked_until: None,
            topics: vec![],
            tags: vec![],
            labels: vec![],
            concepts: vec![],
            drift_history: vec![],
            ratings: vec![],
            work_context: None,
            injection_stats: None,
            embedding: None,
            extraction_mode: ai_smartness::processing::extractor::ExtractionMode::default(),
            has_truncated_origin: false,
            continuity_parent_id: None,
            subject_coherence: None,
            created_at: chrono::Utc::now(),
            last_active: chrono::Utc::now(),
        };
        ThreadStorage::insert(&agent_conn, &t).unwrap();

        let ctx = ToolContext {
            agent_conn: &agent_conn,
            registry_conn: &registry_conn,
            shared_conn: &shared_conn,
            project_hash: PH,
            agent_id: AGENT,
        };

        let params = serde_json::json!({"thread_ids": ["suspended-1"], "confirm": true});
        let result = handle_thread_activate(&params, &ctx);
        assert!(result.is_err(), "Should reject activation when at quota");
    }

    #[test]
    fn test_activate_succeeds_within_quota() {
        let agent_conn = setup_agent_db();
        let registry_conn = setup_registry_db();
        let shared_conn = setup_shared_db();
        insert_project(&registry_conn);
        register_agent(&registry_conn, ThreadMode::Light); // quota=15
        insert_active_threads(&agent_conn, 14);

        // Insert a suspended thread
        let t = ai_smartness::thread::Thread {
            id: "suspended-1".to_string(),
            title: "Suspended".to_string(),
            status: ThreadStatus::Suspended,
            summary: None,
            origin_type: ai_smartness::thread::OriginType::Prompt,
            parent_id: None,
            child_ids: vec![],
            weight: 0.5,
            importance: 0.5,
            importance_manually_set: false,
            relevance_score: 1.0,
            activation_count: 1,
            split_locked: false,
            split_locked_until: None,
            topics: vec![],
            tags: vec![],
            labels: vec![],
            concepts: vec![],
            drift_history: vec![],
            ratings: vec![],
            work_context: None,
            injection_stats: None,
            embedding: None,
            extraction_mode: ai_smartness::processing::extractor::ExtractionMode::default(),
            has_truncated_origin: false,
            continuity_parent_id: None,
            subject_coherence: None,
            created_at: chrono::Utc::now(),
            last_active: chrono::Utc::now(),
        };
        ThreadStorage::insert(&agent_conn, &t).unwrap();

        let ctx = ToolContext {
            agent_conn: &agent_conn,
            registry_conn: &registry_conn,
            shared_conn: &shared_conn,
            project_hash: PH,
            agent_id: AGENT,
        };

        let params = serde_json::json!({"thread_ids": ["suspended-1"], "confirm": true});
        let result = handle_thread_activate(&params, &ctx);
        assert!(result.is_ok(), "Should succeed within quota");
    }

    #[test]
    fn test_reactivate_rejects_at_quota() {
        let agent_conn = setup_agent_db();
        let registry_conn = setup_registry_db();
        let shared_conn = setup_shared_db();
        insert_project(&registry_conn);
        register_agent(&registry_conn, ThreadMode::Light); // quota=15
        insert_active_threads(&agent_conn, 15);

        // Insert a suspended thread
        let t = ai_smartness::thread::Thread {
            id: "suspended-1".to_string(),
            title: "Suspended".to_string(),
            status: ThreadStatus::Suspended,
            summary: None,
            origin_type: ai_smartness::thread::OriginType::Prompt,
            parent_id: None,
            child_ids: vec![],
            weight: 0.5,
            importance: 0.5,
            importance_manually_set: false,
            relevance_score: 1.0,
            activation_count: 1,
            split_locked: false,
            split_locked_until: None,
            topics: vec![],
            tags: vec![],
            labels: vec![],
            concepts: vec![],
            drift_history: vec![],
            ratings: vec![],
            work_context: None,
            injection_stats: None,
            embedding: None,
            extraction_mode: ai_smartness::processing::extractor::ExtractionMode::default(),
            has_truncated_origin: false,
            continuity_parent_id: None,
            subject_coherence: None,
            created_at: chrono::Utc::now(),
            last_active: chrono::Utc::now(),
        };
        ThreadStorage::insert(&agent_conn, &t).unwrap();

        let ctx = ToolContext {
            agent_conn: &agent_conn,
            registry_conn: &registry_conn,
            shared_conn: &shared_conn,
            project_hash: PH,
            agent_id: AGENT,
        };

        let params = serde_json::json!({"thread_id": "suspended-1"});
        let result = handle_reactivate(&params, &ctx);
        assert!(result.is_err(), "Should reject reactivation when at quota");
    }

    #[test]
    fn test_reactivate_skips_check_for_active() {
        let agent_conn = setup_agent_db();
        let registry_conn = setup_registry_db();
        let shared_conn = setup_shared_db();
        insert_project(&registry_conn);
        register_agent(&registry_conn, ThreadMode::Light); // quota=15
        insert_active_threads(&agent_conn, 15);

        let ctx = ToolContext {
            agent_conn: &agent_conn,
            registry_conn: &registry_conn,
            shared_conn: &shared_conn,
            project_hash: PH,
            agent_id: AGENT,
        };

        // Reactivate an already-active thread — should skip quota check
        let params = serde_json::json!({"thread_id": "t-0"});
        let result = handle_reactivate(&params, &ctx);
        assert!(result.is_ok(), "Should skip quota check for already-active thread");
    }

    #[test]
    fn test_quota_zero_edge_case() {
        // When agent is not in registry, fallback quota=50
        let agent_conn = setup_agent_db();
        let registry_conn = setup_registry_db();
        let shared_conn = setup_shared_db();
        insert_project(&registry_conn);
        // Do NOT register agent — fallback to default quota=50

        let ctx = ToolContext {
            agent_conn: &agent_conn,
            registry_conn: &registry_conn,
            shared_conn: &shared_conn,
            project_hash: PH,
            agent_id: "nonexistent-agent",
        };

        let (active, quota) = check_thread_quota(&ctx).unwrap();
        assert_eq!(active, 0);
        assert_eq!(quota, 50, "Fallback quota should be 50 for unknown agent");
    }
}
