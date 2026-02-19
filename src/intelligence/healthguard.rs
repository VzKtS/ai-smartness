//! HealthGuard -- proactive memory health monitoring.
//!
//! Analyzes memory state and generates health findings for injection.
//! 9 checks: capacity, fragmentation, unlabeled, weak bridges,
//! stale threads, merge candidates, poor titles, disk usage, guardian alerts.

use crate::thread::ThreadStatus;
use crate::AiResult;
use crate::storage::threads::ThreadStorage;
use rusqlite::Connection;
use std::path::Path;

/// Health finding priority.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum HealthPriority {
    Low = 0,
    Medium = 1,
    High = 2,
    Critical = 3,
}

impl std::fmt::Display for HealthPriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Critical => write!(f, "CRITICAL"),
            Self::High => write!(f, "HIGH"),
            Self::Medium => write!(f, "MEDIUM"),
            Self::Low => write!(f, "LOW"),
        }
    }
}

/// Individual health finding.
#[derive(Debug, Clone)]
pub struct HealthFinding {
    pub priority: HealthPriority,
    pub category: String,
    pub message: String,
    pub action: String,
    pub metric_value: f64,
    pub threshold: f64,
}

/// HealthGuard configuration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HealthGuardConfig {
    pub enabled: bool,
    pub cooldown_secs: u64,
    pub max_suggestions: usize,
    pub capacity_warning_percent: f64,
    pub capacity_critical_percent: f64,
    pub fragmentation_ratio_threshold: f64,
    pub fragmentation_min_threads: usize,
    pub unlabeled_ratio_threshold: f64,
    pub unlabeled_min_threads: usize,
    pub weak_bridges_threshold: usize,
    pub stale_thread_hours: u64,
    pub stale_thread_count_threshold: usize,
    pub poor_titles_threshold: usize,
    pub disk_warning_bytes: u64,
    /// Custom injection prompts (editable from GUI).
    #[serde(default)]
    pub prompts: HealthGuardPrompts,
}

/// Custom prompts for HealthGuard injection. Empty strings use defaults.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct HealthGuardPrompts {
    /// Header prepended to all findings. Default: "Memory health alerts:"
    #[serde(default)]
    pub header: String,
    /// Template for capacity warnings. Placeholders: {percent}, {quota}
    #[serde(default)]
    pub capacity_warning: String,
    /// Custom onboarding prompt. Empty = use built-in.
    #[serde(default)]
    pub onboarding: String,
}

impl Default for HealthGuardConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            cooldown_secs: 1800,
            max_suggestions: 3,
            capacity_warning_percent: 0.75,
            capacity_critical_percent: 0.90,
            fragmentation_ratio_threshold: 0.30,
            fragmentation_min_threads: 8,
            unlabeled_ratio_threshold: 0.40,
            unlabeled_min_threads: 10,
            weak_bridges_threshold: 50,
            stale_thread_hours: 168,
            stale_thread_count_threshold: 5,
            poor_titles_threshold: 5,
            disk_warning_bytes: 50_000_000,
            prompts: HealthGuardPrompts::default(),
        }
    }
}

pub struct HealthGuard {
    config: HealthGuardConfig,
}

impl HealthGuard {
    pub fn new(config: HealthGuardConfig) -> Self {
        Self { config }
    }

    /// Main health analysis. Returns None if disabled, in cooldown, or healthy.
    pub fn analyze(
        &self,
        conn: &Connection,
        ai_path: &Path,
        thread_quota: usize,
    ) -> Option<Vec<HealthFinding>> {
        if !self.config.enabled {
            return None;
        }

        if self.in_cooldown(ai_path) {
            return None;
        }

        let mut findings = Vec::new();

        // 1. Memory capacity
        if let Ok(active_count) = ThreadStorage::count_by_status(conn, &ThreadStatus::Active) {
            let ratio = active_count as f64 / thread_quota.max(1) as f64;
            if ratio >= self.config.capacity_critical_percent {
                findings.push(HealthFinding {
                    priority: HealthPriority::Critical,
                    category: "capacity".to_string(),
                    message: format!(
                        "Memory at {}% capacity ({}/{})",
                        (ratio * 100.0) as u32, active_count, thread_quota
                    ),
                    action: "Use ai_thread_suspend or merge threads to free slots".to_string(),
                    metric_value: ratio,
                    threshold: self.config.capacity_critical_percent,
                });
            } else if ratio >= self.config.capacity_warning_percent {
                findings.push(HealthFinding {
                    priority: HealthPriority::High,
                    category: "capacity".to_string(),
                    message: format!(
                        "Memory at {}% capacity ({}/{})",
                        (ratio * 100.0) as u32, active_count, thread_quota
                    ),
                    action: "Consider suspending old threads".to_string(),
                    metric_value: ratio,
                    threshold: self.config.capacity_warning_percent,
                });
            }
        }

        // 2. Fragmentation (single-message threads)
        if let Ok(active_count) = ThreadStorage::count_by_status(conn, &ThreadStatus::Active) {
            if active_count >= self.config.fragmentation_min_threads {
                let single_msg_count = count_single_message_threads(conn).unwrap_or(0);
                let ratio = single_msg_count as f64 / active_count.max(1) as f64;
                if ratio >= self.config.fragmentation_ratio_threshold {
                    findings.push(HealthFinding {
                        priority: HealthPriority::Medium,
                        category: "fragmentation".to_string(),
                        message: format!(
                            "{}% of threads have only 1 message ({}/{})",
                            (ratio * 100.0) as u32, single_msg_count, active_count
                        ),
                        action: "Use ai_merge to consolidate similar threads".to_string(),
                        metric_value: ratio,
                        threshold: self.config.fragmentation_ratio_threshold,
                    });
                }
            }
        }

        // 3. Unlabeled threads
        if let Ok(active_count) = ThreadStorage::count_by_status(conn, &ThreadStatus::Active) {
            if active_count >= self.config.unlabeled_min_threads {
                let unlabeled = count_unlabeled_threads(conn).unwrap_or(0);
                let ratio = unlabeled as f64 / active_count.max(1) as f64;
                if ratio >= self.config.unlabeled_ratio_threshold {
                    findings.push(HealthFinding {
                        priority: HealthPriority::Low,
                        category: "labels".to_string(),
                        message: format!(
                            "{}% of threads are unlabeled ({}/{})",
                            (ratio * 100.0) as u32, unlabeled, active_count
                        ),
                        action: "Use ai_label to add labels for better retrieval".to_string(),
                        metric_value: ratio,
                        threshold: self.config.unlabeled_ratio_threshold,
                    });
                }
            }
        }

        // 4. Weak bridges
        if let Ok(weak_count) = count_weak_bridges(conn) {
            if weak_count >= self.config.weak_bridges_threshold {
                findings.push(HealthFinding {
                    priority: HealthPriority::Low,
                    category: "bridges".to_string(),
                    message: format!("{} weak bridges detected", weak_count),
                    action: "Use ai_bridge_scan_orphans to clean up".to_string(),
                    metric_value: weak_count as f64,
                    threshold: self.config.weak_bridges_threshold as f64,
                });
            }
        }

        // 5. Stale active threads
        if let Ok(stale_count) =
            count_stale_active_threads(conn, self.config.stale_thread_hours)
        {
            if stale_count >= self.config.stale_thread_count_threshold {
                findings.push(HealthFinding {
                    priority: HealthPriority::Medium,
                    category: "stale".to_string(),
                    message: format!(
                        "{} active threads with no activity for {}+ hours",
                        stale_count, self.config.stale_thread_hours
                    ),
                    action: "Use ai_thread_suspend to suspend inactive threads".to_string(),
                    metric_value: stale_count as f64,
                    threshold: self.config.stale_thread_count_threshold as f64,
                });
            }
        }

        // 6. Disk usage
        if let Ok(size) = get_db_size(ai_path) {
            if size >= self.config.disk_warning_bytes {
                findings.push(HealthFinding {
                    priority: HealthPriority::High,
                    category: "disk".to_string(),
                    message: format!("Database size: {} MB", size / 1_000_000),
                    action: "Use ai_backup and consider archiving old threads".to_string(),
                    metric_value: size as f64,
                    threshold: self.config.disk_warning_bytes as f64,
                });
            }
        }

        if findings.is_empty() {
            tracing::debug!("Health check: all clear");
            return None;
        }

        // Sort by priority (critical first)
        findings.sort_by(|a, b| b.priority.cmp(&a.priority));
        findings.truncate(self.config.max_suggestions);

        for f in &findings {
            match f.priority {
                HealthPriority::Critical => tracing::error!(category = %f.category, metric = f.metric_value, "{}", f.message),
                HealthPriority::High => tracing::warn!(category = %f.category, metric = f.metric_value, "{}", f.message),
                _ => tracing::info!(category = %f.category, metric = f.metric_value, "{}", f.message),
            }
        }

        tracing::info!(findings_count = findings.len(), "Health check complete");

        self.record_injection(ai_path);

        Some(findings)
    }

    /// Format findings for system-reminder injection.
    pub fn format_injection(findings: &[HealthFinding]) -> String {
        Self::format_injection_with_prompts(findings, &HealthGuardPrompts::default())
    }

    /// Format findings using custom prompts (if set).
    pub fn format_injection_with_prompts(
        findings: &[HealthFinding],
        prompts: &HealthGuardPrompts,
    ) -> String {
        let header = if prompts.header.is_empty() {
            "Memory health alerts:"
        } else {
            &prompts.header
        };
        let mut out = format!("{}\n", header);
        for f in findings {
            out.push_str(&format!(
                "- [{}] {}: {} -> {}\n",
                f.priority, f.category, f.message, f.action
            ));
        }
        out
    }

    /// Check if we're in cooldown period.
    fn in_cooldown(&self, ai_path: &Path) -> bool {
        let ts_file = ai_path.join("healthguard_last.txt");
        if let Ok(content) = std::fs::read_to_string(&ts_file) {
            if let Ok(ts) = content.trim().parse::<u64>() {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                return now - ts < self.config.cooldown_secs;
            }
        }
        false
    }

    /// Record injection timestamp.
    fn record_injection(&self, ai_path: &Path) {
        let ts_file = ai_path.join("healthguard_last.txt");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = std::fs::write(&ts_file, now.to_string());
    }
}

impl Default for HealthGuard {
    fn default() -> Self {
        Self::new(HealthGuardConfig::default())
    }
}

// -- Helper SQL queries --

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
    let active = ThreadStorage::list_active(conn)?;
    let now = chrono::Utc::now();
    let count = active
        .iter()
        .filter(|t| (now - t.last_active).num_hours() >= hours as i64)
        .count();
    Ok(count)
}

fn get_db_size(ai_path: &Path) -> std::io::Result<u64> {
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
