// === Resource Caps ===
pub const MAX_THREADS_PER_AGENT: usize = 10_000;
pub const MAX_COGNITIVE_INBOX_PENDING: usize = 1_000;
pub const MAX_MESSAGE_SIZE_BYTES: usize = 65_536; // 64 KB
pub const MAX_AGENTS_FREE: usize = 5;
pub const DB_SIZE_WARNING_BYTES: u64 = 100 * 1024 * 1024; // 100 MB

// === SQLite Tuning ===
pub const SQLITE_BUSY_TIMEOUT_MS: u32 = 1_000;
pub const HOOK_QUERY_TIMEOUT_MS: u64 = 100;

// === Thread Decay ===
pub const THREAD_SUSPEND_THRESHOLD: f64 = 0.1;
pub const THREAD_USE_BOOST: f64 = 0.1;
pub const THREAD_MIN_HALF_LIFE: f64 = 0.75;
pub const THREAD_MAX_HALF_LIFE: f64 = 7.0;
pub const ORPHAN_HALVING_HOURS: f64 = 6.0;        // half-life halves every 6h without re-injection
pub const ORPHAN_MIN_HALF_LIFE_FACTOR: f64 = 0.1;  // floor: half-life can't go below 10% of base

// === Bridge Decay ===
pub const BRIDGE_DEATH_THRESHOLD: f64 = 0.05;
pub const BRIDGE_USE_BOOST: f64 = 0.1;
pub const BRIDGE_WEAK_THRESHOLD: f64 = 0.15;

// === Gossip v2 ===
pub const GOSSIP_MERGE_MAX_PER_CYCLE: usize = 3;    // max auto merges per gossip cycle
pub const GOSSIP_MERGE_REJECTION_PENALTY: f64 = 0.2; // confidence reduction on merge reject
pub const GOSSIP_OVERLAP_WEIGHT: f64 = 0.5;
pub const GOSSIP_RICHNESS_WEIGHT: f64 = 0.5;
pub const GOSSIP_RICHNESS_NORMALIZATION: f64 = 5.0;

// === Merge Evaluator ===
pub const MERGE_EVALUATOR_MAX_CHARS: usize = 30_000;
pub const MERGE_EVALUATOR_MAX_MESSAGES: usize = 5;
pub const MERGE_EVALUATOR_MSG_MAX_CHARS: usize = 500;

// === Retrieval ===
pub const RETRIEVAL_ACTIVE_MIN: f64 = 0.05;
pub const RETRIEVAL_SUSPENDED_MIN: f64 = 0.12;
pub const RETRIEVAL_ARCHIVED_MIN: f64 = 0.20;
pub const RETRIEVAL_FOCUS_BOOST_DEFAULT: f64 = 0.15;
pub const RETRIEVAL_STATUS_PENALTY_DEFAULT: f64 = 0.1;
pub const REACTIVATION_HIGH_CONFIDENCE: f64 = 0.35;

// === Hook ===
pub const MIN_PROMPT_LENGTH: usize = 50;
pub const PROMPT_RELEVANCE_GATE_MAX: usize = 150;
pub const MIN_CAPTURE_LENGTH: usize = 20;
pub const MAX_CONTEXT_SIZE: usize = 15_000;
pub const MAX_COGNITIVE_MESSAGES: usize = 5;

// === Archiver ===
pub const ARCHIVE_AFTER_HOURS: i64 = 72;

// === Health Check ===
pub const HEALTH_CHECK_BUDGET_MS: u64 = 2;
pub const HEALTH_REPAIR_BUDGET_MS: u64 = 50;

// === HealthGuard defaults (user-overridable via config.json) ===
pub const HEALTHGUARD_COOLDOWN_SECS: u64 = 1800;    // 30 min between injections
pub const HEALTHGUARD_MAX_SUGGESTIONS: usize = 3;

// === Schema ===
pub const SCHEMA_VERSION: u32 = 1;
pub const PROTOCOL_VERSION: u32 = 1;

// === Daemon ===
pub const DAEMON_WAL_AUTOCHECKPOINT: u32 = 1_000;
pub const HOOK_WAL_AUTOCHECKPOINT: u32 = 100;
pub const PRUNE_INTERVAL_SECS: u64 = 300; // 5 min — all periodic tasks

// === Connection Pool (global daemon) ===
pub const POOL_MAX_IDLE_SECS: u64 = 1800;           // 30 min before eviction
pub const POOL_MAX_CONNECTIONS: usize = 50;          // max simultaneous agent connections
pub const POOL_EVICTION_CHECK_SECS: u64 = 300;      // check for idle connections every 5 min

// === Attachments ===
pub const MAX_ATTACHMENT_SIZE_BYTES: usize = 32_768;     // 32 KB per file
pub const MAX_ATTACHMENTS_PER_MESSAGE: usize = 5;
pub const MAX_TOTAL_ATTACHMENT_BYTES: usize = 131_072;   // 128 KB total

// === Messaging ===
pub const DEFAULT_MESSAGE_TTL_MINUTES: u64 = 1_440;  // 24h
pub const HIGH_PRIORITY_TTL_MINUTES: u64 = 2_880;    // 48h
pub const URGENT_TTL_MINUTES: u64 = 4_320;           // 72h

// === Guardian Alert defaults ===
pub const ALERT_WARNING_AFTER: u32 = 3;
pub const ALERT_CRITICAL_AFTER: u32 = 5;
pub const ALERT_COOLDOWN_SECS: u64 = 300;            // 5 min per-system cooldown

// === UTF-8 Safe Truncation ===
/// Truncate a string to at most `max_bytes` bytes on a valid UTF-8 char boundary.
/// Floors to the nearest char boundary at or before `max_bytes`.
pub fn truncate_safe(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

// === Label Filtering ===
/// Labels that are too generic to carry semantic value — filtered out before storage.
pub const LABEL_BLOCKLIST: &[&str] = &[
    "action", "decision", "metadata", "empty", "search result",
    "no matches", "empty result", "file-listing", "directory-listing",
    "grep-output", "search-config", "build-output", "code-snippet",
];

/// Filter out blocked labels (case-insensitive match against LABEL_BLOCKLIST).
pub fn filter_blocked_labels(labels: &[String]) -> Vec<String> {
    labels
        .iter()
        .filter(|l| {
            !LABEL_BLOCKLIST
                .iter()
                .any(|blocked| l.to_lowercase() == *blocked)
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_safe_ascii() {
        assert_eq!(truncate_safe("hello world", 5), "hello");
    }

    #[test]
    fn test_truncate_safe_multibyte() {
        // 'e' with combining accent = 2 bytes for the accent
        let s = "caf\u{00e9}s"; // cafe with e-acute (2 bytes for e-acute)
        let result = truncate_safe(s, 4);
        // Should truncate on char boundary
        assert!(result.len() <= 4);
        assert!(result.is_char_boundary(result.len()));
    }

    #[test]
    fn test_truncate_safe_no_truncation() {
        assert_eq!(truncate_safe("short", 100), "short");
    }

    #[test]
    fn test_truncate_safe_empty() {
        assert_eq!(truncate_safe("", 10), "");
    }

    #[test]
    fn test_filter_blocked_labels_filters_known() {
        let labels: Vec<String> = vec!["action", "rust", "metadata", "testing"]
            .into_iter().map(String::from).collect();
        let result = filter_blocked_labels(&labels);
        assert_eq!(result, vec!["rust", "testing"]);
    }

    #[test]
    fn test_filter_blocked_labels_case_insensitive() {
        let labels: Vec<String> = vec!["Action", "METADATA", "Empty Result"]
            .into_iter().map(String::from).collect();
        let result = filter_blocked_labels(&labels);
        assert!(result.is_empty());
    }

    #[test]
    fn test_filter_blocked_labels_empty_input() {
        let labels: Vec<String> = vec![];
        let result = filter_blocked_labels(&labels);
        assert!(result.is_empty());
    }
}
