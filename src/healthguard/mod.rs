//! HealthGuard — proactive memory health monitoring and maintenance.
//!
//! Two modes of operation:
//!   - **Injection** (High/Critical): Injects actionable instructions into agent's stdin
//!     via system-reminder blocks. The agent treats these as orders and executes them.
//!     Used for merge candidates (0.60-0.85), capacity alerts, etc.
//!   - **Suggestion** (Low/Medium): Returns informational findings via ai_suggestions
//!     MCP tool. Agent may or may not act on these.
//!
//! Checks:
//!   1. Memory capacity (active threads / quota)
//!   2. Fragmentation (single-message threads %)
//!   3. Unlabeled threads (%)
//!   4. Weak bridges (count)
//!   5. Stale active threads (>N hours inactive)
//!   6. Disk usage (DB size)
//!   7. Merge candidates (gossip bridges with weight 0.60-0.85)

pub mod checks;
pub mod formatter;
pub mod merge_detector;

use crate::AiResult;
use rusqlite::Connection;
use std::path::Path;

// ── Types ──

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
    /// Max merge candidates to inject per cycle.
    #[serde(default = "default_max_merge_candidates")]
    pub max_merge_candidates: usize,
    /// Thread quota for capacity check. Overrides pool default (50).
    #[serde(default = "default_thread_quota")]
    pub thread_quota: usize,
    /// Custom injection prompts (editable from GUI).
    #[serde(default)]
    pub prompts: HealthGuardPrompts,
}

fn default_max_merge_candidates() -> usize { 3 }
fn default_thread_quota() -> usize { 50 }

/// Custom prompts for HealthGuard injection. Empty strings use defaults.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct HealthGuardPrompts {
    /// Header prepended to all findings. Default: "Memory maintenance required:"
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
            max_merge_candidates: default_max_merge_candidates(),
            thread_quota: default_thread_quota(),
            prompts: HealthGuardPrompts::default(),
        }
    }
}

// ── HealthGuard ──

pub struct HealthGuard {
    config: HealthGuardConfig,
}

impl HealthGuard {
    pub fn new(config: HealthGuardConfig) -> Self {
        Self { config }
    }

    /// Main health analysis. Returns None if disabled, in cooldown, or healthy.
    ///
    /// Runs all checks sequentially, sorts by priority (critical first),
    /// and truncates to max_suggestions.
    pub fn analyze(
        &self,
        conn: &Connection,
        ai_path: &Path,
        gossip_config: &crate::config::GossipConfig,
    ) -> Option<Vec<HealthFinding>> {
        if !self.config.enabled {
            return None;
        }

        if self.in_cooldown(ai_path) {
            return None;
        }

        let mut findings = Vec::new();

        // 1. Memory capacity
        if let Some(f) = checks::check_capacity(conn, &self.config, self.config.thread_quota) {
            findings.push(f);
        }

        // 2. Fragmentation
        if let Some(f) = checks::check_fragmentation(conn, &self.config) {
            findings.push(f);
        }

        // 3. Unlabeled threads
        if let Some(f) = checks::check_unlabeled(conn, &self.config) {
            findings.push(f);
        }

        // 4. Weak bridges
        if let Some(f) = checks::check_weak_bridges(conn, &self.config) {
            findings.push(f);
        }

        // 5. Stale active threads
        if let Some(f) = checks::check_stale_threads(conn, &self.config) {
            findings.push(f);
        }

        // 6. Disk usage
        if let Some(f) = checks::check_disk_usage(&self.config, ai_path) {
            findings.push(f);
        }

        // 7. Merge candidates (gossip bridges 0.60-0.85)
        if let Ok(candidates) = merge_detector::detect_merge_candidates(
            conn,
            gossip_config.merge_evaluation_threshold,
            gossip_config.merge_auto_threshold,
            self.config.max_merge_candidates,
        ) {
            if !candidates.is_empty() {
                tracing::info!(
                    count = candidates.len(),
                    "HealthGuard: merge candidates detected"
                );
                let merge_findings = merge_detector::merge_candidates_to_findings(&candidates);
                findings.extend(merge_findings);
            }
        }

        if findings.is_empty() {
            tracing::debug!("HealthGuard: all clear");
            return None;
        }

        // Sort by priority (critical first)
        findings.sort_by(|a, b| b.priority.cmp(&a.priority));
        findings.truncate(self.config.max_suggestions);

        for f in &findings {
            match f.priority {
                HealthPriority::Critical => tracing::error!(
                    category = %f.category, metric = f.metric_value, "{}", f.message
                ),
                HealthPriority::High => tracing::warn!(
                    category = %f.category, metric = f.metric_value, "{}", f.message
                ),
                _ => tracing::info!(
                    category = %f.category, metric = f.metric_value, "{}", f.message
                ),
            }
        }

        tracing::info!(findings_count = findings.len(), "HealthGuard analysis complete");

        self.record_injection(ai_path);

        Some(findings)
    }

    /// Partition findings into (injectable, suggestible).
    ///
    /// High/Critical → injection (imposed on agent via stdin)
    /// Low/Medium → suggestions (optional, via ai_suggestions)
    pub fn partition_findings(
        findings: &[HealthFinding],
    ) -> (Vec<&HealthFinding>, Vec<&HealthFinding>) {
        let mut inject = Vec::new();
        let mut suggest = Vec::new();

        for f in findings {
            match f.priority {
                HealthPriority::High | HealthPriority::Critical => inject.push(f),
                HealthPriority::Low | HealthPriority::Medium => suggest.push(f),
            }
        }

        (inject, suggest)
    }

    /// Partition findings into (high_critical, medium, low).
    pub fn partition_findings_by_priority(
        findings: &[HealthFinding],
    ) -> (Vec<&HealthFinding>, Vec<&HealthFinding>, Vec<&HealthFinding>) {
        let mut high_critical = Vec::new();
        let mut medium = Vec::new();
        let mut low = Vec::new();

        for f in findings {
            match f.priority {
                HealthPriority::High | HealthPriority::Critical => high_critical.push(f),
                HealthPriority::Medium => medium.push(f),
                HealthPriority::Low => low.push(f),
            }
        }

        (high_critical, medium, low)
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

    /// Record injection timestamp for cooldown tracking.
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
