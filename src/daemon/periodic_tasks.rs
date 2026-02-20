//! Prune loop — iterates all active agents in the connection pool.
//! Runs every prune_interval_secs (default 300 = 5 min).
//! Respects per-agent memory lock: skips locked agents.
//!
//! PAS DE COMPACTION. Le systeme utilise merge/suspend/archive
//! geres par l'agent via les MCP tools.

use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ai_smartness::config::GuardianConfig;
use ai_smartness::intelligence::archiver::Archiver;
use ai_smartness::intelligence::decayer::Decayer;
use ai_smartness::intelligence::gossip::Gossip;
use ai_smartness::intelligence::merge_evaluator::MergeEvaluator;
use ai_smartness::storage::backup::{BackupConfig, BackupManager};
use ai_smartness::storage::beat::BeatState;
use ai_smartness::storage::cognitive_inbox::CognitiveInbox;
use ai_smartness::storage::path_utils;
use ai_smartness::storage::threads::ThreadStorage;
use rusqlite::Connection;

use super::connection_pool::ConnectionPool;

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

                // 0. Beat increment (no DB needed — filesystem only)
                run_task("beat", || {
                    let data_dir = path_utils::agent_data_dir(&key.project_hash, &key.agent_id);
                    let mut beat = BeatState::load(&data_dir);
                    beat.increment();
                    beat.save(&data_dir);
                    tracing::debug!(
                        agent = %key.agent_id,
                        beat = beat.beat,
                        "Beat incremented"
                    );
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

                let conn_guard = match conn.lock() {
                    Ok(g) => g,
                    Err(e) => {
                        tracing::error!(
                            project = %key.project_hash,
                            agent = %key.agent_id,
                            error = %e,
                            "Failed to lock connection for prune"
                        );
                        continue;
                    }
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

                run_prune_cycle(&conn_guard, &guardian);

                // Drop conn_guard before backup (which opens its own connection)
                drop(conn_guard);

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

/// Single prune cycle for one agent — runs all 8 tasks sequentially.
fn run_prune_cycle(conn: &Connection, guardian: &GuardianConfig) {
    // 1. Gossip v2: concept-based bridge discovery (config-driven limits)
    run_task("gossip", || {
        let gossip = match Gossip::new(conn) {
            Ok(g) => g,
            Err(e) => {
                tracing::warn!("Gossip init error: {}", e);
                return;
            }
        };
        match gossip.run_cycle(conn, &guardian.gossip) {
            Ok((n, merge_candidates)) => {
                if n > 0 {
                    tracing::info!("Gossip v2: created {} bridges", n);
                }
                // Evaluate merge candidates
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
                            match MergeEvaluator::evaluate_and_execute(conn, candidate) {
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
                        // 0.60-0.85: stored for HealthGuard suggestion (future)
                    }
                }
            }
            Err(e) => tracing::warn!("Gossip error: {}", e),
        }
    });

    // 2. Decay: reduce weights, suspend low-weight threads
    run_task("decay", || match Decayer::decay_active(conn, &guardian.decay) {
        Ok(n) => {
            if n > 0 {
                tracing::info!("Decay: {} threads affected", n);
            }
        }
        Err(e) => tracing::warn!("Decay error: {}", e),
    });

    // 3. Archive: stale suspended -> archived (after config hours)
    run_task("archive", || match Archiver::archive_stale(conn, &guardian.decay) {
        Ok(n) => {
            if n > 0 {
                tracing::info!("Archived: {} threads", n);
            }
        }
        Err(e) => tracing::warn!("Archive error: {}", e),
    });

    // 4. Cognitive inbox cleanup: expire stale messages
    run_task("inbox_cleanup", || {
        if let Err(e) = CognitiveInbox::expire_stale(conn) {
            tracing::warn!("Inbox cleanup error: {}", e);
        }
    });

    // 5. Work context cleanup: clear expired work_contexts (> 24h)
    run_task("work_context_cleanup", || {
        match cleanup_stale_work_contexts(conn) {
            Ok(n) => {
                if n > 0 {
                    tracing::info!("WorkContext cleanup: {} expired", n);
                }
            }
            Err(e) => tracing::warn!("WorkContext cleanup error: {}", e),
        }
    });

    // 6. Injection tracking decay: reduce injection scores for unused threads
    run_task("injection_decay", || {
        match decay_injection_scores(conn) {
            Ok(n) => {
                if n > 0 {
                    tracing::info!("Injection decay: {} threads", n);
                }
            }
            Err(e) => tracing::warn!("Injection decay error: {}", e),
        }
    });

    // 7. Backup: moved to run_prune_loop (needs key context)

    // 8. SQLite checkpoint (WAL mode)
    run_task("wal_checkpoint", || {
        conn.execute_batch("PRAGMA wal_checkpoint(PASSIVE);").ok();
    });
}

/// Clean work_contexts expired > 24h (freshness_factor == 0.0).
fn cleanup_stale_work_contexts(conn: &Connection) -> ai_smartness::AiResult<usize> {
    let active = ThreadStorage::list_active(conn)?;
    let mut cleaned = 0;

    for mut thread in active {
        if let Some(ref wc) = thread.work_context {
            if wc.is_expired() {
                thread.work_context = None;
                ThreadStorage::update(conn, &thread)?;
                cleaned += 1;
            }
        }
    }

    Ok(cleaned)
}

/// Decay injection scores for threads injected but never used.
fn decay_injection_scores(conn: &Connection) -> ai_smartness::AiResult<usize> {
    let active = ThreadStorage::list_active(conn)?;
    let mut decayed = 0;

    for mut thread in active {
        if let Some(ref stats) = thread.injection_stats {
            if stats.should_decay() {
                let penalty = stats.compute_relevance_penalty();
                thread.relevance_score = (thread.relevance_score - penalty).max(0.1);
                ThreadStorage::update(conn, &thread)?;
                decayed += 1;
            }
        }
    }

    Ok(decayed)
}
