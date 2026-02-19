//! Shared tracing initialization — all processes append to daemon.log.
//!
//! This ensures hooks, MCP, and daemon all write structured logs to the same
//! file, making them all visible in the Debug Console.

use std::sync::Mutex;

use crate::storage::path_utils;

/// Initialize tracing to daemon.log for the given project (append mode).
///
/// All binaries (daemon, hooks, MCP) should call this so that every event
/// is visible in the GUI Debug Console.
///
/// - `source`: human label like "daemon", "hook", "mcp" (shown in logs via target)
/// - `project_hash`: identifies which project's daemon.log to write to
pub fn init_file_tracing(project_hash: &str) {
    use tracing_subscriber::EnvFilter;

    let project_dir = path_utils::project_dir(project_hash);
    std::fs::create_dir_all(&project_dir).ok();
    let log_path = project_dir.join("daemon.log");

    // Open in APPEND mode — multiple processes write to the same file.
    // Short writes (< PIPE_BUF = 4096) are atomic on Linux/macOS.
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .unwrap_or_else(|_| {
            let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
            std::fs::File::create(null).expect("Cannot create log fallback")
        });

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(Mutex::new(log_file))
        .with_target(true)
        .with_ansi(false)
        .init();
}

/// Initialize global tracing to `{data_dir}/daemon.log`.
///
/// Used by the global daemon process. Unlike `init_file_tracing` which writes
/// to a per-project log, this writes to the global data directory.
pub fn init_global_tracing() {
    use tracing_subscriber::EnvFilter;

    let data_dir = path_utils::data_dir();
    std::fs::create_dir_all(&data_dir).ok();
    let log_path = data_dir.join("daemon.log");

    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .unwrap_or_else(|_| {
            let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
            std::fs::File::create(null).expect("Cannot create log fallback")
        });

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(Mutex::new(log_file))
        .with_target(true)
        .with_ansi(false)
        .init();
}
