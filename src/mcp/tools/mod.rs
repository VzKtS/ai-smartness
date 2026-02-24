pub mod agents;
pub mod bridges;
pub mod discover;
pub mod focus;
pub mod merge;
pub mod messaging;
pub mod recall;
pub mod share;
pub mod split;
pub mod status;
pub mod threads;
pub mod windows;

use ai_smartness::AiResult;
use ai_smartness::registry::heartbeat::Heartbeat;
use ai_smartness::registry::registry::AgentRegistry;
use ai_smartness::storage::beat::BeatState;
use ai_smartness::storage::path_utils;
use ai_smartness::storage::threads::ThreadStorage;
use rusqlite::Connection;

/// Shared context passed to every tool handler.
pub struct ToolContext<'a> {
    pub agent_conn: &'a Connection,
    pub registry_conn: &'a Connection,
    pub shared_conn: &'a Connection,
    pub project_hash: &'a str,
    pub agent_id: &'a str,
}

/// Result of a tool invocation, optionally carrying a side-effect
/// that the server loop must apply after sending the response.
pub enum ToolOutput {
    /// Normal result, no side-effects.
    Plain(serde_json::Value),
    /// Result + request to switch the active agent in-memory.
    AgentSwitch {
        result: serde_json::Value,
        new_agent_id: String,
    },
}

/// Route a tool call by name to the appropriate handler.
pub fn route_tool(
    name: &str,
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<ToolOutput> {
    tracing::info!(tool = %name, "MCP tool called");

    // Update current_activity in registry (non-critical)
    if let Err(e) = Heartbeat::update(ctx.registry_conn, ctx.agent_id, ctx.project_hash, Some(&format!("tool:{}", name))) {
        tracing::debug!(error = %e, "Activity update failed (non-critical)");
    }

    // Tools that produce side-effects
    let result = match name {
        "ai_agent_select" => agents::handle_agent_select(params, ctx),
        _ => route_plain_tool(name, params, ctx).map(ToolOutput::Plain),
    };

    // E5/E6: Update beat.json with tool call count and error tracking
    {
        let data_dir = path_utils::agent_data_dir(ctx.project_hash, ctx.agent_id);
        let mut beat = BeatState::load(&data_dir);
        beat.record_tool_call();
        if let Err(ref e) = result {
            beat.record_error(&e.to_string());
        }
        beat.save(&data_dir);
    }

    match &result {
        Ok(_) => tracing::debug!(tool = %name, "MCP tool success"),
        Err(e) => tracing::warn!(tool = %name, error = %e, "MCP tool error"),
    }
    result
}

/// Route all plain tools (no side-effects). Handlers remain unchanged.
fn route_plain_tool(
    name: &str,
    params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    match name {
        // -- Memory & Search --
        "ai_recall" => recall::handle_recall(params, ctx),

        // -- Thread lifecycle --
        "ai_thread_create" => threads::handle_thread_create(params, ctx),
        "ai_thread_rm" => threads::handle_thread_rm(params, ctx),
        "ai_thread_rm_batch" => threads::handle_thread_rm_batch(params, ctx),
        "ai_thread_list" => threads::handle_thread_list(params, ctx),
        "ai_thread_search" => threads::handle_thread_search(params, ctx),
        "ai_thread_activate" => threads::handle_thread_activate(params, ctx),
        "ai_thread_suspend" => threads::handle_thread_suspend(params, ctx),
        "ai_thread_purge" => threads::handle_thread_purge(params, ctx),
        "ai_reactivate" => threads::handle_reactivate(params, ctx),

        // -- Thread operations --
        "ai_merge" => merge::handle_merge(params, ctx),
        "ai_merge_batch" => merge::handle_merge_batch(params, ctx),
        "ai_split" => split::handle_split(params, ctx),
        "ai_split_unlock" => split::handle_split_unlock(params, ctx),

        // -- Thread metadata --
        "ai_label" => threads::handle_label(params, ctx),
        "ai_labels_suggest" => threads::handle_labels_suggest(params, ctx),
        "ai_concepts" => threads::handle_concepts(params, ctx),
        "ai_backfill_concepts" => threads::handle_backfill_concepts(params, ctx),
        "ai_rename" => threads::handle_rename(params, ctx),
        "ai_rename_batch" => threads::handle_rename_batch(params, ctx),
        "ai_rate_importance" => threads::handle_rate_importance(params, ctx),
        "ai_rate_context" => threads::handle_rate_context(params, ctx),
        "ai_mark_used" => threads::handle_mark_used(params, ctx),

        // -- Bridges --
        "ai_bridges" => bridges::handle_bridges(params, ctx),
        "ai_bridge_analysis" => bridges::handle_bridge_analysis(params, ctx),
        "ai_bridge_scan_orphans" => bridges::handle_bridge_scan_orphans(params, ctx),
        "ai_bridge_purge" => bridges::handle_bridge_purge(params, ctx),
        "ai_bridge_kill" => bridges::handle_bridge_kill(params, ctx),
        "ai_bridge_kill_batch" => bridges::handle_bridge_kill_batch(params, ctx),

        // -- Focus & Pins --
        "ai_focus" => focus::handle_focus(params, ctx),
        "ai_unfocus" => focus::handle_unfocus(params, ctx),
        "ai_pin" => focus::handle_pin(params, ctx),

        // -- Cognitive Messaging --
        "ai_msg_focus" => messaging::handle_msg_focus(params, ctx),
        "ai_msg_ack" => messaging::handle_msg_ack(params, ctx),

        // -- Shared Cognition --
        "ai_share" => share::handle_share(params, ctx),
        "ai_unshare" => share::handle_unshare(params, ctx),
        "ai_publish" => share::handle_publish(params, ctx),
        "ai_discover" => discover::handle_discover(params, ctx),
        "ai_subscribe" => discover::handle_subscribe(params, ctx),
        "ai_unsubscribe" => discover::handle_unsubscribe(params, ctx),
        "ai_sync" => discover::handle_sync(params, ctx),

        // -- System & Status --
        "ai_status" => status::handle_status(params, ctx),
        "ai_sysinfo" => status::handle_sysinfo(params, ctx),
        "ai_help" => status::handle_help(params, ctx),
        "ai_suggestions" => status::handle_suggestions(params, ctx),
        "ai_shared_status" => status::handle_shared_status(params, ctx),
        "ai_profile" => status::handle_profile(params, ctx),

        // -- Maintenance --
        "ai_cleanup" => status::handle_cleanup(params, ctx),
        "ai_lock" | "ai_unlock" | "ai_lock_status" => status::handle_lock(params, ctx, name),
        "ai_backup" => status::handle_backup(params, ctx),

        // -- mcp-smartness-com: Messaging --
        "msg_send" => messaging::handle_msg_send(params, ctx),
        "msg_broadcast" => messaging::handle_msg_broadcast(params, ctx),
        "msg_inbox" => messaging::handle_msg_inbox(params, ctx),
        "msg_reply" => messaging::handle_msg_reply(params, ctx),

        // -- mcp-smartness-com: Agents --
        "agent_list" => agents::handle_agent_list(params, ctx),
        "agent_query" => agents::handle_agent_query(params, ctx),
        "agent_status" => agents::handle_agent_status(params, ctx),
        "agent_cleanup" => agents::handle_agent_cleanup(params, ctx),
        "agent_configure" => agents::handle_agent_configure(params, ctx),
        "agent_tasks" => agents::handle_agent_tasks(params, ctx),

        // -- mcp-smartness-com: Tasks --
        "task_delegate" => agents::handle_task_delegate(params, ctx),
        "task_status" => agents::handle_task_status(params, ctx),

        // -- mcp-smartness-com: Metrics & Health --
        "metrics_cross_agent" => status::handle_metrics(params, ctx),
        "health_check" => status::handle_health_check(params, ctx),
        "topics_network" => status::handle_topics_network(params, ctx),
        "test_sampling" => status::handle_test_sampling(params, ctx),

        // -- Beat / Self-wake --
        "beat_wake" => status::handle_beat_wake(params, ctx),

        // -- Windows --
        "ai_windows" => windows::handle_windows(params, ctx),

        // -- ai-smartness aliases (defined in tool_definitions) --
        "ai_recommend" => discover::handle_recommend(params, ctx),
        "ai_topics" => status::handle_topics_network(params, ctx),

        _ => Err(ai_smartness::AiError::InvalidInput(format!(
            "Unknown tool: {}",
            name
        ))),
    }
}

// ── Quota guard ──

/// Returns (active_count, quota). Fallback quota = 50 if agent not found.
pub fn check_thread_quota(ctx: &ToolContext) -> AiResult<(usize, usize)> {
    let quota = AgentRegistry::get(ctx.registry_conn, ctx.agent_id, ctx.project_hash)
        .ok()
        .flatten()
        .map(|a| a.thread_mode.quota())
        .unwrap_or(50);
    let active = ThreadStorage::count_by_status(
        ctx.agent_conn,
        &ai_smartness::thread::ThreadStatus::Active,
    )?;
    Ok((active, quota))
}

// ── Parameter extraction helpers ──

pub fn required_str(params: &serde_json::Value, key: &str) -> AiResult<String> {
    params
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| {
            ai_smartness::AiError::InvalidInput(format!("Missing required parameter: {}", key))
        })
}

pub fn optional_str(params: &serde_json::Value, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

pub fn optional_bool(params: &serde_json::Value, key: &str) -> Option<bool> {
    params.get(key).and_then(|v| {
        v.as_bool().or_else(|| match v.as_str() {
            Some("true" | "1" | "yes") => Some(true),
            Some("false" | "0" | "no") => Some(false),
            _ => None,
        })
    })
}

pub fn optional_f64(params: &serde_json::Value, key: &str) -> Option<f64> {
    params.get(key).and_then(|v| {
        v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))
    })
}

pub fn optional_usize(params: &serde_json::Value, key: &str) -> Option<usize> {
    params.get(key).and_then(|v| {
        v.as_u64()
            .map(|n| n as usize)
            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
    })
}

#[allow(dead_code)]
pub fn optional_i64(params: &serde_json::Value, key: &str) -> Option<i64> {
    params.get(key).and_then(|v| {
        v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))
    })
}

pub fn required_array(params: &serde_json::Value, key: &str) -> AiResult<Vec<String>> {
    params
        .get(key)
        .and_then(|v| parse_string_or_array(v))
        .ok_or_else(|| {
            ai_smartness::AiError::InvalidInput(format!("Missing required array: {}", key))
        })
}

pub fn optional_array(params: &serde_json::Value, key: &str) -> Option<Vec<String>> {
    params.get(key).and_then(|v| parse_string_or_array(v))
}

/// Parse a value that may be a JSON array or a string containing a JSON array
/// or comma-separated values. MCP tool schemas declare all params as "string",
/// so array values arrive as strings that need parsing.
pub fn parse_string_or_array(v: &serde_json::Value) -> Option<Vec<String>> {
    // Case 1: native JSON array
    if let Some(arr) = v.as_array() {
        return Some(
            arr.iter()
                .filter_map(|item| item.as_str().map(String::from))
                .collect(),
        );
    }
    // Case 2: string — try JSON parse, then comma-separated
    if let Some(s) = v.as_str() {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Some(vec![]);
        }
        // Try parsing as JSON array
        if trimmed.starts_with('[') {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(trimmed) {
                if let Some(arr) = parsed.as_array() {
                    return Some(
                        arr.iter()
                            .map(|item| {
                                item.as_str()
                                    .map(String::from)
                                    .unwrap_or_else(|| item.to_string())
                            })
                            .collect(),
                    );
                }
            }
        }
        // Comma-separated fallback
        return Some(
            trimmed
                .split(',')
                .map(|part| part.trim().to_string())
                .filter(|part| !part.is_empty())
                .collect(),
        );
    }
    None
}

/// Parse a JSON value that may be a native array or a string containing JSON array
/// of objects. Used for batch operations (rename_batch, merge_batch).
pub fn parse_object_array(v: &serde_json::Value) -> Option<Vec<serde_json::Value>> {
    // Case 1: native JSON array
    if let Some(arr) = v.as_array() {
        return Some(arr.clone());
    }
    // Case 2: string containing JSON array
    if let Some(s) = v.as_str() {
        let trimmed = s.trim();
        if let Ok(serde_json::Value::Array(arr)) = serde_json::from_str::<serde_json::Value>(trimmed) {
            return Some(arr);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use ai_smartness::agent::{Agent, AgentStatus, CoordinationMode, ThreadMode};
    use ai_smartness::registry::registry::AgentRegistry;
    use ai_smartness::storage::threads::ThreadStorage;
    use ai_smartness::thread::ThreadStatus;
    use rusqlite::{params, Connection};

    const PH: &str = "test-ph-mod";
    const AGENT: &str = "test-agent-mod";

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
        };
        AgentRegistry::register(conn, &agent).unwrap();
    }

    fn insert_active_threads(conn: &Connection, count: usize) {
        for i in 0..count {
            let t = ai_smartness::thread::Thread {
                id: format!("t-mod-{}", i),
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
                created_at: chrono::Utc::now(),
                last_active: chrono::Utc::now(),
            };
            ThreadStorage::insert(conn, &t).unwrap();
        }
    }

    /// T-Q2.1: check_thread_quota returns (active_count, quota) for Heavy mode with 5 threads.
    #[test]
    fn test_check_thread_quota_returns_count_and_quota() {
        let agent_conn = setup_agent_db();
        let registry_conn = setup_registry_db();
        let shared_conn = setup_shared_db();

        insert_project(&registry_conn);
        register_agent(&registry_conn, ThreadMode::Heavy);
        insert_active_threads(&agent_conn, 5);

        let ctx = ToolContext {
            agent_conn: &agent_conn,
            registry_conn: &registry_conn,
            shared_conn: &shared_conn,
            project_hash: PH,
            agent_id: AGENT,
        };

        let (active, quota) = check_thread_quota(&ctx).unwrap();
        assert_eq!(active, 5, "Should count 5 active threads");
        assert_eq!(quota, 100, "Heavy mode quota should be 100");
    }

    /// T-C2.3: route_tool sets current_activity to "tool:<name>".
    #[test]
    fn test_route_tool_sets_activity() {
        let agent_conn = setup_agent_db();
        let registry_conn = setup_registry_db();
        let shared_conn = setup_shared_db();

        insert_project(&registry_conn);
        register_agent(&registry_conn, ThreadMode::Normal);

        let ctx = ToolContext {
            agent_conn: &agent_conn,
            registry_conn: &registry_conn,
            shared_conn: &shared_conn,
            project_hash: PH,
            agent_id: AGENT,
        };

        // Call route_tool with ai_thread_list (the tool itself may error, but
        // the activity update happens before dispatch, so we ignore the result).
        let _ = route_tool("ai_thread_list", &serde_json::json!({}), &ctx);

        let activity: String = registry_conn.query_row(
            "SELECT current_activity FROM agents WHERE id = ?1 AND project_hash = ?2",
            params![AGENT, PH],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(activity, "tool:ai_thread_list");
    }
}
