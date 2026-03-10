//! Health check — DETECT -> HEAL -> VERIFY -> REPORT

use ai_smartness::{HealthLevel, HealthStatus};
use ai_smartness::processing::daemon_ipc_client;
use ai_smartness::storage::database::{open_connection, ConnectionRole};
use ai_smartness::storage::path_utils;

/// Full health check: SQLite, daemon, wake_signals.
pub fn check_and_heal(project_hash: &str, agent_id: &str) -> HealthStatus {
    tracing::info!("Health check starting");
    let sqlite_ok = check_sqlite(project_hash, agent_id);
    let (daemon_alive, daemon_pid) = check_daemon();
    let wake_signals_ok = check_wake_signals();

    let mut repairs = Vec::new();

    // Auto-repair: create directories if missing
    if !wake_signals_ok {
        let ws_dir = path_utils::wake_signals_dir();
        if std::fs::create_dir_all(&ws_dir).is_ok() {
            repairs.push("Created wake_signals directory".into());
        }
    }

    let overall = if sqlite_ok && daemon_alive {
        HealthLevel::Ok
    } else if sqlite_ok {
        HealthLevel::Degraded
    } else {
        HealthLevel::Critical
    };

    if !sqlite_ok {
        tracing::warn!(sqlite = sqlite_ok, daemon = daemon_alive, "Health check: SQLite unhealthy");
    }
    if !daemon_alive {
        tracing::warn!("Health check: daemon not alive");
    }

    tracing::info!(level = ?overall, sqlite = sqlite_ok, daemon = daemon_alive, repairs = repairs.len(), "Health check complete");

    HealthStatus {
        sqlite_ok,
        daemon_alive,
        daemon_pid,
        wake_signals_ok,
        overall,
        repairs,
    }
}

/// Check SQLite read/write.
fn check_sqlite(project_hash: &str, agent_id: &str) -> bool {
    let db_path = path_utils::agent_db_path(project_hash, agent_id);
    if !db_path.exists() {
        return false;
    }
    match open_connection(&db_path, ConnectionRole::Hook) {
        Ok(conn) => conn.execute_batch("SELECT 1").is_ok(),
        Err(_) => false,
    }
}

/// Check daemon liveness via IPC ping + PID file fallback.
fn check_daemon() -> (bool, Option<u32>) {
    // Try IPC ping (most reliable)
    if daemon_ipc_client::ping().unwrap_or(false) {
        return (true, None);
    }

    // Fallback: check PID files in project directories
    if let Ok(entries) = std::fs::read_dir(path_utils::projects_dir()) {
        for entry in entries.flatten() {
            let pid_path = entry.path().join("processor.pid");
            if let Ok(content) = std::fs::read_to_string(&pid_path) {
                if let Ok(pid) = content.trim().parse::<u32>() {
                    if is_process_alive(pid) {
                        return (true, Some(pid));
                    }
                }
            }
        }
    }

    (false, None)
}

/// Check wake_signals directory exists.
fn check_wake_signals() -> bool {
    path_utils::wake_signals_dir().exists()
}

/// Check if a process is still alive (cross-platform).
fn is_process_alive(_pid: u32) -> bool {
    #[cfg(unix)]
    {
        // Send signal 0 to check if process exists
        extern "C" {
            fn kill(pid: i32, sig: i32) -> i32;
        }
        unsafe { kill(_pid as i32, 0) == 0 }
    }
    #[cfg(not(unix))]
    {
        // On non-Unix platforms, assume alive if PID file exists
        true
    }
}
