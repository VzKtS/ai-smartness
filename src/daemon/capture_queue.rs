//! Capture Queue — bounded MPSC queue with configurable worker thread pool.
//!
//! Architecture:
//!   IPC handler → CaptureQueue::submit(job) → instant response to hook
//!   N worker threads consume jobs → processor::process_capture() (LLM blocking)
//!
//! Designed for dozens of parallel agents: each capture is queued instantly,
//! workers process in parallel. If queue is full, job is dropped with warning
//! (hooks must never block).

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender, Receiver};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use ai_smartness::config::GuardianConfig;
use ai_smartness::registry::registry::AgentRegistry;
use ai_smartness::storage::database::{open_connection, ConnectionRole};
use ai_smartness::storage::path_utils;

use super::connection_pool::{AgentKey, ConnectionPool};
use super::processor;

/// A capture job to be processed asynchronously by a worker.
pub struct CaptureJob {
    pub key: AgentKey,
    pub source_type: String,
    pub content: String,
    pub file_path: Option<String>,
    pub is_prompt: bool,
    /// Optional session_id for prompt captures.
    pub session_id: Option<String>,
}

/// Live stats for the capture queue, shared across workers.
pub struct QueueStats {
    pending: AtomicUsize,
    processed: AtomicU64,
    errors: AtomicU64,
    workers: usize,
}

impl QueueStats {
    fn new(workers: usize) -> Self {
        Self {
            pending: AtomicUsize::new(0),
            processed: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            workers,
        }
    }
}

/// Thread-safe capture queue with worker pool.
pub struct CaptureQueue {
    tx: SyncSender<CaptureJob>,
    stats: Arc<QueueStats>,
    worker_handles: Mutex<Vec<JoinHandle<()>>>,
}

impl CaptureQueue {
    /// Create queue and spawn `num_workers` consumer threads.
    ///
    /// `capacity`: max buffered jobs. If full, `submit()` returns Err (non-blocking).
    pub fn new(
        pool: Arc<ConnectionPool>,
        num_workers: usize,
        capacity: usize,
    ) -> Self {
        let (tx, rx) = sync_channel::<CaptureJob>(capacity);
        let rx = Arc::new(Mutex::new(rx));
        let stats = Arc::new(QueueStats::new(num_workers));
        let mut handles = Vec::with_capacity(num_workers);

        tracing::info!(
            workers = num_workers,
            capacity = capacity,
            "Capture queue initialized"
        );

        for worker_id in 0..num_workers {
            let rx = rx.clone();
            let pool = pool.clone();
            let stats = stats.clone();

            let handle = std::thread::Builder::new()
                .name(format!("capture-worker-{}", worker_id))
                .spawn(move || {
                    tracing::info!(worker_id, "Capture worker started");
                    worker_loop(worker_id, rx, pool, stats);
                    tracing::info!(worker_id, "Capture worker stopped");
                })
                .expect("Failed to spawn capture worker thread");

            handles.push(handle);
        }

        Self {
            tx,
            stats,
            worker_handles: Mutex::new(handles),
        }
    }

    /// Submit a capture job. Returns immediately.
    /// Returns Err if queue is full (job is NOT processed — acceptable under load).
    pub fn submit(&self, job: CaptureJob) -> Result<(), CaptureJob> {
        self.stats.pending.fetch_add(1, Ordering::Relaxed);
        match self.tx.try_send(job) {
            Ok(()) => {
                tracing::debug!("Capture job queued");
                Ok(())
            }
            Err(std::sync::mpsc::TrySendError::Full(job)) => {
                self.stats.pending.fetch_sub(1, Ordering::Relaxed);
                tracing::warn!(
                    pending = self.stats.pending.load(Ordering::Relaxed),
                    "Capture queue full — job dropped"
                );
                Err(job)
            }
            Err(std::sync::mpsc::TrySendError::Disconnected(job)) => {
                self.stats.pending.fetch_sub(1, Ordering::Relaxed);
                tracing::error!("Capture queue disconnected — workers dead?");
                Err(job)
            }
        }
    }

    /// Get current queue statistics.
    pub fn queue_stats(&self) -> serde_json::Value {
        serde_json::json!({
            "pending": self.stats.pending.load(Ordering::Relaxed),
            "processed": self.stats.processed.load(Ordering::Relaxed),
            "errors": self.stats.errors.load(Ordering::Relaxed),
            "workers": self.stats.workers,
        })
    }

    /// Graceful shutdown: drop the sender (workers will exit after draining),
    /// then join all worker threads.
    pub fn shutdown(self) {
        // tx is dropped when self is dropped — workers will see disconnected
        drop(self.tx);

        if let Ok(mut handles) = self.worker_handles.lock() {
            tracing::info!(count = handles.len(), "Waiting for capture workers to finish");
            for handle in handles.drain(..) {
                let _ = handle.join();
            }
            tracing::info!("All capture workers stopped");
        }
    }
}

/// Load the thread quota from the registry DB on first access, cache in pool.
fn ensure_quota_cached(pool: &ConnectionPool, key: &AgentKey) {
    if pool.is_quota_initialized(key) {
        return;
    }

    let reg_path = path_utils::registry_db_path();
    match open_connection(&reg_path, ConnectionRole::Daemon) {
        Ok(reg_conn) => {
            match AgentRegistry::get(&reg_conn, &key.agent_id, &key.project_hash) {
                Ok(Some(agent)) => {
                    let quota = agent.thread_mode.quota();
                    pool.set_thread_quota(key, quota);
                    tracing::info!(
                        agent = %key.agent_id,
                        thread_mode = %agent.thread_mode,
                        quota = quota,
                        "Thread quota loaded from registry"
                    );
                }
                Ok(None) => {
                    // Agent not found in registry — use conservative default (Light=15).
                    // Better to under-allocate than to let an unregistered agent
                    // create 50+ threads with no mode constraint.
                    pool.set_thread_quota(key, 15);
                    tracing::warn!(agent = %key.agent_id, "Agent not in registry, using conservative default quota=15 (Light)");
                }
                Err(e) => {
                    pool.set_thread_quota(key, 15);
                    tracing::warn!(agent = %key.agent_id, error = %e, "Failed to read agent from registry, using default quota=15");
                }
            }
        }
        Err(e) => {
            pool.set_thread_quota(key, 15);
            tracing::warn!(error = %e, "Failed to open registry DB for quota lookup, using default quota=15");
        }
    }
}

/// Worker loop: consume jobs from the shared receiver, process each one.
fn worker_loop(
    worker_id: usize,
    rx: Arc<Mutex<Receiver<CaptureJob>>>,
    pool: Arc<ConnectionPool>,
    stats: Arc<QueueStats>,
) {
    loop {
        // Lock the receiver briefly to grab one job
        let job = {
            let rx_guard = match rx.lock() {
                Ok(g) => g,
                Err(_) => {
                    tracing::error!(worker_id, "Receiver mutex poisoned — worker exiting");
                    return;
                }
            };
            match rx_guard.recv() {
                Ok(job) => job,
                Err(_) => {
                    // Channel disconnected — shutdown
                    tracing::debug!(worker_id, "Channel closed — worker exiting");
                    return;
                }
            }
        };

        stats.pending.fetch_sub(1, Ordering::Relaxed);
        let start = std::time::Instant::now();

        tracing::info!(
            worker_id,
            project = %job.key.project_hash,
            agent = %job.key.agent_id,
            source = %job.source_type,
            content_len = job.content.len(),
            is_prompt = job.is_prompt,
            "Worker processing capture"
        );

        // Ensure thread quota is cached for this agent (lazy load from registry)
        ensure_quota_cached(&pool, &job.key);
        let thread_quota = pool.get_thread_quota(&job.key);

        // Sync quota into BeatState so MCP tools can read it
        {
            let agent_data = path_utils::agent_data_dir(&job.key.project_hash, &job.key.agent_id);
            let mut beat_state = ai_smartness::storage::beat::BeatState::load(&agent_data);
            if beat_state.quota != thread_quota {
                beat_state.quota = thread_quota;
                beat_state.save(&agent_data);
            }
        }

        // Load GuardianConfig from config.json (reload per-job, ~1ms)
        let guardian = {
            let cfg_path = path_utils::data_dir().join("config.json");
            std::fs::read_to_string(&cfg_path)
                .ok()
                .and_then(|s| serde_json::from_str::<GuardianConfig>(&s).ok())
                .unwrap_or_default()
        };

        // Get connection + pending context from pool
        let conn = match pool.get_or_open(&job.key) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(
                    worker_id,
                    agent = %job.key,
                    error = %e,
                    "Worker failed to get DB connection"
                );
                stats.errors.fetch_add(1, Ordering::Relaxed);
                continue;
            }
        };

        let pending = match pool.get_pending(&job.key) {
            Ok(p) => p,
            Err(e) => {
                tracing::error!(
                    worker_id,
                    agent = %job.key,
                    error = %e,
                    "Worker failed to get pending context"
                );
                stats.errors.fetch_add(1, Ordering::Relaxed);
                continue;
            }
        };

        // If connection mutex is poisoned, evict and reconnect before locking
        let conn = if conn.is_poisoned() {
            tracing::warn!(worker_id, agent = %job.key,
                "DB connection mutex poisoned — evicting and reconnecting");
            pool.force_evict(&job.key);
            match pool.get_or_open(&job.key) {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!(worker_id, error = %e,
                        "Failed to reconnect after eviction");
                    stats.errors.fetch_add(1, Ordering::Relaxed);
                    continue;
                }
            }
        } else {
            conn
        };

        let conn_guard = match conn.lock() {
            Ok(g) => g,
            Err(e) => {
                tracing::error!(worker_id, error = %e, "Worker failed to lock DB connection");
                stats.errors.fetch_add(1, Ordering::Relaxed);
                continue;
            }
        };

        let mut pending_guard = match pending.lock() {
            Ok(g) => g,
            Err(poison) => {
                tracing::warn!(worker_id, agent = %job.key,
                    "Pending context mutex poisoned — recovering inner value");
                poison.into_inner()
            }
        };

        // Wrap processing in catch_unwind to prevent future Mutex poisoning
        let job_key = job.key.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if job.is_prompt {
                processor::process_prompt(
                    &conn_guard,
                    &mut pending_guard,
                    &job.content,
                    job.session_id.as_deref(),
                    thread_quota,
                    &guardian,
                )
            } else {
                processor::process_capture(
                    &conn_guard,
                    &mut pending_guard,
                    &job.source_type,
                    &job.content,
                    job.file_path.as_deref(),
                    thread_quota,
                    &guardian,
                )
            }
        }));

        // Explicitly drop guards before handling result
        drop(conn_guard);
        drop(pending_guard);

        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(Ok(tid)) => {
                stats.processed.fetch_add(1, Ordering::Relaxed);
                tracing::info!(
                    worker_id,
                    agent = %job_key,
                    thread_id = ?tid,
                    duration_ms,
                    "Worker capture complete"
                );
            }
            Ok(Err(e)) => {
                stats.errors.fetch_add(1, Ordering::Relaxed);
                tracing::error!(
                    worker_id,
                    agent = %job_key,
                    error = %e,
                    duration_ms,
                    "Worker capture failed"
                );
            }
            Err(panic_payload) => {
                stats.errors.fetch_add(1, Ordering::Relaxed);
                let panic_msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "unknown panic (non-string payload)".to_string()
                };
                tracing::error!(
                    worker_id,
                    agent = %job_key,
                    panic_message = %panic_msg,
                    duration_ms,
                    "Worker capture PANICKED — evicting connection to prevent poison cascade"
                );
                pool.force_evict(&job_key);
            }
        }
    }
}
