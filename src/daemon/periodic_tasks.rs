//! Prune loop — iterates all active agents in the connection pool.
//! Runs every prune_interval_secs (default 300 = 5 min).
//! Respects per-agent memory lock: skips locked agents.
//!
//! PAS DE COMPACTION. Le systeme utilise merge/suspend/archive
//! geres par l'agent via les MCP tools.

use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ai_smartness::config::GuardianConfig;
use ai_smartness::intelligence::archiver::Archiver;
use ai_smartness::intelligence::decayer::Decayer;
use ai_smartness::intelligence::gossip::Gossip;
use ai_smartness::intelligence::merge_evaluator::MergeEvaluator;
use ai_smartness::registry::registry::AgentRegistry;
use ai_smartness::storage::backup::{BackupConfig, BackupManager};
use ai_smartness::storage::beat::BeatState;
use ai_smartness::storage::cognitive_inbox::CognitiveInbox;
use ai_smartness::storage::database::{self, ConnectionRole};
use ai_smartness::storage::path_utils;
use ai_smartness::storage::threads::ThreadStorage;
use ai_smartness::thread::Thread;
use rusqlite::Connection;

use super::connection_pool::{ConnectionPool, AgentKey};
use super::pool_processor;
use super::pool_writer;

/// Run a single periodic task inside catch_unwind for panic isolation.
/// Uses AssertUnwindSafe because rusqlite::Connection is not RefUnwindSafe
/// (contains RefCell), but we accept this for daemon resilience.
fn run_task(name: &str, task: impl FnOnce()) {
    match std::panic::catch_unwind(AssertUnwindSafe(task)) {
        Ok(()) => {}
        Err(_) => {
            tracing::error!("Task '{}' panicked. Daemon continues.", name);
        }
    }
}

/// Main prune loop — iterates all active agents every `prune_interval` seconds.
pub fn run_prune_loop(
    pool: Arc<ConnectionPool>,
    capture_queue: Option<Arc<super::capture_queue::CaptureQueue>>,
    running: Arc<AtomicBool>,
    prune_interval_secs: u64,
) {
    let interval = Duration::from_secs(prune_interval_secs);
    let eviction_interval = Duration::from_secs(
        ai_smartness::constants::POOL_EVICTION_CHECK_SECS,
    );
    let mut last_eviction = Instant::now();

    while running.load(Ordering::Relaxed) {
        std::thread::sleep(interval);
        if !running.load(Ordering::Relaxed) {
            break;
        }

        // System watchdog — collect once per cycle (shared across all agents)
        let system_metrics = super::watchdog::collect();
        if system_metrics.cpu_usage_percent > 90.0 {
            tracing::warn!(
                cpu = system_metrics.cpu_usage_percent,
                "HIGH CPU usage detected by watchdog"
            );
        }
        if let (Some(vram_used), Some(vram_total)) = (system_metrics.gpu_vram_used_mb, system_metrics.gpu_vram_total_mb) {
            if vram_total > 0 {
                let pct = (vram_used as f64 / vram_total as f64) * 100.0;
                if pct > 90.0 {
                    tracing::warn!(vram_used, vram_total, pct = format!("{:.0}", pct), "HIGH GPU VRAM usage");
                }
            }
        }
        tracing::debug!(
            cpu = format!("{:.1}", system_metrics.cpu_usage_percent),
            ram_used_mb = system_metrics.ram_used_mb,
            ram_available_mb = system_metrics.ram_available_mb,
            gpu_vram_used = ?system_metrics.gpu_vram_used_mb,
            threads = ?system_metrics.thread_count,
            "Watchdog metrics collected"
        );

        let keys = pool.active_keys();
        let agent_count = keys.len();

        if agent_count == 0 {
            tracing::debug!("No active agents in pool, skipping prune cycle");
        } else {
            let cycle_start = Instant::now();
            tracing::info!(agent_count = agent_count, "Starting global prune cycle");

            for key in &keys {
                if !running.load(Ordering::Relaxed) {
                    break;
                }

                // Skip locked agents
                if pool.is_locked(key) {
                    tracing::debug!(
                        project = %key.project_hash,
                        agent = %key.agent_id,
                        "Skipping locked agent"
                    );
                    continue;
                }

                // 0. Beat increment + watchdog metrics + LLM status + backpressure auto-clear
                let metrics_clone = system_metrics.clone();
                run_task("beat", || {
                    let data_dir = path_utils::agent_data_dir(&key.project_hash, &key.agent_id);
                    let mut beat = BeatState::load(&data_dir);
                    beat.increment();
                    // Write system watchdog metrics into beat.json
                    beat.system_metrics = Some(metrics_clone);
                    // Write LLM observability into beat.json
                    let llm = ai_smartness::processing::local_llm::LocalLlm::global();
                    beat.llm_status = Some(llm.status().to_string());
                    let guardian_cfg = {
                        let cfg_path = path_utils::data_dir().join("config.json");
                        std::fs::read_to_string(&cfg_path)
                            .ok()
                            .and_then(|s| serde_json::from_str::<GuardianConfig>(&s).ok())
                            .unwrap_or_default()
                    };
                    beat.llm_backend = Some(format!("{:?}", guardian_cfg.llm_backend));
                    beat.llm_ctx_size = Some(llm.current_ctx_size());
                    beat.llm_gpu_layers = Some(llm.current_gpu_layers());
                    // Auto-clear backpressure if stale (> 10 min safety timeout)
                    if beat.processing_backpressure {
                        if let Some(ref since) = beat.backpressure_since {
                            if let Ok(t) = since.parse::<chrono::DateTime<chrono::Utc>>() {
                                let age_secs = (chrono::Utc::now() - t).num_seconds();
                                if age_secs > 600 {
                                    beat.processing_backpressure = false;
                                    beat.backpressure_since = None;
                                    tracing::info!(
                                        agent = %key.agent_id,
                                        age_secs,
                                        "Backpressure auto-cleared (>10min timeout)"
                                    );
                                }
                            }
                        }
                    }
                    beat.save(&data_dir);
                    tracing::debug!(
                        agent = %key.agent_id,
                        beat = beat.beat,
                        "Beat incremented"
                    );
                });

                // 0b. Quota sync from registry → BeatState + pool cache
                run_task("quota_sync", || {
                    let reg_path = path_utils::registry_db_path();
                    if let Ok(reg_conn) = database::open_connection(&reg_path, ConnectionRole::Daemon) {
                        if let Ok(Some(agent)) = AgentRegistry::get(&reg_conn, &key.agent_id, &key.project_hash) {
                            let quota = agent.thread_mode.quota();
                            let data_dir = path_utils::agent_data_dir(&key.project_hash, &key.agent_id);
                            let mut beat = BeatState::load(&data_dir);
                            if beat.quota != quota {
                                tracing::info!(
                                    agent = %key.agent_id,
                                    old_quota = beat.quota,
                                    new_quota = quota,
                                    "Quota sync: registry → beat.json"
                                );
                                beat.quota = quota;
                                beat.save(&data_dir);
                            }
                            pool.refresh_quota(key, quota);
                        }
                    }
                });

                // Get connection for this agent
                let conn = match pool.get_or_open(key) {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::error!(
                            project = %key.project_hash,
                            agent = %key.agent_id,
                            error = %e,
                            "Failed to get connection for prune"
                        );
                        continue;
                    }
                };

                // If connection mutex is poisoned, evict and reconnect before locking
                let conn = if conn.is_poisoned() {
                    tracing::warn!(
                        project = %key.project_hash,
                        agent = %key.agent_id,
                        "Prune: DB mutex poisoned — evicting and reconnecting"
                    );
                    pool.force_evict(key);
                    match pool.get_or_open(key) {
                        Ok(c) => c,
                        Err(e) => {
                            tracing::error!(agent = %key.agent_id, error = %e,
                                "Prune: failed to reconnect after eviction");
                            continue;
                        }
                    }
                } else {
                    conn
                };

                tracing::debug!(
                    project = %key.project_hash,
                    agent = %key.agent_id,
                    "Running prune cycle for agent"
                );

                // Load GuardianConfig for this prune cycle
                let guardian = {
                    let cfg_path = path_utils::data_dir().join("config.json");
                    std::fs::read_to_string(&cfg_path)
                        .ok()
                        .and_then(|s| serde_json::from_str::<GuardianConfig>(&s).ok())
                        .unwrap_or_default()
                };

                let data_dir = path_utils::agent_data_dir(&key.project_hash, &key.agent_id);
                run_prune_cycle(
                    &conn, &guardian, &data_dir, &key.project_hash, &key.agent_id,
                    capture_queue.as_deref(),
                );

                // 7. Scheduled backup (outside prune cycle — needs key context)
                run_task("backup", || {
                    let config = BackupConfig::load();
                    if !config.is_backup_due() {
                        return;
                    }
                    let backup_dir = std::path::PathBuf::from(
                        path_utils::expand_tilde(&config.backup_path),
                    );
                    match BackupManager::backup_agent(
                        &key.project_hash,
                        &key.agent_id,
                        &backup_dir,
                    ) {
                        Ok(path) => {
                            tracing::info!(
                                agent = %key.agent_id,
                                path = %path.display(),
                                "Scheduled backup"
                            );
                            let mut cfg = BackupConfig::load();
                            cfg.last_backup_at =
                                Some(chrono::Utc::now().to_rfc3339());
                            cfg.save();
                            BackupManager::enforce_retention(
                                &backup_dir,
                                config.retention_count,
                            );
                        }
                        Err(e) => tracing::warn!(
                            agent = %key.agent_id,
                            error = %e,
                            "Scheduled backup failed"
                        ),
                    }
                });
            }

            tracing::info!(
                agent_count = agent_count,
                duration_ms = cycle_start.elapsed().as_millis() as u64,
                "Global prune cycle complete"
            );
        }

        // Periodically evict idle connections
        if last_eviction.elapsed() >= eviction_interval {
            pool.evict_idle();
            last_eviction = Instant::now();
        }
    }

    tracing::info!("Prune loop stopped");
}

/// Pool consumer loop — processes .pending pool files at LLM speed.
/// Runs every 10s, separate from the prune loop (which runs every 5 min).
/// Workers write captures to pool instantly; this thread processes them.
///
/// Discovery: scans the filesystem (projects/*/agents/*/pool/) instead of
/// relying on active_keys(), because non-prompt captures write to pool
/// without opening a DB connection in the connection pool.
pub fn run_pool_consumer_loop(
    pool: Arc<ConnectionPool>,
    running: Arc<AtomicBool>,
) {
    const POOL_SCAN_INTERVAL_SECS: u64 = 10;
    let interval = Duration::from_secs(POOL_SCAN_INTERVAL_SECS);

    while running.load(Ordering::Relaxed) {
        std::thread::sleep(interval);
        if !running.load(Ordering::Relaxed) {
            break;
        }

        let keys = discover_agents_with_pools();
        for key in &keys {
            if !running.load(Ordering::Relaxed) {
                break;
            }
            if pool.is_locked(key) {
                continue;
            }

            let agent_data = path_utils::agent_data_dir(&key.project_hash, &key.agent_id);
            let pool_dir = agent_data.join("pool");
            if !pool_dir.exists() {
                continue;
            }

            // Quick check: any .pending files?
            let has_pending = std::fs::read_dir(&pool_dir)
                .ok()
                .map(|mut entries| {
                    entries.any(|e| {
                        e.ok()
                            .map(|e| {
                                e.path()
                                    .extension()
                                    .map(|x| x == "pending")
                                    .unwrap_or(false)
                            })
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false);
            if !has_pending {
                continue;
            }

            // Get connection + pending context from pool
            let conn = match pool.get_or_open(key) {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!(agent = %key.agent_id, error = %e, "Pool consumer: connection error");
                    continue;
                }
            };
            let conn_guard = match conn.lock() {
                Ok(g) => g,
                Err(_) => {
                    tracing::warn!(agent = %key.agent_id, "Pool consumer: connection mutex poisoned");
                    pool.force_evict(key);
                    continue;
                }
            };
            let pending_arc = match pool.get_pending(key) {
                Ok(p) => p,
                Err(_) => continue,
            };
            let mut pending_guard = match pending_arc.lock() {
                Ok(g) => g,
                Err(poison) => poison.into_inner(),
            };

            let guardian = {
                let cfg_path = path_utils::data_dir().join("config.json");
                std::fs::read_to_string(&cfg_path)
                    .ok()
                    .and_then(|s| serde_json::from_str::<GuardianConfig>(&s).ok())
                    .unwrap_or_default()
            };
            let thread_quota = pool.get_thread_quota(key);

            match pool_processor::process_pending_files(
                &pool_dir,
                &conn_guard,
                &mut pending_guard,
                thread_quota,
                &guardian,
            ) {
                Ok(n) if n > 0 => {
                    tracing::info!(
                        agent = %key.agent_id,
                        captures = n,
                        "Pool consumer: processed"
                    );
                    clear_backpressure(key);
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!(
                        agent = %key.agent_id,
                        error = %e,
                        "Pool consumer: processing error"
                    );
                    set_backpressure(key);
                }
            }
        }
    }

    tracing::info!("Pool consumer loop stopped");
}

/// Force-process all pending pool files across all agents. Used by IPC pool_flush.
/// Returns (total_processed, errors).
pub fn flush_all_pools(pool: &ConnectionPool) -> (usize, Vec<String>) {
    let keys = discover_agents_with_pools();
    let mut total = 0;
    let mut errors = Vec::new();

    for key in &keys {
        if pool.is_locked(key) {
            continue;
        }

        let agent_data = path_utils::agent_data_dir(&key.project_hash, &key.agent_id);
        let pool_dir = agent_data.join("pool");
        if !pool_dir.exists() {
            continue;
        }

        let conn = match pool.get_or_open(key) {
            Ok(c) => c,
            Err(e) => {
                errors.push(format!("{}: {}", key.agent_id, e));
                continue;
            }
        };
        let conn_guard = match conn.lock() {
            Ok(g) => g,
            Err(_) => {
                pool.force_evict(key);
                errors.push(format!("{}: connection mutex poisoned", key.agent_id));
                continue;
            }
        };
        let pending_arc = match pool.get_pending(key) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let mut pending_guard = match pending_arc.lock() {
            Ok(g) => g,
            Err(poison) => poison.into_inner(),
        };

        let guardian = {
            let cfg_path = path_utils::data_dir().join("config.json");
            std::fs::read_to_string(&cfg_path)
                .ok()
                .and_then(|s| serde_json::from_str::<GuardianConfig>(&s).ok())
                .unwrap_or_default()
        };
        let thread_quota = pool.get_thread_quota(key);

        match pool_processor::process_pending_files(
            &pool_dir,
            &conn_guard,
            &mut pending_guard,
            thread_quota,
            &guardian,
        ) {
            Ok(n) => {
                total += n;
                if n > 0 {
                    clear_backpressure(key);
                }
            }
            Err(e) => {
                errors.push(format!("{}: {}", key.agent_id, e));
                set_backpressure(key);
            }
        }
    }

    (total, errors)
}

/// Discover all agents that have a pool/ directory by scanning the filesystem.
/// This is necessary because non-prompt captures write to pool/ without opening
/// a DB connection, so pool.active_keys() misses them entirely.
fn discover_agents_with_pools() -> Vec<AgentKey> {
    let projects_dir = path_utils::projects_dir();
    let mut keys = Vec::new();

    let project_entries = match std::fs::read_dir(&projects_dir) {
        Ok(e) => e,
        Err(_) => return keys,
    };

    for project_entry in project_entries.flatten() {
        let project_hash = project_entry.file_name().to_string_lossy().to_string();
        let agents_dir = project_entry.path().join("agents");
        if !agents_dir.exists() {
            continue;
        }

        let agent_entries = match std::fs::read_dir(&agents_dir) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for agent_entry in agent_entries.flatten() {
            let path = agent_entry.path();
            // Only consider directories (agent data dirs), skip .db files
            if !path.is_dir() {
                continue;
            }
            let pool_dir = path.join("pool");
            if !pool_dir.exists() {
                continue;
            }
            let agent_id = agent_entry.file_name().to_string_lossy().to_string();
            keys.push(AgentKey {
                project_hash: project_hash.clone(),
                agent_id,
            });
        }
    }

    keys
}

/// Signal backpressure ON via BeatState.
fn set_backpressure(key: &AgentKey) {
    let agent_data = path_utils::agent_data_dir(&key.project_hash, &key.agent_id);
    let mut beat = BeatState::load(&agent_data);
    if !beat.processing_backpressure {
        beat.processing_backpressure = true;
        beat.backpressure_since = Some(chrono::Utc::now().to_rfc3339());
        beat.save(&agent_data);
        tracing::info!(agent = %key.agent_id, "Backpressure ON: pool processing failed");
    }
}

/// Clear backpressure via BeatState.
fn clear_backpressure(key: &AgentKey) {
    let agent_data = path_utils::agent_data_dir(&key.project_hash, &key.agent_id);
    let mut beat = BeatState::load(&agent_data);
    if beat.processing_backpressure {
        beat.processing_backpressure = false;
        beat.backpressure_since = None;
        beat.save(&agent_data);
        tracing::info!(agent = %key.agent_id, "Backpressure OFF: pool processing succeeded");
    }
}

/// Single prune cycle for one agent — each task acquires/releases the lock
/// independently to reduce contention (~5s per task instead of ~60s continuous).
fn run_prune_cycle(
    conn_mtx: &Mutex<Connection>,
    guardian: &GuardianConfig,
    agent_data_dir: &std::path::Path,
    project_hash: &str,
    agent_id: &str,
    capture_queue: Option<&super::capture_queue::CaptureQueue>,
) {
    // 1. Gossip v2: concept-based bridge discovery (config-driven limits)
    run_task("gossip", || {
        let Ok(conn) = conn_mtx.lock() else { return };
        let gossip = match Gossip::new(&conn) {
            Ok(g) => g,
            Err(e) => {
                tracing::warn!("Gossip init error: {}", e);
                return;
            }
        };
        match gossip.run_cycle(&conn, &guardian.gossip) {
            Ok((n, merge_candidates)) => {
                if n > 0 {
                    tracing::info!("Gossip v2: created {} bridges", n);
                }
                if !merge_candidates.is_empty() {
                    tracing::info!(
                        "Gossip v2: {} merge candidates (scores: {})",
                        merge_candidates.len(),
                        merge_candidates.iter()
                            .map(|c| format!("{:.2}", c.overlap_score))
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                    let max_per_cycle = ai_smartness::constants::GOSSIP_MERGE_MAX_PER_CYCLE;
                    let auto_threshold = guardian.gossip.merge_auto_threshold;
                    for candidate in merge_candidates.iter().take(max_per_cycle) {
                        if candidate.overlap_score >= auto_threshold {
                            match MergeEvaluator::evaluate_and_execute(&conn, candidate, &guardian.gossip.embedding.mode, &guardian.local_model_size) {
                                Ok(true) => tracing::info!(
                                    "MergeEvaluator: auto-merged (score={:.2})",
                                    candidate.overlap_score
                                ),
                                Ok(false) => tracing::info!(
                                    "MergeEvaluator: rejected (score={:.2})",
                                    candidate.overlap_score
                                ),
                                Err(e) => tracing::warn!("MergeEvaluator error: {}", e),
                            }
                        }
                    }
                }
            }
            Err(e) => tracing::warn!("Gossip error: {}", e),
        }
    });

    // 2. Decay: reduce weights, suspend low-weight threads
    run_task("decay", || {
        let Ok(conn) = conn_mtx.lock() else { return };
        match Decayer::decay_active(&conn, &guardian.decay) {
            Ok(n) => {
                if n > 0 {
                    tracing::info!("Decay: {} threads affected", n);
                }
            }
            Err(e) => tracing::warn!("Decay error: {}", e),
        }
    });

    // 3. Archive: stale suspended -> archived (after config hours)
    run_task("archive", || {
        let Ok(conn) = conn_mtx.lock() else { return };
        match Archiver::archive_stale(&conn, &guardian.decay) {
            Ok(n) => {
                if n > 0 {
                    tracing::info!("Archived: {} threads", n);
                }
            }
            Err(e) => tracing::warn!("Archive error: {}", e),
        }
    });

    // 4. Cognitive inbox cleanup: expire stale messages
    run_task("inbox_cleanup", || {
        let Ok(conn) = conn_mtx.lock() else { return };
        if let Err(e) = CognitiveInbox::expire_stale(&conn) {
            tracing::warn!("Inbox cleanup error: {}", e);
        }
    });

    // 5+6. Work context cleanup + injection decay (shared list_active cache)
    run_task("work_context_and_injection", || {
        let Ok(conn) = conn_mtx.lock() else { return };
        let active = ThreadStorage::list_active(&conn).unwrap_or_default();
        match cleanup_stale_work_contexts(&conn, &active) {
            Ok(n) => {
                if n > 0 {
                    tracing::info!("WorkContext cleanup: {} expired", n);
                }
            }
            Err(e) => tracing::warn!("WorkContext cleanup error: {}", e),
        }
        match decay_injection_scores(&conn, &active) {
            Ok(n) => {
                if n > 0 {
                    tracing::info!("Injection decay: {} threads", n);
                }
            }
            Err(e) => tracing::warn!("Injection decay error: {}", e),
        }
    });

    // 7. Concept backfill — populate empty concepts from topics (1x per day, ~288 beats)
    {
        let beat_state = BeatState::load(agent_data_dir);
        if beat_state.beat % 288 == 0 {
            run_task("concept_backfill", || {
                let Ok(conn) = conn_mtx.lock() else { return };
                let threads = ThreadStorage::list_active(&conn).unwrap_or_default();
                let mut count = 0usize;
                for thread in threads.iter()
                    .filter(|t| t.concepts.is_empty())
                    .take(10)
                {
                    if !thread.topics.is_empty() {
                        let concepts_json = serde_json::to_string(&thread.topics)
                            .unwrap_or_default();
                        ThreadStorage::update_concepts(&conn, &thread.id, &concepts_json).ok();
                        count += 1;
                    }
                }
                if count > 0 {
                    tracing::info!(count, "Concept backfill: populated {} threads", count);
                }
            });
        }
    }

    // 7b. Quality scan: detect threads with empty/degenerate fields, auto-queue enrichment.
    // Lightweight scan — no LLM call. Only submits jobs to the capture queue.
    // Retry max 2x is handled by the capture queue worker (enrichment_retry field).
    if let Some(cq) = capture_queue {
        run_task("quality_scan", || {
            let Ok(conn) = conn_mtx.lock() else { return };
            let threads = ThreadStorage::list_active(&conn).unwrap_or_default();
            let mut queued = 0usize;

            for thread in threads.iter().take(20) {
                if needs_enrichment(thread) {
                    let job = super::capture_queue::CaptureJob {
                        key: AgentKey {
                            project_hash: project_hash.to_string(),
                            agent_id: agent_id.to_string(),
                        },
                        source_type: "quality_scan".to_string(),
                        content: String::new(),
                        file_path: None,
                        is_prompt: false,
                        session_id: None,
                        enrich_thread_id: Some(thread.id.clone()),
                        enrichment_retry: 0,
                    };
                    if cq.submit(job).is_ok() {
                        queued += 1;
                    } else {
                        // Queue full — stop submitting this cycle
                        break;
                    }
                }
            }

            if queued > 0 {
                tracing::info!(queued, "Quality scan: queued {} threads for enrichment", queued);
            }
        });
    }

    // 8. Backup: moved to run_prune_loop (needs key context)

    // 9. Shared orphan cleanup: remove shared_threads entries whose source thread is gone
    run_task("shared_orphan_cleanup", || {
        let Ok(conn) = conn_mtx.lock() else { return };
        if let Err(e) = cleanup_shared_orphans(&conn, project_hash) {
            tracing::warn!("Shared orphan cleanup error: {}", e);
        }
    });

    // 10. SQLite checkpoint (WAL mode)
    run_task("wal_checkpoint", || {
        let Ok(conn) = conn_mtx.lock() else { return };
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").ok();
    });

    // 11. Session GC: remove stale session_agents/ files older than 48h
    run_task("session_gc", || {
        let dir = path_utils::session_agents_dir(project_hash);
        let cutoff = std::time::SystemTime::now()
            .checked_sub(std::time::Duration::from_secs(48 * 3600))
            .unwrap_or(std::time::UNIX_EPOCH);
        gc_session_agents(&dir, cutoff);
    });

    // 12. Pool .done cleanup: remove processed pool files
    run_task("pool_done_cleanup", || {
        let pool_dir = agent_data_dir.join("pool");
        if pool_dir.exists() {
            match pool_writer::cleanup_done_files(&pool_dir, guardian.capture.pool.cleanup_interval_secs) {
                Ok(n) if n > 0 => tracing::info!(cleaned = n, "Pool .done cleanup"),
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %e, "Pool .done cleanup failed"),
            }
        }
    });
}

/// Delete files in `dir` whose mtime is older than `cutoff`.
fn gc_session_agents(dir: &std::path::Path, cutoff: std::time::SystemTime) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.modified().map(|m| m < cutoff).unwrap_or(false) {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::FileTimes;
    use std::time::{Duration, SystemTime};

    #[test]
    fn test_session_gc_deletes_old_files() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let dir_path = dir.path();

        // Old file: mtime set to 72h ago (> 48h cutoff)
        let old_file = dir_path.join("old_session");
        std::fs::write(&old_file, b"old").unwrap();
        let old_mtime = SystemTime::now() - Duration::from_secs(72 * 3600);
        std::fs::OpenOptions::new()
            .write(true)
            .open(&old_file)
            .unwrap()
            .set_times(FileTimes::new().set_modified(old_mtime))
            .unwrap();

        // Recent file: mtime = now (< 48h cutoff)
        let new_file = dir_path.join("new_session");
        std::fs::write(&new_file, b"new").unwrap();

        let cutoff = SystemTime::now()
            .checked_sub(Duration::from_secs(48 * 3600))
            .unwrap();

        gc_session_agents(dir_path, cutoff);

        assert!(!old_file.exists(), "old file should be deleted");
        assert!(new_file.exists(), "new file should survive");
    }
}

/// Clean work_contexts expired > 24h (freshness_factor == 0.0).
fn cleanup_stale_work_contexts(conn: &Connection, active: &[Thread]) -> ai_smartness::AiResult<usize> {
    let mut cleaned = 0;

    for thread in active {
        if let Some(ref wc) = thread.work_context {
            if wc.is_expired() {
                ThreadStorage::clear_work_context(conn, &thread.id)?;
                cleaned += 1;
            }
        }
    }

    Ok(cleaned)
}

/// Decay injection scores for threads injected but never used.
fn decay_injection_scores(conn: &Connection, active: &[Thread]) -> ai_smartness::AiResult<usize> {
    let mut decayed = 0;

    for thread in active {
        if let Some(ref stats) = thread.injection_stats {
            if stats.should_decay() {
                let penalty = stats.compute_relevance_penalty();
                let new_score = (thread.relevance_score - penalty).max(0.1);
                ThreadStorage::update_relevance_score(conn, &thread.id, new_score)?;
                decayed += 1;
            }
        }
    }

    Ok(decayed)
}

/// Detect if a thread has empty or degenerate fields that need LLM enrichment.
/// Used by the quality scan task to auto-queue enrichment for incomplete threads.
fn needs_enrichment(thread: &Thread) -> bool {
    use ai_smartness::processing::extractor::is_placeholder;

    // Empty fields
    if thread.summary.is_none() || thread.summary.as_deref() == Some("") {
        return true;
    }
    if thread.labels.is_empty() {
        return true;
    }
    if thread.concepts.is_empty() {
        return true;
    }
    if thread.topics.is_empty() {
        return true;
    }
    if thread.embedding.is_none() {
        return true;
    }

    // Degenerate fields (LLM placeholder output)
    if let Some(ref s) = thread.summary {
        if is_placeholder(s) {
            return true;
        }
    }
    if is_placeholder(&thread.title) {
        return true;
    }

    false
}

/// Remove shared_threads entries whose source thread no longer exists in the agent DB.
fn cleanup_shared_orphans(agent_conn: &Connection, project_hash: &str) -> ai_smartness::AiResult<usize> {
    use ai_smartness::storage::database::{self, ConnectionRole};
    use ai_smartness::storage::shared_storage::SharedStorage;

    let shared_db_path = path_utils::shared_db_path(project_hash);
    if !shared_db_path.exists() {
        return Ok(0);
    }
    let shared_conn = database::open_connection(&shared_db_path, ConnectionRole::Daemon)?;

    let published = SharedStorage::list_published(&shared_conn)?;
    let mut cleaned = 0;

    for shared in &published {
        if ThreadStorage::get(agent_conn, &shared.thread_id)?.is_none() {
            SharedStorage::unpublish(&shared_conn, &shared.shared_id)?;
            tracing::debug!(shared_id = %shared.shared_id, thread_id = %shared.thread_id, "Unpublished orphan shared thread");
            cleaned += 1;
        }
    }

    if cleaned > 0 {
        tracing::info!(cleaned, "Shared orphan cleanup complete");
    }

    Ok(cleaned)
}
