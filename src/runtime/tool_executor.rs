//! Tool executor — bridges Anthropic tool_use to MCP tool handlers.
//!
//! Converts Anthropic tool_use format to MCP tool calls and back.
//! Reuses all existing MCP tool handlers via `route_tool()`.

use crate::mcp::tools::{self, ToolContext, ToolOutput};
use ai_smartness::AiResult;

/// Execute a tool call from the Anthropic API.
///
/// Maps the tool name + input to the existing MCP `route_tool()` dispatcher.
/// Returns the tool output as JSON Value.
pub fn execute_tool(
    name: &str,
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    tracing::info!(tool = %name, "Runtime: executing tool");

    let result = tools::route_tool(name, input, ctx)?;

    match result {
        ToolOutput::Plain(value) => Ok(value),
        ToolOutput::AgentSwitch { result, new_agent_id } => {
            // In the runtime, agent switch would need to be handled at a higher level.
            // For now, return the result and log the switch request.
            tracing::warn!(
                new_agent = %new_agent_id,
                "Agent switch requested via tool — not yet supported in runtime"
            );
            Ok(result)
        }
    }
}

/// Build tool definitions in Anthropic API format.
///
/// Anthropic expects:
/// ```json
/// {
///   "name": "tool_name",
///   "description": "...",
///   "input_schema": { "type": "object", "properties": {...}, "required": [...] }
/// }
/// ```
///
/// Note: Anthropic uses `input_schema`, MCP uses `inputSchema`.
pub fn anthropic_tool_definitions() -> Vec<serde_json::Value> {
    // Reuse the tool definition list from MCP server, but convert the schema key
    let mcp_tools = mcp_tool_defs();

    mcp_tools
        .into_iter()
        .map(|mut tool| {
            // Convert inputSchema → input_schema for Anthropic format
            if let Some(schema) = tool.as_object_mut().and_then(|o| o.remove("inputSchema")) {
                tool.as_object_mut()
                    .unwrap()
                    .insert("input_schema".to_string(), schema);
            }
            tool
        })
        .collect()
}

/// Generate MCP-format tool definitions.
/// Mirrors the definitions in mcp/server.rs tool_definitions().
fn mcp_tool_defs() -> Vec<serde_json::Value> {
    vec![
        tool_def("ai_recall", "Search semantic memory for relevant threads", &["query"], &["label", "include_bridges", "depth"]),
        tool_def("ai_thread_create", "Create a new thread manually", &["title", "content"], &["topics", "importance", "tags"]),
        tool_def("ai_thread_rm", "Delete a thread by ID", &["thread_id"], &[]),
        tool_def("ai_thread_rm_batch", "Delete multiple threads", &["thread_ids"], &[]),
        tool_def("ai_thread_list", "List threads with filters", &[], &["status", "sort_by", "limit", "offset"]),
        tool_def("ai_thread_search", "Search threads across all states", &["query"], &["scope", "states"]),
        tool_def("ai_thread_activate", "Reactivate threads", &["thread_ids"], &["confirm"]),
        tool_def("ai_thread_suspend", "Suspend active threads", &["thread_ids"], &["reason", "confirm"]),
        tool_def("ai_thread_purge", "Bulk delete all threads by status", &["status"], &["confirm"]),
        tool_def("ai_reactivate", "Reactivate a thread by ID", &["thread_id"], &[]),
        tool_def("ai_annotate", "Add a note to a thread", &["thread_id", "note"], &[]),
        tool_def("ai_split", "Split a thread", &["thread_id"], &["confirm", "message_groups", "titles", "lock_mode"]),
        tool_def("ai_split_unlock", "Remove split lock", &["thread_id"], &[]),
        tool_def("ai_label", "Manage labels", &["thread_id"], &["labels", "mode"]),
        tool_def("ai_labels_suggest", "Show existing labels", &["label"], &[]),
        tool_def("ai_concepts", "Manage semantic concepts", &["thread_id"], &["concepts", "mode"]),
        tool_def("ai_backfill_concepts", "Generate concepts for threads missing them", &[], &["limit", "dry_run"]),
        tool_def("ai_rename", "Rename a thread", &["thread_id", "new_title"], &[]),
        tool_def("ai_rename_batch", "Rename multiple threads", &["operations"], &[]),
        tool_def("ai_rate_importance", "Set importance score", &["thread_id", "score"], &["reason"]),
        tool_def("ai_rate_context", "Rate context usefulness", &["thread_id", "useful"], &["reason"]),
        tool_def("ai_mark_used", "Mark thread as used", &["thread_id"], &[]),
        tool_def("ai_continuity_edges", "Manage continuity edges", &[], &["action", "thread_id", "parent_id", "coherence"]),
        tool_def("ai_bridges", "List bridges", &[], &["thread_id", "relation_type", "status"]),
        tool_def("ai_bridge_analysis", "Bridge network analytics", &[], &[]),
        tool_def("ai_bridge_scan_orphans", "Scan orphan bridges", &[], &["confirm"]),
        tool_def("ai_bridge_purge", "Bulk delete bridges by status", &["status"], &["confirm"]),
        tool_def("ai_bridge_kill", "Delete a bridge", &["bridge_id"], &[]),
        tool_def("ai_bridge_kill_batch", "Delete multiple bridges", &["bridge_ids"], &[]),
        tool_def("ai_focus", "Focus on a topic", &["topic"], &["weight"]),
        tool_def("ai_unfocus", "Remove focus", &[], &["topic"]),
        tool_def("ai_pin", "Pin important content", &["content"], &["title", "topics", "weight_boost"]),
        tool_def("ai_msg_focus", "Write cognitive message", &["target_agent_id", "from_agent", "subject", "content"], &["priority", "ttl_minutes", "attachments", "reply_to"]),
        tool_def("ai_msg_ack", "Acknowledge message", &[], &["thread_id", "msg_ref"]),
        tool_def("ai_share", "Share a thread", &["thread_id"], &["visibility", "allowed_agents"]),
        tool_def("ai_unshare", "Unshare a thread", &["shared_id"], &[]),
        tool_def("ai_publish", "Update shared snapshot", &["shared_id"], &[]),
        tool_def("ai_discover", "Discover shared threads", &[], &["topics", "agent_id", "limit"]),
        tool_def("ai_subscribe", "Subscribe to shared thread", &["shared_id"], &[]),
        tool_def("ai_unsubscribe", "Unsubscribe", &["shared_id"], &[]),
        tool_def("ai_sync", "Sync subscriptions", &[], &["shared_id"]),
        tool_def("ai_status", "Memory status", &[], &[]),
        tool_def("ai_sysinfo", "System info", &[], &[]),
        tool_def("ai_help", "Documentation", &[], &["topic"]),
        tool_def("ai_suggestions", "Proactive suggestions", &[], &["context"]),
        tool_def("ai_shared_status", "Shared cognition status", &[], &[]),
        tool_def("ai_profile", "User profile management", &["action"], &["key", "value"]),
        tool_def("ai_cleanup", "Fix thread titles", &[], &["mode", "dry_run"]),
        tool_def("ai_lock", "Lock memory", &[], &["reason", "duration_minutes"]),
        tool_def("ai_unlock", "Unlock memory", &[], &[]),
        tool_def("ai_lock_status", "Lock state", &[], &[]),
        tool_def("ai_backup", "Backup/restore", &["action"], &["interval_hours"]),
        tool_def("ai_recommend", "Subscription recommendations", &[], &["limit"]),
        tool_def("ai_topics", "Topic discovery", &[], &["agent_id"]),
        tool_def("msg_send", "Send message", &["to", "subject"], &["payload", "priority", "agent_id", "attachments"]),
        tool_def("msg_broadcast", "Broadcast message", &["subject"], &["payload", "priority", "attachments"]),
        tool_def("msg_inbox", "Get pending messages", &[], &["limit", "agent_id"]),
        tool_def("msg_reply", "Reply to message", &["message_id"], &["payload", "agent_id"]),
        tool_def("ai_agent_select", "Switch agent identity", &["agent_id"], &["session_id"]),
        tool_def("agent_list", "List agents", &[], &[]),
        tool_def("agent_query", "Find agents by capability", &["capability"], &[]),
        tool_def("agent_status", "Agent status", &["agent_id"], &[]),
        tool_def("agent_context", "Inspect agent runtime state", &[], &["agent_id"]),
        tool_def("agent_cleanup", "Clean up agents", &[], &["remove_agent", "remove_orphans"]),
        tool_def("agent_configure", "Configure agent", &["agent_id", "project_hash"], &["role", "supervisor_id"]),
        tool_def("agent_tasks", "Manage tasks", &["action"], &[]),
        tool_def("task_delegate", "Delegate task", &["to", "task"], &["context", "priority", "context_path"]),
        tool_def("task_status", "Task status", &["task_id"], &[]),
        tool_def("task_complete", "Mark task completed", &["task_id"], &["result"]),
        tool_def("metrics_cross_agent", "Cross-agent metrics", &[], &["agent_id", "period"]),
        tool_def("health_check", "Health check", &[], &[]),
        tool_def("topics_network", "Trending topics", &[], &["agent_id", "limit"]),
        tool_def("beat_wake", "Schedule self-wake", &["after"], &["reason"]),
        tool_def("nanobeat_schedule", "Schedule sub-beat wake", &["delay_seconds", "reason"], &["recall_query", "recall_thread_id"]),
    ]
}

/// Build a tool definition JSON.
fn tool_def(name: &str, desc: &str, required: &[&str], optional: &[&str]) -> serde_json::Value {
    let mut props = serde_json::Map::new();
    for &r in required.iter().chain(optional.iter()) {
        props.insert(
            r.to_string(),
            serde_json::json!({"type": "string", "description": r}),
        );
    }
    let req: Vec<&str> = required.to_vec();
    serde_json::json!({
        "name": name,
        "description": desc,
        "inputSchema": {
            "type": "object",
            "properties": props,
            "required": req
        }
    })
}
