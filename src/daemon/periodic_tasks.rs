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
                run_prune_cycle(&conn, &guardian, &data_dir, &key.project_hash);

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

/// Single prune cycle for one agent — each task acquires/releases the lock
/// independently to reduce contention (~5s per task instead of ~60s continuous).
fn run_prune_cycle(conn_mtx: &Mutex<Connection>, guardian: &GuardianConfig, agent_data_dir: &std::path::Path, project_hash: &str) {
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
                            match MergeEvaluator::evaluate_and_execute(&conn, candidate, &guardian.gossip.embedding.mode) {
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
