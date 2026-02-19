// === Resource Caps ===
pub const MAX_THREADS_PER_AGENT: usize = 10_000;
pub const MAX_COGNITIVE_INBOX_PENDING: usize = 1_000;
pub const MAX_MESSAGE_SIZE_BYTES: usize = 65_536; // 64 KB
pub const MAX_AGENTS_FREE: usize = 5;
pub const DB_SIZE_WARNING_BYTES: u64 = 100 * 1024 * 1024; // 100 MB

// === SQLite Tuning ===
pub const SQLITE_BUSY_TIMEOUT_MS: u32 = 5_000;
pub const HOOK_QUERY_TIMEOUT_MS: u64 = 100;

// === Thread Decay ===
pub const THREAD_SUSPEND_THRESHOLD: f64 = 0.1;
pub const THREAD_USE_BOOST: f64 = 0.1;
pub const THREAD_MIN_HALF_LIFE: f64 = 0.75;
pub const THREAD_MAX_HALF_LIFE: f64 = 7.0;

// === Bridge Decay ===
pub const BRIDGE_HALF_LIFE: f64 = 2.0;
pub const BRIDGE_DEATH_THRESHOLD: f64 = 0.05;
pub const BRIDGE_USE_BOOST: f64 = 0.1;

// === Gossip defaults (overridable via GossipConfig) ===
pub const GOSSIP_BATCH_SIZE: usize = 50;
pub const GOSSIP_YIELD_MS: u64 = 10;
pub const GOSSIP_SIMILARITY_THRESHOLD: f64 = 0.75; // ONNX default (aligned with Python)
pub const GOSSIP_TFIDF_THRESHOLD: f64 = 0.55;      // TF-IDF default (lower: different distribution)
pub const GOSSIP_STRONG_BRIDGE: f64 = 0.80;         // high similarity → extends relation
pub const GOSSIP_TOPIC_OVERLAP_MIN: usize = 2;      // min shared topics for topic overlap bridge
pub const GOSSIP_LABEL_OVERLAP_MIN: usize = 2;      // min shared labels for label overlap bridge

// === Retrieval ===
pub const RETRIEVAL_ACTIVE_MIN: f64 = 0.05;
pub const RETRIEVAL_SUSPENDED_MIN: f64 = 0.12;
pub const RETRIEVAL_ARCHIVED_MIN: f64 = 0.20;
pub const RETRIEVAL_FOCUS_BOOST_DEFAULT: f64 = 0.15;
pub const RETRIEVAL_STATUS_PENALTY_DEFAULT: f64 = 0.1;
pub const REACTIVATION_HIGH_CONFIDENCE: f64 = 0.35;

// === Hook ===
pub const MIN_PROMPT_LENGTH: usize = 50;
pub const MIN_CAPTURE_LENGTH: usize = 20;
pub const MAX_CONTEXT_SIZE: usize = 8000;
pub const MAX_COGNITIVE_MESSAGES: usize = 5;

// === Archiver ===
pub const ARCHIVE_AFTER_HOURS: i64 = 72;

// === Health Check ===
pub const HEALTH_CHECK_BUDGET_MS: u64 = 2;
pub const HEALTH_REPAIR_BUDGET_MS: u64 = 50;

// === HealthGuard defaults (user-overridable via config.json) ===
pub const HEALTHGUARD_COOLDOWN_SECS: u64 = 1800;    // 30 min between injections
pub const HEALTHGUARD_MAX_SUGGESTIONS: usize = 3;
pub const WARNING_THRESHOLD: usize = 50;             // thread count warning

// === Schema ===
pub const SCHEMA_VERSION: u32 = 1;
pub const PROTOCOL_VERSION: u32 = 1;

// === Daemon ===
pub const DAEMON_WAL_AUTOCHECKPOINT: u32 = 1_000;
pub const HOOK_WAL_AUTOCHECKPOINT: u32 = 0;
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
