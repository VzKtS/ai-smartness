//! IPC Server — cross-platform local socket, line-delimited JSON-RPC protocol.
//!
//! Uses `interprocess` crate for cross-platform IPC:
//!   - Unix/macOS: Unix domain sockets
//!   - Windows: Named pipes
//!
//! Protocol: JSON-RPC 2.0 over local socket (matches daemon_ipc_client.rs).
//! Each message is a single JSON line terminated by \n.
//!
//! **Multi-threaded**: each incoming connection is handled in its own thread,
//! so slow operations never block other agents.
//!
//! **Async captures**: tool_capture and prompt_capture are dispatched to a
//! bounded CaptureQueue with N worker threads — hooks get instant responses.
//!
//! Methods:
//!   ping            → {"pong": true}
//!   status          → daemon status JSON (global or per-agent)
//!   shutdown        → initiate graceful shutdown
//!   tool_capture    → queue tool output capture (instant response)
//!   prompt_capture  → queue user prompt capture (instant response)
//!   injection_usage → record thread injection usage
//!   pool_status     → connection pool stats
//!   queue_status    → capture queue stats (pending, processed, errors, workers)
//!   list_active_agents → list all agents in pool
//!   lock / unlock   → per-agent memory lock

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;

use ai_smartness::agent::ThreadMode;
use ai_smartness::intelligence::thread_manager::ThreadManager;
use ai_smartness::thread::ThreadStatus;
use ai_smartness::storage::threads::ThreadStorage;

use super::capture_queue::{CaptureJob, CaptureQueue};
use super::connection_pool::{AgentKey, ConnectionPool};

/// JSON-RPC response sent back to clients.
#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
    id: u64,
}

/// JSON-RPC error object.
#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i64,
    message: String,
}

/// Run the IPC listener using interprocess for cross-platform support.
/// Multi-threaded: each incoming connection spawns a dedicated handler thread.
pub fn run(
    socket_path: &Path,
    pool: Arc<ConnectionPool>,
    capture_queue: Arc<CaptureQueue>,
    running: Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error>> {
    use interprocess::local_socket::{prelude::*, GenericFilePath, ListenerOptions};

    // Remove stale socket file (harmless on Windows)
    let _ = std::fs::remove_file(socket_path);

    let listener = ListenerOptions::new()
        .name(socket_path.to_fs_name::<GenericFilePath>()?)
        .create_sync()?;

    tracing::info!("IPC listening on {:?} (multi-threaded)", socket_path);

    let start_time = Arc::new(Instant::now());

    while running.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok(stream) => {
                if !running.load(Ordering::Relaxed) {
                    break;
                }
                let pool = pool.clone();
                let queue = capture_queue.clone();
                let running = running.clone();
                let start = start_time.clone();
                std::thread::spawn(move || {
                    handle_connection(stream, &pool, &queue, &running, &start);
                });
            }
            Err(e) => {
                if !running.load(Ordering::Relaxed) {
                    break;
                }
                tracing::warn!("Accept error: {}", e);
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }

    // Cleanup socket file
    let _ = std::fs::remove_file(socket_path);
    tracing::info!("IPC server stopped");
    Ok(())
}

/// Wake the listener by connecting to it (for clean shutdown).
pub fn wake(socket_path: &Path) {
    use interprocess::local_socket::{prelude::*, GenericFilePath};

    if let Ok(name) = socket_path.to_fs_name::<GenericFilePath>() {
        // Just connect and drop — the accept will unblock
        let _ = interprocess::local_socket::Stream::connect(name);
    }
}

/// Extract (project_hash, agent_id) from JSON-RPC params.
fn extract_agent_key(params: &serde_json::Value) -> Result<AgentKey, String> {
    let project_hash = params
        .get("project_hash")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing 'project_hash' in params".to_string())?;
    let agent_id = params
        .get("agent_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing 'agent_id' in params".to_string())?;

    Ok(AgentKey {
        project_hash: project_hash.to_string(),
        agent_id: agent_id.to_string(),
    })
}

fn handle_connection(
    stream: interprocess::local_socket::Stream,
    pool: &Arc<ConnectionPool>,
    capture_queue: &Arc<CaptureQueue>,
    running: &Arc<AtomicBool>,
    start_time: &Instant,
) {
    let mut stream = stream;
    let mut line = String::new();
    {
        let mut reader = BufReader::new(&mut stream);
        if reader.read_line(&mut line).is_err() || line.trim().is_empty() {
            return;
        }
    }

    let data: serde_json::Value = match serde_json::from_str(&line) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "IPC: invalid JSON received");
            return;
        }
    };

    let method = data
        .get("method")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let params = data
        .get("params")
        .cloned()
        .unwrap_or(serde_json::json!({}));
    let id = data.get("id").and_then(|v| v.as_u64()).unwrap_or(0);

    let request_start = Instant::now();
    tracing::debug!(method = method, id = id, "IPC request received");

    let result = dispatch(method, &params, pool, capture_queue, running, start_time);

    tracing::debug!(
        method = method,
        duration_ms = request_start.elapsed().as_millis() as u64,
        "IPC request completed"
    );

    let response = match result {
        Ok(r) => JsonRpcResponse {
            jsonrpc: "2.0",
            result: Some(r),
            error: None,
            id,
        },
        Err(msg) => JsonRpcResponse {
            jsonrpc: "2.0",
            result: None,
            error: Some(JsonRpcError {
                code: -1,
                message: msg,
            }),
            id,
        },
    };

    if let Ok(json) = serde_json::to_string(&response) {
        let _ = stream.write_all(json.as_bytes());
        let _ = stream.write_all(b"\n");
        let _ = stream.flush();
    }
}

fn dispatch(
    method: &str,
    params: &serde_json::Value,
    pool: &Arc<ConnectionPool>,
    capture_queue: &Arc<CaptureQueue>,
    running: &Arc<AtomicBool>,
    start_time: &Instant,
) -> Result<serde_json::Value, String> {
    match method {
        "ping" => Ok(serde_json::json!({"pong": true})),

        "shutdown" => {
            tracing::info!("Shutdown requested via IPC");
            running.store(false, Ordering::Relaxed);
            Ok(serde_json::json!({"shutting_down": true}))
        }

        "status" => {
            // If params contain project_hash → per-agent status
            // Otherwise → global daemon status
            if params.get("project_hash").is_some() {
                let key = extract_agent_key(params)?;
                tracing::info!(project = %key.project_hash, agent = %key.agent_id, "IPC status request (agent)");
                let conn = pool.get_or_open(&key)?;
                let conn_guard = conn.lock().map_err(|e| e.to_string())?;
                Ok(build_agent_status(&conn_guard, &key, pool, capture_queue, start_time))
            } else {
                Ok(build_global_status(pool, capture_queue, start_time))
            }
        }

        // ── Async captures: dispatch to queue, respond instantly ──

        "tool_capture" => {
            let key = extract_agent_key(params)?;
            tracing::info!(
                method = "tool_capture",
                project = %key.project_hash,
                agent = %key.agent_id,
                "IPC: queuing tool capture"
            );

            // Ensure agent DB connection exists in pool (lazy open)
            let _ = pool.get_or_open(&key)?;

            let source_type = params
                .get("source_type")
                .and_then(|v| v.as_str())
                .unwrap_or("prompt")
                .to_string();
            let content = params
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let file_path = params
                .get("file_path")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let job = CaptureJob {
                key,
                source_type,
                content,
                file_path,
                is_prompt: false,
                session_id: None,
            };

            match capture_queue.submit(job) {
                Ok(()) => Ok(serde_json::json!({"queued": true})),
                Err(_) => Ok(serde_json::json!({"queued": false, "reason": "queue_full"})),
            }
        }

        "prompt_capture" => {
            let key = extract_agent_key(params)?;
            tracing::info!(
                method = "prompt_capture",
                project = %key.project_hash,
                agent = %key.agent_id,
                "IPC: queuing prompt capture"
            );

            // Ensure agent DB connection exists in pool (lazy open)
            let _ = pool.get_or_open(&key)?;

            let prompt = params
                .get("prompt")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let session_id = params
                .get("session_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let job = CaptureJob {
                key,
                source_type: "prompt".to_string(),
                content: prompt,
                file_path: None,
                is_prompt: true,
                session_id,
            };

            match capture_queue.submit(job) {
                Ok(()) => Ok(serde_json::json!({"queued": true})),
                Err(_) => Ok(serde_json::json!({"queued": false, "reason": "queue_full"})),
            }
        }

        "injection_usage" => {
            let key = extract_agent_key(params)?;
            let thread_id = params
                .get("thread_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if !thread_id.is_empty() {
                let conn = pool.get_or_open(&key)?;
                let conn_guard = conn.lock().map_err(|e| e.to_string())?;
                if let Ok(Some(mut thread)) = ThreadStorage::get(&conn_guard, thread_id) {
                    thread.activation_count += 1;
                    let _ = ThreadStorage::update(&conn_guard, &thread);
                }
            }
            Ok(serde_json::json!({"recorded": true}))
        }

        "lock" => {
            let key = extract_agent_key(params)?;
            pool.set_locked(&key, true);
            tracing::info!(project = %key.project_hash, agent = %key.agent_id, "Memory locked");
            Ok(serde_json::json!({"locked": true}))
        }

        "unlock" => {
            let key = extract_agent_key(params)?;
            pool.set_locked(&key, false);
            tracing::info!(project = %key.project_hash, agent = %key.agent_id, "Memory unlocked");
            Ok(serde_json::json!({"locked": false}))
        }

        "pool_status" => {
            let stats = pool.stats();
            Ok(serde_json::to_value(stats).unwrap_or_default())
        }

        "queue_status" => {
            Ok(capture_queue.queue_stats())
        }

        "set_thread_mode" => {
            let key = extract_agent_key(params)?;
            let mode_str = params
                .get("thread_mode")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing 'thread_mode' in params".to_string())?;
            let mode: ThreadMode = mode_str.parse().map_err(|_| {
                format!("Invalid thread_mode: '{}'. Use: light, normal, heavy, max", mode_str)
            })?;

            let quota = mode.quota();
            pool.set_thread_quota(&key, quota);
            tracing::info!(
                agent = %key.agent_id,
                thread_mode = %mode,
                quota = quota,
                "Thread mode updated via IPC"
            );

            // Enforce quota: suspend excess threads
            let conn = pool.get_or_open(&key)?;
            let conn_guard = conn.lock().map_err(|e| e.to_string())?;
            let suspended = ThreadManager::enforce_quota(&conn_guard, quota)
                .map_err(|e| format!("Failed to enforce quota: {}", e))?;

            Ok(serde_json::json!({
                "updated": true,
                "thread_mode": mode_str,
                "quota": quota,
                "threads_suspended": suspended,
            }))
        }

        "list_active_agents" => {
            let keys = pool.active_keys();
            let agents: Vec<serde_json::Value> = keys
                .iter()
                .map(|k| {
                    serde_json::json!({
                        "project_hash": k.project_hash,
                        "agent_id": k.agent_id,
                        "locked": pool.is_locked(k),
                    })
                })
                .collect();
            Ok(serde_json::json!({"agents": agents}))
        }

        _ => {
            tracing::warn!(method = method, "Unknown IPC method");
            Err(format!("Unknown method: {}", method))
        }
    }
}

fn build_global_status(
    pool: &ConnectionPool,
    capture_queue: &CaptureQueue,
    start_time: &Instant,
) -> serde_json::Value {
    let stats = pool.stats();
    let agents = pool.active_keys();
    let queue = capture_queue.queue_stats();

    serde_json::json!({
        "version": "5.1.0",
        "mode": "global",
        "pid": std::process::id(),
        "uptime_secs": start_time.elapsed().as_secs(),
        "pool": stats,
        "active_agents": agents.len(),
        "capture_queue": queue,
        "health": "ok",
    })
}

fn build_agent_status(
    conn: &rusqlite::Connection,
    key: &AgentKey,
    pool: &ConnectionPool,
    capture_queue: &CaptureQueue,
    start_time: &Instant,
) -> serde_json::Value {
    let thread_count = ThreadStorage::count(conn).unwrap_or(0);
    let active_count = ThreadStorage::count_by_status(conn, &ThreadStatus::Active).unwrap_or(0);
    let queue = capture_queue.queue_stats();

    serde_json::json!({
        "version": "5.1.0",
        "mode": "global",
        "pid": std::process::id(),
        "uptime_secs": start_time.elapsed().as_secs(),
        "project_hash": key.project_hash,
        "agent_id": key.agent_id,
        "threads_total": thread_count,
        "threads_active": active_count,
        "locked": pool.is_locked(key),
        "capture_queue": queue,
        "health": "ok",
    })
}
