use ai_smartness::AiResult;
use ai_smartness::storage::beat::BeatState;
use ai_smartness::storage::cognitive_inbox::CognitiveInbox;
use ai_smartness::storage::database::{self, ConnectionRole};
use ai_smartness::storage::migrations;
use ai_smartness::storage::path_utils;
use rusqlite::Connection;
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::jsonrpc::{self, JsonRpcResponse};
use super::tools::{self, ToolContext};

pub struct McpServer {
    project_hash: String,
    agent_id: String,
    /// Shared with the heartbeat thread so it tracks agent swaps.
    shared_agent_id: Arc<RwLock<String>>,
    agent_conn: Connection,
    registry_conn: Connection,
    shared_conn: Connection,
}

impl McpServer {
    pub fn new(project_hash: String, agent_id: String) -> AiResult<Self> {
        let agent_db = path_utils::agent_db_path(&project_hash, &agent_id);
        let registry_db = path_utils::registry_db_path();
        let shared_db = path_utils::shared_db_path(&project_hash);

        let agent_conn = database::open_connection(&agent_db, ConnectionRole::Mcp)?;
        let registry_conn = database::open_connection(&registry_db, ConnectionRole::Mcp)?;
        let shared_conn = database::open_connection(&shared_db, ConnectionRole::Mcp)?;

        migrations::migrate_agent_db(&agent_conn)?;
        migrations::migrate_registry_db(&registry_conn)?;
        migrations::migrate_shared_db(&shared_conn)?;

        let shared_agent_id = Arc::new(RwLock::new(agent_id.clone()));
        Ok(Self {
            project_hash,
            agent_id,
            shared_agent_id,
            agent_conn,
            registry_conn,
            shared_conn,
        })
    }

    /// Hot-swap the active agent: update agent_id and reopen agent_conn.
    fn swap_agent(&mut self, new_agent_id: String) -> AiResult<()> {
        let agent_db = path_utils::agent_db_path(&self.project_hash, &new_agent_id);
        let new_conn = database::open_connection(&agent_db, ConnectionRole::Mcp)?;
        migrations::migrate_agent_db(&new_conn)?;

        tracing::info!(
            from = %self.agent_id,
            to = %new_agent_id,
            "Hot-swapping agent connection"
        );

        self.agent_id = new_agent_id.clone();
        self.agent_conn = new_conn;
        // Notify heartbeat thread of the agent swap
        if let Ok(mut shared) = self.shared_agent_id.write() {
            *shared = new_agent_id;
        }
        Ok(())
    }

    pub fn run(&mut self) -> AiResult<()> {
        // Start background heartbeat thread (PID tracking + self-wake + cognitive proactive)
        let bg_project_hash = self.project_hash.clone();
        let bg_shared_agent = self.shared_agent_id.clone();
        let bg_running = Arc::new(AtomicBool::new(true));
        let bg_running_clone = bg_running.clone();

        let heartbeat_handle = std::thread::spawn(move || {
            heartbeat_loop(&bg_project_hash, bg_shared_agent, bg_running_clone);
        });

        let stdin = io::stdin();
        let mut stdout = io::stdout();

        for line in stdin.lock().lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => break,
            };

            if line.trim().is_empty() {
                continue;
            }

            if let Some(resp) = self.handle_message(&line) {
                let out = jsonrpc::format_response(&resp);
                let _ = writeln!(stdout, "{}", out);
                let _ = stdout.flush();
            }
        }

        // Shutdown heartbeat thread
        bg_running.store(false, Ordering::Relaxed);
        let _ = heartbeat_handle.join();

        Ok(())
    }

    fn handle_message(&mut self, input: &str) -> Option<JsonRpcResponse> {
        tracing::debug!(input_len = input.len(), "MCP request received");
        let request = match jsonrpc::parse_request(input) {
            Ok(r) => r,
            Err(e) => {
                return Some(JsonRpcResponse::error(
                    None,
                    -32700,
                    format!("Parse error: {}", e),
                ));
            }
        };

        // Notifications (no id) don't get responses
        if request.id.is_none() {
            return None;
        }

        let id = request.id.clone();

        match request.method.as_str() {
            "initialize" => Some(self.handle_initialize(id)),
            "tools/list" => Some(self.handle_tools_list(id)),
            "tools/call" => Some(self.handle_tools_call(id, &request.params)),
            _ => Some(JsonRpcResponse::error(
                id,
                -32601,
                format!("Method not found: {}", request.method),
            )),
        }
    }

    fn handle_initialize(&self, id: Option<serde_json::Value>) -> JsonRpcResponse {
        JsonRpcResponse::success(
            id,
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "ai-smartness",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        )
    }

    fn handle_tools_list(&self, id: Option<serde_json::Value>) -> JsonRpcResponse {
        JsonRpcResponse::success(
            id,
            serde_json::json!({
                "tools": tool_definitions()
            }),
        )
    }

    fn handle_tools_call(
        &mut self,
        id: Option<serde_json::Value>,
        params: &Option<serde_json::Value>,
    ) -> JsonRpcResponse {
        let params = match params {
            Some(p) => p,
            None => {
                return JsonRpcResponse::error(id, -32602, "Missing params".into());
            }
        };

        let tool_name = match params.get("name").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => {
                return JsonRpcResponse::error(id, -32602, "Missing tool name".into());
            }
        };

        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or(serde_json::json!({}));

        // Phase 1: execute the tool (ToolContext borrows are scoped here)
        let tool_result = {
            let ctx = ToolContext {
                agent_conn: &self.agent_conn,
                registry_conn: &self.registry_conn,
                shared_conn: &self.shared_conn,
                project_hash: &self.project_hash,
                agent_id: &self.agent_id,
            };
            tracing::debug!(tool = %tool_name, "MCP tools/call dispatching");
            tools::route_tool(tool_name, &arguments, &ctx)
        };
        // Phase 1 done — ctx is dropped, no outstanding borrows on self.

        // Phase 2: inspect ToolOutput, apply side-effects, build response
        match tool_result {
            Ok(output) => {
                let (result, side_effect) = match output {
                    tools::ToolOutput::Plain(v) => (v, None),
                    tools::ToolOutput::AgentSwitch { result, new_agent_id } => {
                        (result, Some(new_agent_id))
                    }
                };

                // Apply side-effect before building success response
                if let Some(new_agent_id) = side_effect {
                    if let Err(e) = self.swap_agent(new_agent_id) {
                        tracing::error!(error = %e, "Failed to hot-swap agent");
                        return JsonRpcResponse::success(
                            id,
                            serde_json::json!({
                                "content": [{"type": "text", "text": format!("Agent switch failed: {}", e)}],
                                "isError": true
                            }),
                        );
                    }
                }

                let text = serde_json::to_string_pretty(&result)
                    .unwrap_or_else(|_| result.to_string());
                JsonRpcResponse::success(
                    id,
                    serde_json::json!({
                        "content": [{"type": "text", "text": text}]
                    }),
                )
            }
            Err(e) => JsonRpcResponse::success(
                id,
                serde_json::json!({
                    "content": [{"type": "text", "text": format!("Error: {}", e)}],
                    "isError": true
                }),
            ),
        }
    }
}

// ─── Background Heartbeat ───

const HEARTBEAT_TICK_SECS: u64 = 10;

fn heartbeat_loop(project_hash: &str, shared_agent: Arc<RwLock<String>>, running: Arc<AtomicBool>) {
    let pid = std::process::id();
    let mut current_agent = String::new();

    tracing::info!(pid, "Heartbeat thread started");

    while running.load(Ordering::Relaxed) {
        // Read current agent_id (may change via swap_agent)
        let agent_id = match shared_agent.read() {
            Ok(s) => s.clone(),
            Err(_) => {
                tracing::error!("Heartbeat: shared_agent RwLock poisoned, stopping");
                break;
            }
        };

        // Detect agent swap: clean old PID, update tracking
        if agent_id != current_agent {
            if !current_agent.is_empty() {
                let old_dir = path_utils::agent_data_dir(project_hash, &current_agent);
                let mut old_beat = BeatState::load(&old_dir);
                old_beat.pid = None;
                old_beat.save(&old_dir);
                tracing::info!(from = %current_agent, to = %agent_id, "Heartbeat: agent swapped");
            }
            current_agent = agent_id.clone();
        }

        let data_dir = path_utils::agent_data_dir(project_hash, &agent_id);

        // 1. Update beat.json with PID and timestamp
        let mut beat = BeatState::load(&data_dir);
        beat.pid = Some(pid);
        beat.last_beat_at = chrono::Utc::now().to_rfc3339();
        beat.save(&data_dir);

        // 2. Check scheduled self-wakes
        check_scheduled_wakes(&agent_id, &data_dir);

        // 3. Check cognitive inbox for proactive wake
        check_cognitive_proactive(project_hash, &agent_id);

        // Sleep in 1s increments for clean shutdown
        for _ in 0..HEARTBEAT_TICK_SECS {
            if !running.load(Ordering::Relaxed) { break; }
            std::thread::sleep(Duration::from_secs(1));
        }
    }

    // Cleanup: remove PID from beat.json on exit
    if !current_agent.is_empty() {
        let data_dir = path_utils::agent_data_dir(project_hash, &current_agent);
        let mut beat = BeatState::load(&data_dir);
        beat.pid = None;
        beat.save(&data_dir);
    }

    tracing::info!("Heartbeat thread stopped");
}

fn check_scheduled_wakes(agent_id: &str, data_dir: &Path) {
    let mut beat = BeatState::load(data_dir);
    let due = beat.drain_due_wakes();
    if due.is_empty() { return; }
    beat.save(data_dir);

    let reasons: Vec<&str> = due.iter().map(|w| w.reason.as_str()).collect();
    let message = format!("Self-wake: {}", reasons.join(", "));
    tracing::info!(agent = agent_id, message = %message, "Scheduled wake triggered");
    super::tools::messaging::emit_wake_signal(agent_id, "heartbeat", &message, "cognitive");
}

fn check_cognitive_proactive(project_hash: &str, agent_id: &str) {

    let agent_db = path_utils::agent_db_path(project_hash, agent_id);
    if !agent_db.exists() { return; }

    let conn = match database::open_connection(&agent_db, ConnectionRole::Mcp) {
        Ok(c) => c,
        Err(_) => return,
    };

    let messages = match CognitiveInbox::read_pending(&conn, agent_id) {
        Ok(m) => m,
        Err(_) => return,
    };

    if messages.is_empty() { return; }

    // Check if a wake signal already exists and is unacknowledged
    let signal_path = path_utils::wake_signal_path(agent_id);
    if signal_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&signal_path) {
            if let Ok(sig) = serde_json::from_str::<serde_json::Value>(&content) {
                if sig.get("acknowledged").and_then(|v| v.as_bool()) == Some(false) {
                    return; // Already has pending wake signal
                }
            }
        }
    }

    tracing::info!(
        agent = agent_id,
        pending = messages.len(),
        "Cognitive proactive: emitting wake signal"
    );
    super::tools::messaging::emit_wake_signal(agent_id, "cognitive-inbox", "Pending cognitive messages", "cognitive");
}

fn tool_definitions() -> Vec<serde_json::Value> {
    vec![
        tool_def("ai_recall", "Search semantic memory for relevant threads", &["query"], &["label"]),
        tool_def("ai_thread_create", "Create a new thread manually", &["title", "content"], &["topics", "importance"]),
        tool_def("ai_thread_rm", "Delete a thread by ID", &["thread_id"], &[]),
        tool_def("ai_thread_rm_batch", "Delete multiple threads", &["thread_ids"], &[]),
        tool_def("ai_thread_list", "List threads with filters", &[], &["status", "sort_by", "limit", "offset"]),
        tool_def("ai_thread_search", "Search threads across all states", &["query"], &["scope", "states"]),
        tool_def("ai_thread_activate", "Reactivate threads", &["thread_ids"], &["confirm"]),
        tool_def("ai_thread_suspend", "Suspend active threads", &["thread_ids"], &["reason", "confirm"]),
        tool_def("ai_reactivate", "Reactivate a thread by ID", &["thread_id"], &[]),
        tool_def("ai_merge", "Merge two threads", &["survivor_id", "absorbed_id"], &[]),
        tool_def("ai_merge_batch", "Merge multiple thread pairs", &["operations"], &[]),
        tool_def("ai_split", "Split a thread", &["thread_id"], &["confirm", "message_groups", "titles", "lock_mode"]),
        tool_def("ai_split_unlock", "Remove split lock", &["thread_id"], &[]),
        tool_def("ai_label", "Manage labels", &["thread_id", "labels"], &["mode"]),
        tool_def("ai_labels_suggest", "Show existing labels", &["label"], &[]),
        tool_def("ai_rename", "Rename a thread", &["thread_id", "new_title"], &[]),
        tool_def("ai_rename_batch", "Rename multiple threads", &["operations"], &[]),
        tool_def("ai_rate_importance", "Set importance score", &["thread_id", "score"], &["reason"]),
        tool_def("ai_rate_context", "Rate context usefulness", &["thread_id", "useful"], &["reason"]),
        tool_def("ai_bridges", "List bridges", &[], &["thread_id", "relation_type", "status"]),
        tool_def("ai_bridge_analysis", "Bridge network analytics", &[], &[]),
        tool_def("ai_bridge_scan_orphans", "Scan orphan bridges", &[], &["confirm"]),
        tool_def("ai_bridge_kill", "Delete a bridge", &["bridge_id"], &[]),
        tool_def("ai_bridge_kill_batch", "Delete multiple bridges", &["bridge_ids"], &[]),
        tool_def("ai_focus", "Focus on a topic", &["topic"], &["weight"]),
        tool_def("ai_unfocus", "Remove focus", &[], &["topic"]),
        tool_def("ai_pin", "Pin important content", &["content"], &["title", "topics", "weight_boost"]),
        tool_def("ai_msg_focus", "Write cognitive message", &["target_agent_id", "from_agent", "subject", "content"], &["priority", "ttl_minutes"]),
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
        tool_def("ai_help", "Documentation", &[], &[]),
        tool_def("ai_suggestions", "Proactive suggestions", &[], &["context"]),
        tool_def("ai_shared_status", "Shared cognition status", &[], &[]),
        tool_def("ai_profile", "User profile", &["action"], &["key", "value"]),
        tool_def("ai_cleanup", "Fix thread titles", &[], &["mode", "dry_run"]),
        tool_def("ai_lock", "Lock memory", &[], &["reason", "duration_minutes"]),
        tool_def("ai_unlock", "Unlock memory", &[], &[]),
        tool_def("ai_lock_status", "Lock state", &[], &[]),
        tool_def("ai_backup", "Backup/restore", &["action"], &["interval_hours"]),
        tool_def("ai_recommend", "Subscription recommendations", &[], &["limit"]),
        tool_def("ai_topics", "Topic discovery", &[], &["agent_id"]),
        tool_def("msg_send", "Send message", &["to", "subject"], &["payload", "priority", "agent_id"]),
        tool_def("msg_broadcast", "Broadcast message", &["subject"], &["payload", "priority"]),
        tool_def("msg_inbox", "Get pending messages", &[], &["limit", "agent_id"]),
        tool_def("msg_reply", "Reply to message", &["message_id"], &["payload", "agent_id"]),
        tool_def("ai_agent_select", "Switch to a different agent for this session. Writes the session file so subsequent prompts use the new agent identity. Pass session_id from your context for multi-panel isolation.", &["agent_id"], &["session_id"]),
        tool_def("agent_list", "List agents", &[], &[]),
        tool_def("agent_query", "Find agents by capability", &["capability"], &[]),
        tool_def("agent_status", "Agent status", &["agent_id"], &[]),
        tool_def("agent_cleanup", "Clean up agents", &[], &["remove_agent", "remove_orphans"]),
        tool_def("agent_configure", "Configure agent", &["agent_id", "project_hash"], &["role", "supervisor_id"]),
        tool_def("agent_tasks", "Manage tasks", &["action"], &[]),
        tool_def("task_delegate", "Delegate task", &["to", "task"], &["context", "priority"]),
        tool_def("task_status", "Task status", &["task_id"], &[]),
        tool_def("metrics_cross_agent", "Cross-agent metrics", &[], &["agent_id", "period"]),
        tool_def("health_check", "Health check", &[], &[]),
        tool_def("topics_network", "Trending topics", &[], &["agent_id", "limit"]),
        tool_def("test_sampling", "Test sampling", &[], &["attempt_sampling"]),
        tool_def("beat_wake", "Schedule self-wake after N beats (~5 min each). The heartbeat system will wake you automatically.", &["after"], &["reason"]),
    ]
}

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
