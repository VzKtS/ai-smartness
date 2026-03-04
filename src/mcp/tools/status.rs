use ai_smartness::agent::TaskStatus;
use ai_smartness::registry::tasks::AgentTaskStorage;
use ai_smartness::storage::backup::BackupManager;
use ai_smartness::storage::beat::BeatState;
use ai_smartness::storage::bridges::BridgeStorage;
use ai_smartness::storage::cognitive_inbox::CognitiveInbox;
use ai_smartness::storage::mcp_messages::McpMessages;
use ai_smartness::storage::path_utils;
use ai_smartness::storage::shared_storage::SharedStorage;
use ai_smartness::storage::threads::ThreadStorage;
use ai_smartness::thread::ThreadStatus;
use ai_smartness::user_profile::UserProfile;
use ai_smartness::AiResult;

use super::{optional_str, required_str, ToolContext};

pub fn handle_status(
    _params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let active = ThreadStorage::count_by_status(ctx.agent_conn, &ThreadStatus::Active)?;
    let suspended = ThreadStorage::count_by_status(ctx.agent_conn, &ThreadStatus::Suspended)?;
    let archived = ThreadStorage::count_by_status(ctx.agent_conn, &ThreadStatus::Archived)?;
    let bridges = BridgeStorage::count(ctx.agent_conn)?;

    let data_dir = path_utils::agent_data_dir(ctx.project_hash, ctx.agent_id);
    let beat = BeatState::load(&data_dir);

    // Profile
    let profile = UserProfile::load(&data_dir);
    let profile_json = serde_json::json!({
        "identity": {
            "role": profile.identity.role,
            "relationship": profile.identity.relationship,
            "name": profile.identity.name,
        },
        "preferences": {
            "language": profile.preferences.language,
            "verbosity": profile.preferences.verbosity,
            "emoji_usage": profile.preferences.emoji_usage,
            "technical_level": profile.preferences.technical_level,
        },
        "rules": profile.context_rules.iter().collect::<Vec<_>>(),
        "rules_count": profile.context_rules.len(),
    });

    // Pins, focus, top threads (single list_all pass)
    let all_threads = ThreadStorage::list_all(ctx.agent_conn)?;

    let pins: Vec<serde_json::Value> = all_threads
        .iter()
        .filter(|t| t.tags.contains(&"__pin__".to_string()) && t.status == ThreadStatus::Active)
        .take(10)
        .map(|t| {
            let content = ThreadStorage::get_messages(ctx.agent_conn, &t.id)
                .ok()
                .and_then(|msgs| msgs.first().map(|m| m.content.clone()));
            serde_json::json!({
                "id": t.id, "title": t.title, "weight": t.weight,
                "topics": t.topics, "content": content,
            })
        })
        .collect();

    let focus: Vec<serde_json::Value> = all_threads
        .iter()
        .filter(|t| t.tags.contains(&"__focus__".to_string()) && t.status == ThreadStatus::Active)
        .map(|t| {
            serde_json::json!({
                "topic": t.topics.first().unwrap_or(&t.title),
                "weight": t.weight,
            })
        })
        .collect();

    let top_threads: Vec<serde_json::Value> = all_threads
        .iter()
        .filter(|t| {
            t.status == ThreadStatus::Active
                && !t.tags.contains(&"__pin__".to_string())
                && !t.tags.contains(&"__focus__".to_string())
        })
        .take(5)
        .map(|t| {
            serde_json::json!({
                "id": t.id, "title": t.title, "weight": t.weight,
                "topics": t.topics, "labels": t.labels,
            })
        })
        .collect();

    // Pending tasks
    let pending_tasks = AgentTaskStorage::list_tasks_for_agent(
        ctx.registry_conn,
        ctx.agent_id,
        ctx.project_hash,
    )
    .unwrap_or_default()
    .iter()
    .filter(|t| matches!(t.status, TaskStatus::Pending | TaskStatus::InProgress))
    .count();

    // Pending messages
    let pending_messages = McpMessages::count_pending(ctx.shared_conn, ctx.agent_id).unwrap_or(0)
        + CognitiveInbox::count_pending(ctx.agent_conn, ctx.agent_id).unwrap_or(0);

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
        "system_metrics": beat.system_metrics,
        "llm_status": beat.llm_status,
        "llm_backend": beat.llm_backend,
        "llm_ctx_size": beat.llm_ctx_size,
        "llm_gpu_layers": beat.llm_gpu_layers,
        "profile": profile_json,
        "pins": pins,
        "focus": focus,
        "top_threads": top_threads,
        "pending_tasks": pending_tasks,
        "pending_messages": pending_messages,
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
            "Reminder & Context (ai_status → full context read, ai_profile set → edit identity/preferences, ai_profile set_rule → edit rules, ai_pin → pin content, ai_focus/ai_unfocus → prioritize topics)",
            "Memory & Search (ai_recall)",
            "Thread Lifecycle (ai_thread_create, ai_thread_rm, ai_thread_rm_batch, ai_thread_list, ai_thread_search, ai_thread_activate, ai_thread_suspend, ai_reactivate)",
            "Thread Operations (ai_merge, ai_merge_batch, ai_split, ai_split_unlock)",
            "Thread Metadata (ai_label, ai_labels_suggest, ai_rename, ai_rename_batch, ai_rate_importance, ai_rate_context)",
            "Bridges (ai_bridges, ai_bridge_analysis, ai_bridge_scan_orphans, ai_bridge_kill, ai_bridge_kill_batch)",
            "Cognitive Messaging (ai_msg_focus, ai_msg_ack)",
            "Shared Cognition (ai_share, ai_unshare, ai_publish, ai_discover, ai_subscribe, ai_unsubscribe, ai_sync, ai_shared_status, ai_recommend)",
            "System & Status (ai_sysinfo, ai_help, ai_suggestions, ai_topics, health_check, topics_network, metrics_cross_agent, test_sampling)",
            "Maintenance (ai_cleanup, ai_lock, ai_unlock, ai_lock_status, ai_backup, beat_wake)",
            "Agent Management (ai_agent_select, agent_list, agent_query, agent_status, agent_cleanup, agent_configure, agent_tasks, task_delegate, task_status, task_complete)",
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
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let action = required_str(params, "action")?;
    let data_dir = path_utils::agent_data_dir(ctx.project_hash, ctx.agent_id);
    let mut profile = ai_smartness::user_profile::UserProfile::load(&data_dir);

    match action.as_str() {
        "view" => {
            let rules: Vec<serde_json::Value> = profile.context_rules.iter().enumerate()
                .map(|(i, r)| serde_json::json!({"index": i + 1, "rule": r}))
                .collect();
            Ok(serde_json::json!({
                "identity": {
                    "role": profile.identity.role,
                    "relationship": profile.identity.relationship,
                    "name": profile.identity.name,
                },
                "preferences": {
                    "language": profile.preferences.language,
                    "verbosity": profile.preferences.verbosity,
                    "emoji_usage": profile.preferences.emoji_usage,
                    "technical_level": profile.preferences.technical_level,
                },
                "rules": rules,
                "rules_count": profile.context_rules.len(),
            }))
        }
        "set_rule" => {
            let value = required_str(params, "value")?;
            let added = profile.add_rule(value.clone());
            profile.save(&data_dir);
            Ok(serde_json::json!({
                "action": "set_rule",
                "value": value,
                "added": added,
                "rules_count": profile.context_rules.len(),
            }))
        }
        "remove_rule" => {
            let key = required_str(params, "key")?;
            let idx: usize = key.parse::<usize>()
                .map_err(|_| ai_smartness::AiError::InvalidInput("key must be a number (1-based index)".into()))?
                .saturating_sub(1);
            let removed = profile.remove_rule(idx);
            profile.save(&data_dir);
            Ok(serde_json::json!({
                "action": "remove_rule",
                "index": key,
                "removed": removed,
                "rules_count": profile.context_rules.len(),
            }))
        }
        "list" => {
            let rules: Vec<serde_json::Value> = profile.context_rules.iter().enumerate()
                .map(|(i, r)| serde_json::json!({"index": i + 1, "rule": r}))
                .collect();
            Ok(serde_json::json!({
                "rules": rules,
                "rules_count": profile.context_rules.len(),
            }))
        }
        "clear_rules" => {
            profile.clear_rules();
            profile.save(&data_dir);
            Ok(serde_json::json!({"action": "clear_rules", "rules_count": 0}))
        }
        "set" => {
            let key = required_str(params, "key")?;
            let value = required_str(params, "value")?;
            match key.as_str() {
                "name" => profile.identity.name = if value.is_empty() { None } else { Some(value.clone()) },
                "role" => profile.identity.role = value.clone(),
                "relationship" => profile.identity.relationship = value.clone(),
                "language" => profile.preferences.language = value.clone(),
                "verbosity" => profile.preferences.verbosity = value.clone(),
                "emoji_usage" => profile.preferences.emoji_usage = matches!(value.as_str(), "true" | "1" | "yes"),
                "technical_level" => profile.preferences.technical_level = value.clone(),
                _ => return Ok(serde_json::json!({
                    "action": "set",
                    "error": format!("Unknown key: {}. Valid: name, role, relationship, language, verbosity, emoji_usage, technical_level", key),
                })),
            }
            profile.save(&data_dir);
            Ok(serde_json::json!({"action": "set", "key": key, "value": value, "saved": true}))
        }
        _ => Ok(serde_json::json!({"action": action, "error": "unknown action — use view, set, set_rule, remove_rule, list, or clear_rules"})),
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
