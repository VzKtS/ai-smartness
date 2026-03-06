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
pub const GOSSIP_OVERLAP_WEIGHT: f64 = 0.5;
pub const GOSSIP_RICHNESS_WEIGHT: f64 = 0.5;
pub const GOSSIP_RICHNESS_NORMALIZATION: f64 = 5.0;

// === Content Limits ===
pub const CONTENT_LIMIT_DEFAULT: usize = 2_000;
pub const CONTENT_LIMIT_CONVERSATION: usize = 10_000;
pub const CONTENT_LIMIT_WEB: usize = 8_000;
pub const VERBATIM_SUMMARY_LIMIT: usize = 250;

// === Retrieval ===
pub const RETRIEVAL_ACTIVE_MIN: f64 = 0.05;
pub const RETRIEVAL_SUSPENDED_MIN: f64 = 0.12;
pub const RETRIEVAL_ARCHIVED_MIN: f64 = 0.20;
pub const RETRIEVAL_FOCUS_BOOST_DEFAULT: f64 = 0.15;
pub const RETRIEVAL_STATUS_PENALTY_DEFAULT: f64 = 0.1;
pub const REACTIVATION_HIGH_CONFIDENCE: f64 = 0.35;

// === Hook ===
/// Legacy default — now configurable via config.json `capture.min_prompt_length`.
pub const MIN_PROMPT_LENGTH: usize = 10;
pub const PROMPT_RELEVANCE_GATE_MAX: usize = 150;
pub const MIN_CAPTURE_LENGTH: usize = 10;
/// Legacy default — now configurable via config.json `capture.min_response_length`.
pub const MIN_RESPONSE_LENGTH: usize = 10;
pub const MAX_CONTEXT_SIZE: usize = 15_000;
/// Maximum characters for transcript capture in __mind__ savepoints.
pub const MAX_MIND_TRANSCRIPT_CHARS: usize = 8_000;
pub const MAX_COGNITIVE_MESSAGES: usize = 5;

// === Archiver ===
pub const ARCHIVE_AFTER_HOURS: i64 = 72;

// === Health Check ===
pub const HEALTH_CHECK_BUDGET_MS: u64 = 2;
pub const HEALTH_REPAIR_BUDGET_MS: u64 = 50;

// === Schema ===
pub const SCHEMA_VERSION: u32 = 1;
pub const PROTOCOL_VERSION: u32 = 1;

// === Daemon ===
pub const DAEMON_WAL_AUTOCHECKPOINT: u32 = 1_000;
pub const HOOK_WAL_AUTOCHECKPOINT: u32 = 100;
pub const PRUNE_INTERVAL_SECS: u64 = 300; // 5 min — all periodic tasks
pub const MAX_IPC_THREADS: usize = 32;    // max concurrent IPC handler threads

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

// === Concept Normalization ===
/// Stopwords filtered from concepts: Rust stdlib noise, generic English glue words.
pub const CONCEPT_STOPWORDS: &[&str] = &[
    // Rust stdlib / common crate noise
    "std", "self", "super", "crate", "pub", "use", "impl", "trait",
    "enum", "struct", "type", "where", "async", "await", "unsafe", "dyn",
    "box", "ref", "mut", "let", "const", "static", "move", "return",
    "panic", "unwrap", "expect", "clone", "copy", "drop", "send", "sync",
    "hashmap", "vec", "string", "option", "result", "arc", "mutex",
    "rwlock", "cell", "refcell", "atomicbool", "atomicu64", "atomicusize",
    "stdin", "stdout", "stderr", "println", "eprintln", "format",
    "serde", "tokio", "tracing", "instant",
    // Generic English glue (>= 3 chars)
    "the", "and", "for", "are", "but", "not", "you", "all", "can",
    "had", "her", "was", "one", "our", "out", "has", "his", "how",
    "its", "may", "new", "now", "old", "see", "way", "who", "did",
    "get", "got", "say", "she", "too", "with", "from",
    "that", "this", "will", "have", "been", "some", "than", "them",
    "then", "into", "only", "over", "such", "also", "more", "other",
    "about", "which", "their", "would", "could", "should", "these",
    "those", "being", "through",
];

/// Maximum concepts per thread after normalization.
pub const MAX_CONCEPTS_PER_THREAD: usize = 25;

/// Normalize concepts: split multi-word phrases into single words, lowercase,
/// deduplicate, filter stopwords, cap at MAX_CONCEPTS_PER_THREAD.
pub fn normalize_concepts(raw: &[String]) -> Vec<String> {
    let stopwords: std::collections::HashSet<&str> =
        CONCEPT_STOPWORDS.iter().copied().collect();
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();

    for phrase in raw {
        for token in phrase.split(|c: char| c.is_whitespace() || c == ':' || c == '_' || c == '-') {
            let word = token.trim().to_lowercase();
            if word.len() < 3 {
                continue;
            }
            if stopwords.contains(word.as_str()) {
                continue;
            }
            if seen.insert(word.clone()) {
                result.push(word);
            }
        }
    }

    result.truncate(MAX_CONCEPTS_PER_THREAD);
    result
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

    #[test]
    fn test_min_prompt_length_default() {
        assert_eq!(MIN_PROMPT_LENGTH, 10);
    }

    #[test]
    fn test_min_response_length_default() {
        assert_eq!(MIN_RESPONSE_LENGTH, 10);
    }

    #[test]
    fn test_normalize_concepts_splits_multiword() {
        let raw = vec![
            "machine learning".to_string(),
            "web development".to_string(),
        ];
        let result = normalize_concepts(&raw);
        assert_eq!(result, vec!["machine", "learning", "web", "development"]);
    }

    #[test]
    fn test_normalize_concepts_filters_stopwords() {
        let raw = vec![
            "std::panic::catch_unwind".to_string(),
            "hashmap".to_string(),
            "rust".to_string(),
        ];
        let result = normalize_concepts(&raw);
        assert_eq!(result, vec!["catch", "unwind", "rust"]);
    }

    #[test]
    fn test_normalize_concepts_deduplicates() {
        let raw = vec![
            "config management".to_string(),
            "configuration config".to_string(),
        ];
        let result = normalize_concepts(&raw);
        assert_eq!(result, vec!["config", "management", "configuration"]);
    }

    #[test]
    fn test_normalize_concepts_caps_at_max() {
        let raw: Vec<String> = (0..30).map(|i| format!("concept{}", i)).collect();
        let result = normalize_concepts(&raw);
        assert_eq!(result.len(), MAX_CONCEPTS_PER_THREAD);
    }

    #[test]
    fn test_normalize_concepts_filters_short() {
        let raw = vec!["a b of in to".to_string(), "rust".to_string()];
        let result = normalize_concepts(&raw);
        assert_eq!(result, vec!["rust"]);
    }

    #[test]
    fn test_normalize_concepts_empty() {
        let result = normalize_concepts(&[]);
        assert!(result.is_empty());
    }
}
