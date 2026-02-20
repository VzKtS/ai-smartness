pub mod capture_queue;
pub mod connection_pool;
pub mod ipc_server;
pub mod periodic_tasks;
pub mod processor;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use ai_smartness::config::DaemonConfig;
use ai_smartness::storage::path_utils;

use capture_queue::CaptureQueue;
use connection_pool::ConnectionPool;

/// Run the global daemon in foreground mode.
///
/// No project_hash or agent_id required — the daemon serves all projects
/// and agents via the connection pool. Connections are opened lazily on
/// first IPC request containing (project_hash, agent_id).
///
/// Architecture:
///   - IPC server: multi-threaded (1 thread per connection)
///   - Capture queue: N worker threads for LLM extraction (configurable)
///   - Prune loop: 1 thread for periodic maintenance
pub fn run() {
    // Init global tracing to {data_dir}/daemon.log
    ai_smartness::tracing_init::init_global_tracing();

    let config = DaemonConfig::load();
    tracing::info!(
        pool_max = config.pool_max_connections,
        idle_secs = config.pool_max_idle_secs,
        prune_secs = config.prune_interval_secs,
        capture_workers = config.capture_workers,
        capture_queue_capacity = config.capture_queue_capacity,
        "Starting global ai-daemon"
    );

    // Global PID file: {data_dir}/daemon.pid
    let data_dir = path_utils::data_dir();
    std::fs::create_dir_all(&data_dir).ok();

    // Set ORT_DYLIB_PATH before any ONNX usage (EmbeddingManager::global())
    // ort with load-dynamic panics if libonnxruntime is not found; this env var
    // tells it exactly where to look.
    if std::env::var("ORT_DYLIB_PATH").is_err() {
        let lib_name = if cfg!(target_os = "macos") {
            "libonnxruntime.dylib"
        } else {
            "libonnxruntime.so"
        };
        let ort_path = data_dir.join("lib").join(lib_name);
        if ort_path.exists() {
            tracing::info!(path = %ort_path.display(), "Setting ORT_DYLIB_PATH");
            std::env::set_var("ORT_DYLIB_PATH", &ort_path);
        } else {
            tracing::warn!(
                path = %ort_path.display(),
                "ONNX Runtime not found — embeddings will use TF-IDF fallback. \
                 Run `ai-smartness setup-onnx` to download it."
            );
        }
    }

    let pid_path = data_dir.join("daemon.pid");
    std::fs::write(&pid_path, std::process::id().to_string()).ok();

    // Cleanup legacy per-project PID files
    cleanup_legacy_pid_files();

    // Startup validation: integrity check + missed backups
    startup_validation();

    // Eagerly initialize the embedding singleton BEFORE spawning workers.
    // OnceLock init loads the ONNX model (~5-10s); if workers hit it first,
    // all block simultaneously and the capture queue fills up.
    {
        let emb = ai_smartness::processing::embeddings::EmbeddingManager::global();
        tracing::info!(use_onnx = emb.use_onnx, "EmbeddingManager initialized (eager)");
    }

    // Connection pool
    let pool = Arc::new(ConnectionPool::new(
        config.pool_max_idle_secs,
        config.pool_max_connections,
    ));

    // Capture queue with worker thread pool
    let capture_queue = Arc::new(CaptureQueue::new(
        pool.clone(),
        config.capture_workers,
        config.capture_queue_capacity,
    ));

    // Shared state
    let running = Arc::new(AtomicBool::new(true));

    // Socket path (cross-platform via interprocess)
    let socket_path = data_dir.join("processor.sock");
    std::fs::create_dir_all(socket_path.parent().unwrap_or(&socket_path)).ok();

    // Start IPC listener thread (multi-threaded: spawns per-connection threads)
    let ipc_handle = {
        let pool = pool.clone();
        let queue = capture_queue.clone();
        let running = running.clone();
        let sock = socket_path.clone();
        std::thread::spawn(move || {
            if let Err(e) = ipc_server::run(&sock, pool, queue, running) {
                tracing::error!("IPC server error: {}", e);
            }
        })
    };

    // Start prune loop thread
    let prune_handle = {
        let pool = pool.clone();
        let running = running.clone();
        let prune_interval = config.prune_interval_secs;
        std::thread::spawn(move || {
            periodic_tasks::run_prune_loop(pool, running, prune_interval);
        })
    };

    // Signal handlers (cross-platform)
    signal_hook::flag::register(signal_hook::consts::SIGINT, running.clone()).ok();
    #[cfg(unix)]
    signal_hook::flag::register(signal_hook::consts::SIGTERM, running.clone()).ok();

    // Main loop: wait for shutdown
    while running.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_secs(1));
    }

    tracing::info!("Shutting down...");

    // Wake IPC listener to unblock accept()
    ipc_server::wake(&socket_path);

    // Close all pool connections
    pool.close_all();

    // Shutdown capture queue (drain pending jobs, join workers)
    // We need to extract from Arc — if other refs exist, workers will stop when channel drops
    if let Ok(queue) = Arc::try_unwrap(capture_queue) {
        queue.shutdown();
    } else {
        tracing::warn!("Capture queue still referenced — workers will stop when channel drops");
    }

    // Cleanup PID file
    let _ = std::fs::remove_file(&pid_path);

    // Wait for threads
    let _ = ipc_handle.join();
    let _ = prune_handle.join();

    tracing::info!("Global daemon shutdown complete");
}

/// Startup validation: integrity check on all agent DBs + missed backup catch-up.
fn startup_validation() {
    tracing::info!("Running startup validation...");

    // 1. Integrity check on all known agent DBs
    let projects_dir = path_utils::projects_dir();
    if let Ok(project_entries) = std::fs::read_dir(&projects_dir) {
        for project_entry in project_entries.flatten() {
            let agents_dir = project_entry.path().join("agents");
            if !agents_dir.exists() {
                continue;
            }
            if let Ok(agent_entries) = std::fs::read_dir(&agents_dir) {
                for agent_entry in agent_entries.flatten() {
                    let path = agent_entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("db") {
                        check_db_integrity(&path);
                    }
                }
            }
        }
    }

    // 2. Check for missed backups
    check_missed_backups();

    tracing::info!("Startup validation complete");
}

fn check_db_integrity(db_path: &std::path::Path) {
    use ai_smartness::storage::database::{open_connection, ConnectionRole};
    let conn = match open_connection(db_path, ConnectionRole::Daemon) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(path = %db_path.display(), error = %e, "Failed to open DB");
            return;
        }
    };
    match conn.query_row("PRAGMA quick_check;", [], |row| row.get::<_, String>(0)) {
        Ok(ref r) if r == "ok" => {
            tracing::debug!(path = %db_path.display(), "DB integrity OK")
        }
        Ok(result) => {
            tracing::error!(path = %db_path.display(), result = %result, "DB integrity FAILED");
            conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").ok();
        }
        Err(e) => {
            tracing::error!(path = %db_path.display(), error = %e, "Integrity check failed")
        }
    }
}

fn check_missed_backups() {
    use ai_smartness::storage::backup::{BackupConfig, BackupManager};
    let config = BackupConfig::load();
    if !config.is_missed() {
        return;
    }

    tracing::warn!("Missed backup detected — running catch-up");
    let backup_dir =
        std::path::PathBuf::from(path_utils::expand_tilde(&config.backup_path));
    let projects_dir = path_utils::projects_dir();

    if let Ok(project_entries) = std::fs::read_dir(&projects_dir) {
        for project_entry in project_entries.flatten() {
            let project_hash = project_entry.file_name().to_string_lossy().to_string();
            let agents_dir = project_entry.path().join("agents");
            if !agents_dir.exists() {
                continue;
            }
            if let Ok(agent_entries) = std::fs::read_dir(&agents_dir) {
                for agent_entry in agent_entries.flatten() {
                    let path = agent_entry.path();
                    if path.extension().and_then(|e| e.to_str()) != Some("db") {
                        continue;
                    }
                    let agent_id =
                        path.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown");
                    match BackupManager::backup_agent(&project_hash, agent_id, &backup_dir)
                    {
                        Ok(p) => tracing::info!(
                            agent = agent_id,
                            path = %p.display(),
                            "Catch-up backup"
                        ),
                        Err(e) => tracing::warn!(
                            agent = agent_id,
                            error = %e,
                            "Catch-up backup failed"
                        ),
                    }
                }
            }
        }
    }

    let mut cfg = BackupConfig::load();
    cfg.last_backup_at = Some(chrono::Utc::now().to_rfc3339());
    cfg.save();
    BackupManager::enforce_retention(&backup_dir, config.retention_count);
}

/// Remove legacy per-project PID files (from pre-global daemon era).
fn cleanup_legacy_pid_files() {
    let projects_dir = path_utils::projects_dir();
    if let Ok(entries) = std::fs::read_dir(&projects_dir) {
        for entry in entries.flatten() {
            let pid_file = entry.path().join("processor.pid");
            if pid_file.exists() {
                tracing::info!(path = %pid_file.display(), "Removing legacy PID file");
                let _ = std::fs::remove_file(&pid_file);
            }
        }
    }
}
