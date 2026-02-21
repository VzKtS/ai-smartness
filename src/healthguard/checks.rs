//! Health check SQL queries — individual checks called by HealthGuard.analyze().

use crate::storage::beat::BeatState;
use crate::thread::ThreadStatus;
use crate::storage::threads::ThreadStorage;
use crate::AiResult;
use rusqlite::Connection;

use super::{HealthFinding, HealthGuardConfig, HealthPriority};

/// Check 1: Memory capacity (active threads / quota).
pub fn check_capacity(
    conn: &Connection,
    config: &HealthGuardConfig,
    thread_quota: usize,
) -> Option<HealthFinding> {
    let active_count = ThreadStorage::count_by_status(conn, &ThreadStatus::Active).ok()?;
    let ratio = active_count as f64 / thread_quota.max(1) as f64;

    if ratio >= config.capacity_critical_percent {
        Some(HealthFinding {
            priority: HealthPriority::Critical,
            category: "capacity".to_string(),
            message: format!(
                "Memory at {}% capacity ({}/{})",
                (ratio * 100.0) as u32, active_count, thread_quota
            ),
            action: "Use ai_thread_suspend or merge threads to free slots".to_string(),
            metric_value: ratio,
            threshold: config.capacity_critical_percent,
        })
    } else if ratio >= config.capacity_warning_percent {
        Some(HealthFinding {
            priority: HealthPriority::High,
            category: "capacity".to_string(),
            message: format!(
                "Memory at {}% capacity ({}/{})",
                (ratio * 100.0) as u32, active_count, thread_quota
            ),
            action: "Consider suspending old threads".to_string(),
            metric_value: ratio,
            threshold: config.capacity_warning_percent,
        })
    } else {
        None
    }
}

/// Check 2: Fragmentation (single-message threads %).
pub fn check_fragmentation(
    conn: &Connection,
    config: &HealthGuardConfig,
) -> Option<HealthFinding> {
    let active_count = ThreadStorage::count_by_status(conn, &ThreadStatus::Active).ok()?;
    if active_count < config.fragmentation_min_threads {
        return None;
    }

    let single_msg_count = count_single_message_threads(conn).ok()?;
    let ratio = single_msg_count as f64 / active_count.max(1) as f64;

    if ratio >= config.fragmentation_ratio_threshold {
        Some(HealthFinding {
            priority: HealthPriority::Medium,
            category: "fragmentation".to_string(),
            message: format!(
                "{}% of threads have only 1 message ({}/{})",
                (ratio * 100.0) as u32, single_msg_count, active_count
            ),
            action: "Use ai_merge to consolidate similar threads".to_string(),
            metric_value: ratio,
            threshold: config.fragmentation_ratio_threshold,
        })
    } else {
        None
    }
}

/// Check 3: Unlabeled threads.
pub fn check_unlabeled(conn: &Connection, config: &HealthGuardConfig) -> Option<HealthFinding> {
    let active_count = ThreadStorage::count_by_status(conn, &ThreadStatus::Active).ok()?;
    if active_count < config.unlabeled_min_threads {
        return None;
    }

    let unlabeled = count_unlabeled_threads(conn).ok()?;
    let ratio = unlabeled as f64 / active_count.max(1) as f64;

    if ratio >= config.unlabeled_ratio_threshold {
        Some(HealthFinding {
            priority: HealthPriority::Low,
            category: "labels".to_string(),
            message: format!(
                "{}% of threads are unlabeled ({}/{})",
                (ratio * 100.0) as u32, unlabeled, active_count
            ),
            action: "Use ai_label to add labels for better retrieval".to_string(),
            metric_value: ratio,
            threshold: config.unlabeled_ratio_threshold,
        })
    } else {
        None
    }
}

/// Check 4: Weak bridges.
pub fn check_weak_bridges(
    conn: &Connection,
    config: &HealthGuardConfig,
) -> Option<HealthFinding> {
    let weak_count = count_weak_bridges(conn).ok()?;
    if weak_count >= config.weak_bridges_threshold {
        Some(HealthFinding {
            priority: HealthPriority::Low,
            category: "bridges".to_string(),
            message: format!("{} weak bridges detected", weak_count),
            action: "Use ai_bridge_scan_orphans to clean up".to_string(),
            metric_value: weak_count as f64,
            threshold: config.weak_bridges_threshold as f64,
        })
    } else {
        None
    }
}

/// Check 5: Stale active threads.
pub fn check_stale_threads(
    conn: &Connection,
    config: &HealthGuardConfig,
) -> Option<HealthFinding> {
    let stale_count = count_stale_active_threads(conn, config.stale_thread_hours).ok()?;
    if stale_count >= config.stale_thread_count_threshold {
        Some(HealthFinding {
            priority: HealthPriority::Medium,
            category: "stale".to_string(),
            message: format!(
                "{} active threads with no activity for {}+ hours",
                stale_count, config.stale_thread_hours
            ),
            action: "Use ai_thread_suspend to suspend inactive threads".to_string(),
            metric_value: stale_count as f64,
            threshold: config.stale_thread_count_threshold as f64,
        })
    } else {
        None
    }
}

/// Check 6: Disk usage.
pub fn check_disk_usage(
    config: &HealthGuardConfig,
    ai_path: &std::path::Path,
) -> Option<HealthFinding> {
    let size = get_db_size(ai_path).ok()?;
    if size >= config.disk_warning_bytes {
        Some(HealthFinding {
            priority: HealthPriority::High,
            category: "disk".to_string(),
            message: format!("Database size: {} MB", size / 1_000_000),
            action: "Use ai_backup and consider archiving old threads".to_string(),
            metric_value: size as f64,
            threshold: config.disk_warning_bytes as f64,
        })
    } else {
        None
    }
}

/// Check 7: Recall staleness — agent hasn't used ai_recall recently despite rich memory.
pub fn check_recall_staleness(
    conn: &Connection,
    _config: &HealthGuardConfig,
    beat_state: &BeatState,
) -> Option<HealthFinding> {
    let active = ThreadStorage::count_by_status(conn, &ThreadStatus::Active).ok()?;
    if active < 10 {
        return None; // Not enough threads to warrant recall
    }

    let beats_since_recall = beat_state.beat.saturating_sub(beat_state.last_recall_beat);
    let threshold = 15u64; // ~75 minutes at 5-min beats

    if beats_since_recall > threshold {
        Some(HealthFinding {
            priority: HealthPriority::Medium,
            category: "recall_staleness".to_string(),
            message: format!(
                "No ai_recall usage in {} prompts with {} active threads. Memory context may be stale.",
                beats_since_recall, active
            ),
            action: "Run ai_recall with keywords related to your current task.".to_string(),
            metric_value: beats_since_recall as f64,
            threshold: threshold as f64,
        })
    } else {
        None
    }
}

// ── SQL helpers ──

fn count_single_message_threads(conn: &Connection) -> AiResult<usize> {
    conn.query_row(
        "SELECT COUNT(*) FROM threads WHERE status = 'active' \
         AND id IN (SELECT thread_id FROM thread_messages GROUP BY thread_id HAVING COUNT(*) <= 1)",
        [],
        |r| r.get::<_, usize>(0),
    )
    .map_err(|e| crate::AiError::Storage(e.to_string()))
}

fn count_unlabeled_threads(conn: &Connection) -> AiResult<usize> {
    conn.query_row(
        "SELECT COUNT(*) FROM threads WHERE status = 'active' AND (labels IS NULL OR labels = '[]')",
        [],
        |r| r.get::<_, usize>(0),
    )
    .map_err(|e| crate::AiError::Storage(e.to_string()))
}

fn count_weak_bridges(conn: &Connection) -> AiResult<usize> {
    conn.query_row(
        "SELECT COUNT(*) FROM bridges WHERE weight < 0.1 AND status IN ('active', 'weak')",
        [],
        |r| r.get::<_, usize>(0),
    )
    .map_err(|e| crate::AiError::Storage(e.to_string()))
}

fn count_stale_active_threads(conn: &Connection, hours: u64) -> AiResult<usize> {
    conn.query_row(
        "SELECT COUNT(*) FROM threads WHERE status = 'active' \
         AND last_active < datetime('now', '-' || ?1 || ' hours')",
        rusqlite::params![hours],
        |r| r.get::<_, usize>(0),
    )
    .map_err(|e| crate::AiError::Storage(e.to_string()))
}

fn get_db_size(ai_path: &std::path::Path) -> std::io::Result<u64> {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(ai_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                let ext_str = ext.to_string_lossy();
                if ext_str == "db" || ext_str == "db-wal" || ext_str == "db-shm" {
                    if let Ok(meta) = std::fs::metadata(&path) {
                        total += meta.len();
                    }
                }
            }
        }
    }
    Ok(total)
}
