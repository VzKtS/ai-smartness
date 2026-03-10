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

    // Hardware detection
    let hw = ai_smartness::processing::hardware::detect();
    let gpus: Vec<serde_json::Value> = hw.gpus.iter().map(|g| {
        serde_json::json!({
            "index": g.index,
            "name": g.name,
            "vendor": g.vendor,
            "vram_total_mb": g.vram_total_mb,
            "vram_used_mb": g.vram_used_mb,
        })
    }).collect();

    Ok(serde_json::json!({
        "threads": total_threads,
        "bridges": total_bridges,
        "disk_usage_bytes": disk_usage,
        "embedding_backend": "tfidf_hash",
        "version": env!("CARGO_PKG_VERSION"),
        "hardware": {
            "cpu": { "model": hw.cpu.model, "cores": hw.cpu.cores, "threads": hw.cpu.threads },
            "ram": { "total_mb": hw.ram.total_mb, "available_mb": hw.ram.available_mb },
            "gpus": gpus,
        },
    }))
}

pub fn handle_help(
    params: &serde_json::Value,
    _ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let topic = optional_str(params, "topic");

    match topic.as_deref() {
        Some("memory") => Ok(help_memory()),
        Some("threads") => Ok(help_threads()),
        Some("bridges") => Ok(help_bridges()),
        Some("messaging") => Ok(help_messaging()),
        Some("sharing") => Ok(help_sharing()),
        Some("agents") => Ok(help_agents()),
        Some("tasks") => Ok(help_tasks()),
        Some("maintenance") => Ok(help_maintenance()),
        Some("autonomy") => Ok(help_autonomy()),
        _ => Ok(help_overview()),
    }
}

fn help_overview() -> serde_json::Value {
    serde_json::json!({
        "name": "AI Smartness",
        "version": env!("CARGO_PKG_VERSION"),
        "tool_count": 69,
        "usage": "ai_help(topic=\"memory\") for detailed help per category",
        "categories": {
            "memory": "Memory & Search — ai_recall, ai_focus, ai_unfocus, ai_pin",
            "threads": "Thread Lifecycle & Operations — create, list, search, split, annotate, label, rename, rate",
            "bridges": "Bridges — ai_bridges, ai_bridge_analysis, ai_bridge_scan_orphans, ai_bridge_kill",
            "messaging": "Messaging — msg_send, msg_broadcast, msg_inbox, msg_reply, ai_msg_focus, ai_msg_ack",
            "sharing": "Shared Cognition — ai_share, ai_publish, ai_discover, ai_subscribe, ai_sync",
            "agents": "Agent Management — ai_agent_select, agent_list, agent_query, agent_status, agent_context, agent_configure",
            "tasks": "Task Delegation — task_delegate, task_status, task_complete, agent_tasks",
            "maintenance": "Maintenance & System — ai_cleanup, ai_backup, ai_lock, ai_sysinfo, health_check",
            "autonomy": "Autonomous Task Chaining — nanobeat_schedule, beat_wake",
        },
        "quick_ref": [
            "ai_status → full context snapshot (beat, threads, pins, focus, profile)",
            "ai_recall(query) → semantic search [depth=deep for inline messages, freshness score]",
            "ai_help(topic) → detailed help per category",
            "ai_profile(action=set, key, value) → edit identity/preferences",
            "ai_profile(action=set_rule, value) → add persistent rule",
            "nanobeat_schedule(delay_seconds, reason) → self-wake for task chaining",
        ],
    })
}

fn help_memory() -> serde_json::Value {
    serde_json::json!({
        "category": "Memory & Search",
        "tools": {
            "ai_recall": {
                "description": "Semantic search across all threads",
                "required": ["query"],
                "optional": ["label", "include_bridges", "depth"],
                "notes": "depth=deep includes first 3 messages (500 char cap). Every result includes a freshness score (1.0=fresh, 0.0=stale).",
            },
            "ai_focus": {
                "description": "Read full thread content (all messages)",
                "required": ["thread_id"],
                "optional": [],
            },
            "ai_unfocus": {
                "description": "Remove focus tag from a thread",
                "required": ["thread_id"],
                "optional": [],
            },
            "ai_pin": {
                "description": "Pin a thread for persistent visibility in status",
                "required": ["thread_id"],
                "optional": ["content"],
                "notes": "Pinned threads appear in ai_status output.",
            },
            "ai_status": {
                "description": "Full context snapshot — beat, threads, pins, focus, profile, tasks, messages",
                "required": [],
                "optional": [],
            },
            "ai_profile": {
                "description": "View/edit agent identity, preferences, and rules",
                "required": ["action"],
                "optional": ["key", "value"],
                "actions": "view, set, set_rule, remove_rule, list, clear_rules",
            },
        },
    })
}

fn help_threads() -> serde_json::Value {
    serde_json::json!({
        "category": "Thread Lifecycle & Operations",
        "tools": {
            "ai_thread_create": {
                "description": "Create a new thread manually",
                "required": ["title", "content"],
                "optional": ["topics", "labels", "tags", "importance"],
                "notes": "Use tags=[\"__mind__\"] for reasoning savepoints.",
            },
            "ai_thread_list": {
                "description": "List threads with optional filters",
                "required": [],
                "optional": ["status", "limit", "offset", "sort"],
            },
            "ai_thread_search": {
                "description": "Search threads by title/topic keyword",
                "required": ["query"],
                "optional": ["status"],
            },
            "ai_thread_rm": { "description": "Delete a thread", "required": ["thread_id"] },
            "ai_thread_rm_batch": { "description": "Delete multiple threads", "required": ["thread_ids"] },
            "ai_thread_activate": { "description": "Reactivate a suspended thread", "required": ["thread_id"] },
            "ai_thread_suspend": { "description": "Suspend a thread (keeps data, removes from active)", "required": ["thread_id"] },
            "ai_thread_purge": { "description": "Permanently purge archived threads", "required": [] },
            "ai_reactivate": { "description": "Reactivate multiple suspended threads by label/topic", "required": ["filter"] },
            "ai_continuity_edges": { "description": "Show continuity chain for a thread", "required": ["thread_id"] },
            "ai_split": {
                "description": "Split a thread into two based on topic divergence",
                "required": ["thread_id"],
                "optional": ["reason"],
            },
            "ai_split_unlock": { "description": "Unlock a split-locked thread", "required": ["thread_id"] },
            "ai_annotate": {
                "description": "Add a lightweight note to a thread (no LLM, no extraction)",
                "required": ["thread_id", "note"],
                "notes": "Zero-cost housekeeping — mark threads as obsolete, add context notes.",
            },
            "ai_label": { "description": "Add/remove labels on a thread", "required": ["thread_id", "labels"] },
            "ai_labels_suggest": { "description": "Suggest labels for a thread based on content", "required": ["thread_id"] },
            "ai_rename": { "description": "Rename a thread", "required": ["thread_id", "title"] },
            "ai_rename_batch": { "description": "Rename multiple threads", "required": ["operations"] },
            "ai_rate_importance": { "description": "Manually set thread importance (0.0-1.0)", "required": ["thread_id", "importance"] },
            "ai_rate_context": { "description": "Rate thread context relevance", "required": ["thread_id", "rating"] },
            "ai_mark_used": { "description": "Mark a thread as recently used (boosts weight)", "required": ["thread_id"] },
            "ai_concepts": { "description": "View concepts extracted from a thread", "required": ["thread_id"] },
            "ai_backfill_concepts": { "description": "Re-extract concepts for threads missing them", "required": [] },
        },
    })
}

fn help_bridges() -> serde_json::Value {
    serde_json::json!({
        "category": "Bridges",
        "tools": {
            "ai_bridges": { "description": "List bridges for a thread", "required": ["thread_id"], "optional": ["limit"] },
            "ai_bridge_analysis": { "description": "Analyze bridge network for a thread", "required": ["thread_id"] },
            "ai_bridge_scan_orphans": { "description": "Find and optionally remove orphan bridges", "required": [], "optional": ["fix"] },
            "ai_bridge_purge": { "description": "Purge weak bridges below threshold", "required": [], "optional": ["threshold"] },
            "ai_bridge_kill": { "description": "Delete a specific bridge", "required": ["bridge_id"] },
            "ai_bridge_kill_batch": { "description": "Delete multiple bridges", "required": ["bridge_ids"] },
        },
    })
}

fn help_messaging() -> serde_json::Value {
    serde_json::json!({
        "category": "Messaging",
        "tools": {
            "msg_send": {
                "description": "Send a message to another agent",
                "required": ["to", "subject", "content"],
                "optional": ["priority", "reply_to", "attachments"],
                "notes": "Attachments are file paths resolved to content at send time. reply_to chains conversations.",
            },
            "msg_broadcast": {
                "description": "Send a message to all agents in the project",
                "required": ["subject", "content"],
                "optional": ["priority"],
            },
            "msg_inbox": {
                "description": "Read pending messages",
                "required": [],
                "optional": ["as_agent"],
                "notes": "Auto-acknowledges read messages. Returns in_reply_to for threaded conversations.",
            },
            "msg_reply": {
                "description": "Reply to a message (chains in_reply_to)",
                "required": ["message_id", "content"],
                "optional": ["attachments"],
            },
            "ai_msg_focus": {
                "description": "Read cognitive inbox messages (internal system messages)",
                "required": [],
                "optional": [],
            },
            "ai_msg_ack": {
                "description": "Acknowledge a cognitive inbox message",
                "required": ["message_id"],
                "optional": [],
            },
        },
    })
}

fn help_sharing() -> serde_json::Value {
    serde_json::json!({
        "category": "Shared Cognition",
        "tools": {
            "ai_share": { "description": "Share a thread to the shared knowledge base", "required": ["thread_id"] },
            "ai_unshare": { "description": "Remove a thread from shared knowledge", "required": ["thread_id"] },
            "ai_publish": { "description": "Publish a thread snapshot for cross-agent discovery", "required": ["thread_id"] },
            "ai_discover": { "description": "Discover published threads from other agents", "required": [], "optional": ["query", "agent_id"] },
            "ai_subscribe": { "description": "Subscribe to a published thread for auto-sync", "required": ["thread_id"] },
            "ai_unsubscribe": { "description": "Unsubscribe from a published thread", "required": ["thread_id"] },
            "ai_sync": { "description": "Sync subscribed threads with latest published versions", "required": [] },
            "ai_shared_status": { "description": "Show shared cognition stats (published/subscribed)", "required": [] },
            "ai_recommend": { "description": "Get thread recommendations based on current context", "required": [] },
        },
    })
}

fn help_agents() -> serde_json::Value {
    serde_json::json!({
        "category": "Agent Management",
        "tools": {
            "ai_agent_select": {
                "description": "Switch active agent for this session",
                "required": ["agent_id"],
                "optional": ["session_id"],
            },
            "agent_list": { "description": "List all agents in the project", "required": [] },
            "agent_query": { "description": "Find agents by capability", "required": ["capability"] },
            "agent_status": { "description": "Detailed status of a specific agent", "required": ["agent_id"] },
            "agent_context": {
                "description": "Inspect another agent's runtime state (beat, actions, tasks, context %)",
                "required": [],
                "optional": ["agent_id"],
                "notes": "Defaults to self. Use to check what another agent is doing before delegating.",
            },
            "agent_configure": {
                "description": "Configure agent settings (role, supervisor, team, etc.)",
                "required": ["agent_id"],
                "optional": ["role", "supervisor_id", "team", "thread_mode", "report_to", "custom_role", "workspace_path", "full_permissions", "expected_model"],
            },
            "agent_cleanup": { "description": "Clean up stale agents or remove orphans", "required": [], "optional": ["remove_agent", "remove_orphans"] },
        },
    })
}

fn help_tasks() -> serde_json::Value {
    serde_json::json!({
        "category": "Task Delegation",
        "tools": {
            "task_delegate": {
                "description": "Delegate a task to another agent (sends wake signal)",
                "required": ["to", "task"],
                "optional": ["priority", "context", "context_path"],
                "notes": "Auto-sends wake signal to target agent. Use context for inline details, context_path for file references.",
            },
            "task_status": { "description": "Check status of a delegated task", "required": ["task_id"] },
            "task_complete": {
                "description": "Mark a task as completed (auto-notifies delegator)",
                "required": ["task_id"],
                "optional": ["result"],
                "notes": "Sends completion message to delegator with interrupt wake signal.",
            },
            "agent_tasks": {
                "description": "List/create/update/delete tasks",
                "required": ["action"],
                "optional": ["task_id", "title", "assigned_to", "priority", "status", "result"],
                "actions": "list, create, update_status, complete, delete",
            },
        },
    })
}

fn help_maintenance() -> serde_json::Value {
    serde_json::json!({
        "category": "Maintenance & System",
        "tools": {
            "ai_cleanup": { "description": "Check thread health and fix issues", "required": [] },
            "ai_lock": { "description": "Lock memory (prevent daemon modifications)", "required": [] },
            "ai_unlock": { "description": "Unlock memory", "required": [] },
            "ai_lock_status": { "description": "Check memory lock status", "required": [] },
            "ai_backup": { "description": "Create/restore/check database backup", "required": ["action"], "actions": "create, restore, status" },
            "ai_sysinfo": { "description": "System info — threads, bridges, disk, hardware, GPU", "required": [] },
            "ai_suggestions": { "description": "Get maintenance suggestions (unlabeled threads, weak bridges)", "required": [] },
            "health_check": { "description": "Quick health check", "required": [] },
            "topics_network": { "description": "Top 20 topics by thread count", "required": [] },
            "ai_topics": { "description": "Alias for topics_network", "required": [] },
            "ai_windows": { "description": "List active Claude Code windows/sessions", "required": [] },
            "metrics_cross_agent": { "description": "Cross-agent messaging metrics", "required": [] },
            "test_sampling": { "description": "Test MCP sampling support", "required": [] },
        },
    })
}

fn help_autonomy() -> serde_json::Value {
    serde_json::json!({
        "category": "Autonomous Task Chaining",
        "tools": {
            "nanobeat_schedule": {
                "description": "Schedule a self-wake in N seconds with recall context",
                "required": ["delay_seconds", "reason"],
                "optional": ["recall_query", "recall_thread_id"],
                "notes": "Daemon checks every 1s. On fire, injects a wake prompt with reason + recalled context. Use for multi-step autonomous work.",
            },
            "beat_wake": {
                "description": "Schedule a self-wake in N beats (~5min each)",
                "required": ["after"],
                "optional": ["reason"],
                "notes": "Coarser granularity than nanobeat. Good for deferred follow-ups.",
            },
        },
    })
}

pub fn handle_agent_context(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let target = optional_str(params, "agent_id")
        .unwrap_or_else(|| ctx.agent_id.to_string());

    let data_dir = path_utils::agent_data_dir(ctx.project_hash, &target);
    let beat = BeatState::load(&data_dir);

    // Pending tasks for this agent
    let tasks = AgentTaskStorage::list_tasks_for_agent(
        ctx.registry_conn, &target, ctx.project_hash,
    ).unwrap_or_default();
    let pending_tasks: Vec<serde_json::Value> = tasks.iter()
        .filter(|t| matches!(t.status, TaskStatus::Pending | TaskStatus::InProgress))
        .take(10)
        .map(|t| serde_json::json!({
            "id": t.id, "title": t.title, "status": t.status.as_str(),
            "priority": t.priority.as_str(), "assigned_by": t.assigned_by,
        }))
        .collect();

    // Pending messages
    let pending_messages = McpMessages::count_pending(ctx.shared_conn, &target).unwrap_or(0);

    // Last actions from beat state
    let last_actions: Vec<serde_json::Value> = beat.last_actions.iter()
        .map(|a| serde_json::json!({
            "tool": a.tool, "target": a.target, "at": a.at,
        }))
        .collect();

    Ok(serde_json::json!({
        "agent_id": target,
        "beat": beat.beat,
        "prompt_count": beat.prompt_count,
        "tool_call_count": beat.tool_call_count,
        "context_percent": beat.context_percent,
        "last_session_id": beat.last_session_id,
        "last_interaction_at": beat.last_interaction_at,
        "last_error": beat.last_error,
        "model": beat.model,
        "last_actions": last_actions,
        "pending_tasks": pending_tasks,
        "pending_messages": pending_messages,
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

pub fn handle_nanobeat_schedule(
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    let delay_str = required_str(params, "delay_seconds")?;
    let delay: u64 = delay_str
        .parse()
        .map_err(|_| ai_smartness::AiError::InvalidInput("'delay_seconds' must be a positive integer".into()))?;
    let reason = required_str(params, "reason")?;
    let recall_query = optional_str(params, "recall_query");
    let recall_thread_id = optional_str(params, "recall_thread_id");

    let data_dir = path_utils::agent_data_dir(ctx.project_hash, ctx.agent_id);
    let mut beat = BeatState::load(&data_dir);
    beat.schedule_nanobeat(delay, reason.clone(), recall_query.clone(), recall_thread_id.clone());
    let fire_at = beat.scheduled_nanobeats.last()
        .map(|nb| nb.fire_at.clone())
        .unwrap_or_default();
    beat.save(&data_dir);

    Ok(serde_json::json!({
        "scheduled": true,
        "delay_seconds": delay,
        "fire_at": fire_at,
        "reason": reason,
        "recall_query": recall_query,
        "recall_thread_id": recall_thread_id,
        "note": format!("Nanobeat scheduled: self-wake in {}s with recall context", delay)
    }))
}
