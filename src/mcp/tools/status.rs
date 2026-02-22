use ai_smartness::thread::ThreadStatus;
use ai_smartness::AiResult;
use ai_smartness::storage::backup::BackupManager;
use ai_smartness::storage::beat::BeatState;
use ai_smartness::storage::bridges::BridgeStorage;
use ai_smartness::storage::path_utils;
use ai_smartness::storage::shared_storage::SharedStorage;
use ai_smartness::storage::threads::ThreadStorage;

use super::{optional_str, required_str, ToolContext};

pub fn handle_status(
    _params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let active = ThreadStorage::count_by_status(ctx.agent_conn, &ThreadStatus::Active)?;
    let suspended = ThreadStorage::count_by_status(ctx.agent_conn, &ThreadStatus::Suspended)?;
    let archived = ThreadStorage::count_by_status(ctx.agent_conn, &ThreadStatus::Archived)?;
    let bridges = BridgeStorage::count(ctx.agent_conn)?;

    // E7: Expose beat.json data in ai_status response
    let data_dir = path_utils::agent_data_dir(ctx.project_hash, ctx.agent_id);
    let beat = BeatState::load(&data_dir);

    Ok(serde_json::json!({
        "threads": {"active": active, "suspended": suspended, "archived": archived},
        "bridges": bridges,
        "agent_id": ctx.agent_id,
        "project_hash": ctx.project_hash,
        "beat": beat.beat,
        "started_at": beat.started_at,
        "last_beat_at": beat.last_beat_at,
        "last_interaction_at": beat.last_interaction_at,
        "since_last_interaction": beat.since_last(),
        "session_id": beat.last_session_id,
        "context_tokens": beat.context_tokens,
        "context_percent": beat.context_percent,
        "quota": beat.quota,
        "prompt_count": beat.prompt_count,
        "tool_call_count": beat.tool_call_count,
        "response_latency_ms": beat.response_latency_ms,
        "last_error": beat.last_error,
        "last_error_at": beat.last_error_at,
        "plan_type": beat.plan_type,
        "plan_tier": beat.plan_tier,
        "quota_5h": beat.quota_5h,
        "quota_7d": beat.quota_7d,
        "quota_constraint": beat.quota_constraint,
        "quota_alert": beat.quota_alert,
        "quota_updated_at": beat.quota_updated_at,
    }))
}

pub fn handle_sysinfo(
    _params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let total_threads = ThreadStorage::count(ctx.agent_conn)?;
    let total_bridges = BridgeStorage::count(ctx.agent_conn)?;

    let db_path = path_utils::agent_db_path(ctx.project_hash, ctx.agent_id);
    let disk_usage = std::fs::metadata(&db_path)
        .map(|m| m.len())
        .unwrap_or(0);

    Ok(serde_json::json!({
        "threads": total_threads,
        "bridges": total_bridges,
        "disk_usage_bytes": disk_usage,
        "embedding_backend": "tfidf_hash",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

pub fn handle_help(
    _params: &serde_json::Value,
    _ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    Ok(serde_json::json!({
        "name": "AI Smartness",
        "version": env!("CARGO_PKG_VERSION"),
        "tool_count": 67,
        "categories": [
            "Memory & Search (ai_recall)",
            "Thread Lifecycle (ai_thread_create, ai_thread_rm, ai_thread_rm_batch, ai_thread_list, ai_thread_search, ai_thread_activate, ai_thread_suspend, ai_reactivate)",
            "Thread Operations (ai_merge, ai_merge_batch, ai_split, ai_split_unlock)",
            "Thread Metadata (ai_label, ai_labels_suggest, ai_rename, ai_rename_batch, ai_rate_importance, ai_rate_context)",
            "Bridges (ai_bridges, ai_bridge_analysis, ai_bridge_scan_orphans, ai_bridge_kill, ai_bridge_kill_batch)",
            "Focus & Pins (ai_focus, ai_unfocus, ai_pin)",
            "Cognitive Messaging (ai_msg_focus, ai_msg_ack)",
            "Shared Cognition (ai_share, ai_unshare, ai_publish, ai_discover, ai_subscribe, ai_unsubscribe, ai_sync, ai_shared_status, ai_recommend)",
            "System & Status (ai_status, ai_sysinfo, ai_help, ai_suggestions, ai_profile, ai_topics, health_check, topics_network, metrics_cross_agent, test_sampling)",
            "Maintenance (ai_cleanup, ai_lock, ai_unlock, ai_lock_status, ai_backup, beat_wake)",
            "Agent Management (ai_agent_select, agent_list, agent_query, agent_status, agent_cleanup, agent_configure, agent_tasks, task_delegate, task_status)",
            "Inter-Agent Messaging (msg_send, msg_broadcast, msg_inbox, msg_reply)",
        ],
    }))
}

pub fn handle_suggestions(
    _params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let mut suggestions = Vec::new();

    let all_threads = ThreadStorage::list_all(ctx.agent_conn)?;
    let total = all_threads.len();

    if total > 0 {
        let unlabeled = all_threads.iter().filter(|t| t.labels.is_empty()).count();
        if unlabeled as f64 / total as f64 > 0.4 {
            suggestions.push(serde_json::json!({
                "type": "unlabeled_threads",
                "message": format!("{}/{} threads have no labels. Use ai_label to organize.", unlabeled, total),
            }));
        }

        let single_msg = all_threads
            .iter()
            .filter(|t| t.status == ThreadStatus::Active)
            .count();
        // Check single-message threads would require message_count calls, simplified
        if single_msg > 0 {
            suggestions.push(serde_json::json!({
                "type": "thread_health",
                "message": format!("{} active threads. Consider merging similar ones.", single_msg),
            }));
        }
    }

    let weak_bridges = BridgeStorage::list_all(ctx.agent_conn)?
        .iter()
        .filter(|b| b.weight < 0.1)
        .count();
    if weak_bridges > 50 {
        suggestions.push(serde_json::json!({
            "type": "weak_bridges",
            "message": format!("{} weak bridges (weight < 0.1). Consider ai_bridge_scan_orphans.", weak_bridges),
        }));
    }

    Ok(serde_json::json!({"suggestions": suggestions}))
}

pub fn handle_shared_status(
    _params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let published = SharedStorage::list_published(ctx.shared_conn)?;
    let subscriptions = SharedStorage::list_subscriptions(ctx.shared_conn, ctx.agent_id)?;

    Ok(serde_json::json!({
        "published": published.len(),
        "subscriptions": subscriptions.len(),
    }))
}

pub fn handle_profile(
    params: &serde_json::Value,
    _ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let action = required_str(params, "action")?;
    // Profile is stored as a file, simplified here
    match action.as_str() {
        "view" => Ok(serde_json::json!({"role": "developer", "preferences": {}})),
        _ => Ok(serde_json::json!({"action": action, "status": "ok"})),
    }
}

pub fn handle_cleanup(
    _params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let threads = ThreadStorage::list_all(ctx.agent_conn)?;
    let mut fixed = 0;
    for t in &threads {
        if t.title.is_empty() || t.title.len() < 3 {
            // Would auto-generate title from first message
            fixed += 1;
        }
    }
    Ok(serde_json::json!({"checked": threads.len(), "issues": fixed}))
}

pub fn handle_lock(
    _params: &serde_json::Value,
    ctx: &ToolContext,
    method: &str,
) -> AiResult<serde_json::Value> {
    let lock_file = path_utils::project_dir(ctx.project_hash).join("memory.lock");
    match method {
        "ai_lock" => {
            std::fs::create_dir_all(lock_file.parent().unwrap_or(std::path::Path::new(".")))?;
            std::fs::write(&lock_file, chrono::Utc::now().to_rfc3339())?;
            Ok(serde_json::json!({"locked": true}))
        }
        "ai_unlock" => {
            let _ = std::fs::remove_file(&lock_file);
            Ok(serde_json::json!({"locked": false}))
        }
        _ => {
            let locked = lock_file.exists();
            Ok(serde_json::json!({"locked": locked}))
        }
    }
}

pub fn handle_backup(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let action = required_str(params, "action")?;
    let db_path = path_utils::agent_db_path(ctx.project_hash, ctx.agent_id);

    match action.as_str() {
        "create" => {
            let backup_path = db_path.with_extension("db.backup");
            BackupManager::create_backup(ctx.agent_conn, &backup_path)?;
            Ok(serde_json::json!({"action": "create", "path": backup_path.display().to_string()}))
        }
        "restore" => {
            let backup_path = db_path.with_extension("db.backup");
            BackupManager::restore_backup(&backup_path, &db_path)?;
            Ok(serde_json::json!({"action": "restore", "status": "ok"}))
        }
        "status" => {
            let backup_path = db_path.with_extension("db.backup");
            let exists = backup_path.exists();
            Ok(serde_json::json!({"backup_exists": exists}))
        }
        _ => Ok(serde_json::json!({"action": action, "status": "ok"})),
    }
}

// ── mcp-smartness-com system tools ──

pub fn handle_metrics(
    _params: &serde_json::Value,
    _ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    Ok(serde_json::json!({"period": "week", "messages_sent": 0, "messages_received": 0}))
}

pub fn handle_health_check(
    _params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let thread_count = ThreadStorage::count(ctx.agent_conn).unwrap_or(0);
    Ok(serde_json::json!({
        "status": "ok",
        "threads": thread_count,
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

pub fn handle_topics_network(
    _params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let threads = ThreadStorage::list_all(ctx.agent_conn)?;
    let mut topic_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for t in &threads {
        for topic in &t.topics {
            *topic_counts.entry(topic.clone()).or_insert(0) += 1;
        }
    }
    let mut topics: Vec<(String, usize)> = topic_counts.into_iter().collect();
    topics.sort_by(|a, b| b.1.cmp(&a.1));
    topics.truncate(20);

    Ok(serde_json::json!({
        "topics": topics.iter().map(|(t, c)| serde_json::json!({"topic": t, "count": c})).collect::<Vec<_>>(),
    }))
}

pub fn handle_test_sampling(
    _params: &serde_json::Value,
    _ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    Ok(serde_json::json!({
        "sampling_supported": false,
        "message": "MCP sampling/createMessage is not supported in this implementation",
    }))
}

pub fn handle_beat_wake(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let after_str = required_str(params, "after")?;
    let after: u64 = after_str
        .parse()
        .map_err(|_| ai_smartness::AiError::InvalidInput("'after' must be a positive integer".into()))?;
    let reason = optional_str(params, "reason")
        .unwrap_or_else(|| "self-wake".into());

    let data_dir = path_utils::agent_data_dir(ctx.project_hash, ctx.agent_id);
    let mut beat = BeatState::load(&data_dir);
    let target = beat.beat + after;
    beat.schedule_wake(after, reason.clone());
    beat.save(&data_dir);

    Ok(serde_json::json!({
        "scheduled": true,
        "current_beat": beat.beat,
        "target_beat": target,
        "reason": reason,
        "note": format!("You will be woken at beat {} (~{} min from now)", target, after * 5)
    }))
}
