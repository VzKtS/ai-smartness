//! Daemon IPC Client — sends captures to the daemon via local socket.
//!
//! Used by: ai-hook to send tool output captures to the daemon for processing.
//! Protocol: JSON-RPC over local socket (cross-platform via interprocess).
//!   - Unix/macOS: Unix domain sockets
//!   - Windows: Named pipes

use crate::{AiError, AiResult};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};

/// IPC request (JSON-RPC 2.0 simplified).
#[derive(Debug, Serialize)]
struct IpcRequest<'a> {
    jsonrpc: &'a str,
    method: &'a str,
    params: serde_json::Value,
    id: u64,
}

/// IPC response (JSON-RPC 2.0 simplified).
#[derive(Debug, Deserialize)]
struct IpcResponse {
    #[allow(dead_code)]
    jsonrpc: String,
    result: Option<serde_json::Value>,
    error: Option<IpcError>,
    #[allow(dead_code)]
    id: u64,
}

#[derive(Debug, Deserialize)]
struct IpcError {
    #[allow(dead_code)]
    code: i64,
    message: String,
}

/// Get the daemon socket path (cross-platform).
pub fn socket_path() -> std::path::PathBuf {
    let data_dir = std::env::var("AI_SMARTNESS_DATA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME")
                .or_else(|_| std::env::var("USERPROFILE"))
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            if cfg!(target_os = "macos") {
                home.join("Library/Application Support/ai-smartness")
            } else if cfg!(target_os = "windows") {
                home.join("AppData/Roaming/ai-smartness")
            } else {
                home.join(".config/ai-smartness")
            }
        });
    data_dir.join("processor.sock")
}

/// Send a capture to the daemon for processing.
pub fn send_capture(
    project_hash: &str,
    agent_id: &str,
    source_type: &str,
    content: &str,
) -> AiResult<serde_json::Value> {
    let params = serde_json::json!({
        "project_hash": project_hash,
        "agent_id": agent_id,
        "source_type": source_type,
        "content": content,
    });

    call_daemon("tool_capture", params)
}

/// Send a prompt capture to the daemon (distinct from tool captures).
/// Uses "prompt_capture" method so the daemon creates a CaptureJob with is_prompt=true.
pub fn send_prompt_capture(
    project_hash: &str,
    agent_id: &str,
    prompt: &str,
    session_id: Option<&str>,
) -> AiResult<serde_json::Value> {
    let mut params = serde_json::json!({
        "project_hash": project_hash,
        "agent_id": agent_id,
        "prompt": prompt,
    });
    if let Some(sid) = session_id {
        params["session_id"] = serde_json::Value::String(sid.to_string());
    }
    call_daemon("prompt_capture", params)
}

/// Ping the daemon to check if it's alive.
pub fn ping() -> AiResult<bool> {
    match call_daemon("ping", serde_json::json!({})) {
        Ok(v) => Ok(v.get("pong").is_some()),
        Err(_) => Ok(false),
    }
}

/// Get daemon status.
pub fn daemon_status() -> AiResult<serde_json::Value> {
    call_daemon("status", serde_json::json!({}))
}

/// Send shutdown command to daemon.
pub fn shutdown() -> AiResult<serde_json::Value> {
    call_daemon("shutdown", serde_json::json!({}))
}

/// Generic method call to the daemon (public wrapper).
pub fn send_method(method: &str, params: serde_json::Value) -> AiResult<serde_json::Value> {
    call_daemon(method, params)
}

/// Inner IPC call — connect, write, read, parse. Runs in a dedicated thread.
fn do_ipc_call(sock_path: std::path::PathBuf, request_json: String) -> AiResult<serde_json::Value> {
    use interprocess::local_socket::{prelude::*, GenericFilePath};

    let name = sock_path
        .to_fs_name::<GenericFilePath>()
        .map_err(|e| AiError::Provider(format!("Invalid socket name: {}", e)))?;

    let mut stream = interprocess::local_socket::Stream::connect(name)
        .map_err(|e| AiError::Provider(format!("Failed to connect to daemon: {}", e)))?;

    // Send request
    stream
        .write_all(request_json.as_bytes())
        .map_err(|e| AiError::Provider(format!("Failed to write to daemon: {}", e)))?;
    stream
        .write_all(b"\n")
        .map_err(|e| AiError::Provider(format!("Failed to write newline: {}", e)))?;
    stream
        .flush()
        .map_err(|e| AiError::Provider(format!("Failed to flush: {}", e)))?;

    // Read response
    let mut reader = BufReader::new(&mut stream);
    let mut response_line = String::new();
    reader
        .read_line(&mut response_line)
        .map_err(|e| AiError::Provider(format!("Failed to read daemon response: {}", e)))?;

    let response: IpcResponse = serde_json::from_str(&response_line)
        .map_err(|e| AiError::Provider(format!("Invalid daemon response: {}", e)))?;

    if let Some(err) = response.error {
        return Err(AiError::Provider(format!("Daemon error: {}", err.message)));
    }

    Ok(response
        .result
        .unwrap_or(serde_json::Value::Object(Default::default())))
}

/// Generic IPC call to the daemon — guarded by a 5s timeout via thread + channel.
fn call_daemon(method: &str, params: serde_json::Value) -> AiResult<serde_json::Value> {
    use std::time::Duration;

    let sock_path = socket_path();

    // On Unix, check if socket file exists; on Windows named pipes don't create files
    #[cfg(unix)]
    if !sock_path.exists() {
        return Err(AiError::Provider(format!(
            "Daemon socket not found: {}. Is the daemon running?",
            sock_path.display()
        )));
    }

    let request = IpcRequest {
        jsonrpc: "2.0",
        method,
        params,
        id: 1,
    };
    let request_json = serde_json::to_string(&request).map_err(AiError::Serialization)?;

    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        tx.send(do_ipc_call(sock_path, request_json)).ok();
    });

    rx.recv_timeout(Duration::from_secs(5))
        .map_err(|_| AiError::Provider("Daemon IPC timeout after 5s".into()))?
}
