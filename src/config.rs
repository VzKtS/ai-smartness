//! Guardian & system configuration — all LLM tasks + embedding systems.
//!
//! Each subsystem is independently configurable:
//!   - LLM-based: extraction, coherence, reactivation, synthesis, labels, importance
//!   - Embedding-based: gossip, recall, thread matching
//!
//! Design: LLM-FIRST — prefer LLM (even haiku) over heuristic fallbacks.
//! Heuristics create "junk" threads with poor titles and imprecise topics.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

// ============================================================================
// CLAUDE MODEL
// ============================================================================

/// Supported Claude model tiers (version-agnostic).
/// The actual model ID is resolved at runtime by claude CLI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum ClaudeModel {
    #[default]
    Haiku,      // Fast, cheap — ideal for high-frequency tasks
    Sonnet,     // Balanced quality/cost — good for moderate tasks
    Opus,       // Maximum quality — for critical decisions
}

impl ClaudeModel {
    pub fn as_cli_flag(&self) -> &str {
        match self {
            Self::Haiku => "haiku",
            Self::Sonnet => "sonnet",
            Self::Opus => "opus",
        }
    }
}


fn parse_model(s: &str) -> ClaudeModel {
    match s.to_lowercase().as_str() {
        "haiku" => ClaudeModel::Haiku,
        "sonnet" => ClaudeModel::Sonnet,
        "opus" => ClaudeModel::Opus,
        _ => ClaudeModel::Haiku,
    }
}

// ============================================================================
// PER-TASK LLM CONFIGURATION
// ============================================================================

/// Configuration for a single LLM task.
/// Each of the 6 Guardian tasks has its own instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskLlmConfig {
    pub model: ClaudeModel,
    pub timeout_secs: u64,
    pub max_retries: u32,       // 0 = no retry, 1 = one retry on failure
    pub enabled: bool,          // false = skip this task completely
    /// Behavior when LLM fails (timeout, CLI absent, network down).
    /// LLM-FIRST: always prefer an LLM (even haiku) over a heuristic.
    pub failure_mode: LlmFailureMode,
}

/// Behavior on LLM failure.
/// Design: LLM-first — even haiku produces better results than regex.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum LlmFailureMode {
    /// Default: retry with lightest model (haiku), then skip.
    #[default]
    RetryWithHaiku,
    /// Skip silently. No thread created.
    Skip,
    /// Legacy: use FallbackPatterns regex.
    /// NOT RECOMMENDED — produces threads with poor titles and imprecise topics.
    HeuristicRegex,
}

// ============================================================================
// EMBEDDING SYSTEM CONFIG (for non-LLM systems: gossip, recall, matching)
// ============================================================================

/// Embedding mode for vector-similarity-based systems.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum EmbeddingMode {
    /// ONNX Runtime (best). If unavailable → automatic TF-IDF fallback.
    #[default]
    OnnxWithFallback,
    /// ONNX only. If unavailable → skip (no fallback).
    OnnxOnly,
    /// TF-IDF only. Never calls ONNX (ultra-light, offline).
    TfidfOnly,
    /// Disable this system entirely.
    Disabled,
}

fn parse_embedding_mode(s: &str) -> EmbeddingMode {
    match s {
        "OnnxWithFallback" => EmbeddingMode::OnnxWithFallback,
        "OnnxOnly" => EmbeddingMode::OnnxOnly,
        "TfidfOnly" => EmbeddingMode::TfidfOnly,
        "Disabled" => EmbeddingMode::Disabled,
        _ => EmbeddingMode::OnnxWithFallback,
    }
}

/// Configuration for embedding-based subsystems.
/// Each system (gossip, recall, thread matching, reactivation) has its own instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingSystemConfig {
    pub mode: EmbeddingMode,
    /// Similarity threshold for ONNX (calibrated on all-MiniLM-L6-v2).
    pub onnx_threshold: f64,
    /// Similarity threshold for TF-IDF (typically lower — different distribution).
    pub tfidf_threshold: f64,
}

impl EmbeddingSystemConfig {
    /// Returns the active threshold based on current embedding backend.
    pub fn active_threshold(&self, use_onnx: bool) -> f64 {
        if use_onnx { self.onnx_threshold } else { self.tfidf_threshold }
    }
}

/// Parse an "embedding" section from config.json into EmbeddingSystemConfig.
fn parse_embedding_config(obj: &serde_json::Map<String, serde_json::Value>, cfg: &mut EmbeddingSystemConfig) {
    if let Some(m) = obj.get("mode").and_then(|v| v.as_str()) {
        cfg.mode = parse_embedding_mode(m);
    }
    if let Some(v) = obj.get("onnx_threshold").and_then(|v| v.as_f64()) {
        cfg.onnx_threshold = v;
    }
    if let Some(v) = obj.get("tfidf_threshold").and_then(|v| v.as_f64()) {
        cfg.tfidf_threshold = v;
    }
}

// ============================================================================
// GUARDIAN ALERT SYSTEM (real-time notification with deep-link)
// ============================================================================

/// Alert emitted when a system fails or is in degraded mode.
/// Injected into next prompt AND displayed in admin panel.
///
/// Flow:
/// 1. Guardian record_call(task, success=false) → increment failure count
/// 2. If failures >= warning_threshold → write GuardianAlert to guardian_alerts.json
/// 3. Inject hook: read guardian_alerts.json, inject warning with config_path
/// 4. Admin panel: poll guardian_alerts.json, display toast notification
/// 5. VSCode extension: receive IPC event, display notification with "Configure" button
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardianAlert {
    pub id: String,
    /// Failing system: "extraction", "coherence", "gossip", "recall",
    /// "reactivation", "thread_matching", "embeddings".
    pub system: String,
    pub level: AlertLevel,
    pub message: String,
    /// Config path for admin panel deep-link.
    /// Format: "guardian.{system}.{field}"
    pub config_path: String,
    pub consecutive_failures: u32,
    pub timestamp: String,
    pub recommended_action: String,
    pub acknowledged: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AlertLevel {
    /// 3+ failures — non-blocking informative notification.
    Warning,
    /// 5+ failures — urgent notification with admin panel deep-link.
    Critical,
    /// System in fallback mode — persistent informative notification.
    Degraded,
}

/// Alert trigger thresholds. Configurable in config.json "guardian.alerts".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertThresholds {
    pub warning_after: u32,              // default: 3
    pub critical_after: u32,             // default: 5
    pub per_system_cooldown_secs: u64,   // default: 300 (5 min)
}

impl Default for AlertThresholds {
    fn default() -> Self {
        Self {
            warning_after: 3,
            critical_after: 5,
            per_system_cooldown_secs: 300,
        }
    }
}

/// Guardian alert manager.
/// Reads/writes guardian_alerts.json in ai_path.
pub struct GuardianAlertManager;

impl GuardianAlertManager {
    /// Record a failure for a system. Emits alert if threshold reached.
    pub fn record_failure(
        _ai_path: &Path,
        _system: &str,
        _thresholds: &AlertThresholds,
    ) -> Option<GuardianAlert> {
        todo!()
    }

    /// Record a success — reset failure counter for this system.
    pub fn record_success(_ai_path: &Path, _system: &str) {
        todo!()
    }

    /// Get all unacknowledged alerts.
    pub fn get_active_alerts(_ai_path: &Path) -> Vec<GuardianAlert> {
        todo!()
    }

    /// Acknowledge (dismiss) an alert.
    pub fn acknowledge(_ai_path: &Path, _alert_id: &str) {
        todo!()
    }

    /// Format alerts for prompt injection.
    pub fn format_for_injection(alerts: &[GuardianAlert]) -> String {
        let mut lines = Vec::new();
        for alert in alerts {
            let icon = match alert.level {
                AlertLevel::Warning => "WARNING",
                AlertLevel::Critical => "CRITICAL",
                AlertLevel::Degraded => "DEGRADED",
            };
            lines.push(format!(
                "[{}] Guardian {}: {} ({}x failures). Configure: {}",
                icon, alert.system, alert.message,
                alert.consecutive_failures, alert.config_path
            ));
            lines.push(format!("  Action: {}", alert.recommended_action));
        }
        lines.join("\n")
    }
}

// ============================================================================
// EXTRACTION CONFIG
// ============================================================================

/// Extraction-specific configuration.
/// Controls how the Guardian extracts title/topics/summary from tool outputs.
///
/// 7 source-specific prompt templates:
///   prompt, read, write, task, fetch, response, command
///
/// Frequency: HIGH — called on every tool capture.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionConfig {
    pub llm: TaskLlmConfig,
    /// Max content chars sent to LLM (truncation).
    pub max_content_chars: usize,           // default: 15000
    /// Minimum content length to attempt capture (below = noise).
    pub min_capture_length: usize,          // default: 80
    /// Noise words filtered from extracted topics.
    pub topic_noise_words: Vec<String>,
    /// Custom topic aliases — maps extracted terms to canonical topics.
    pub topic_aliases: HashMap<String, String>,
    /// Minimum topic relevance.
    pub min_topic_frequency: usize,         // default: 1
    /// Skip extraction for these tool names.
    pub skip_tools: Vec<String>,
    /// Enable skip signal detection.
    pub enable_skip_signal: bool,           // default: true
    /// TTL for pending context in the daemon processor (seconds).
    #[serde(default = "default_pending_context_ttl")]
    pub pending_context_ttl_secs: u64,      // default: 600
}

impl Default for ExtractionConfig {
    fn default() -> Self {
        Self {
            llm: TaskLlmConfig {
                model: ClaudeModel::Haiku,
                timeout_secs: 30,
                max_retries: 1,
                enabled: true,
                failure_mode: LlmFailureMode::RetryWithHaiku,
            },
            max_content_chars: 15000,
            min_capture_length: 80,
            topic_noise_words: vec![
                "message", "contenu", "analyse", "fichier",
                "response", "result", "data", "type", "value",
            ].into_iter().map(String::from).collect(),
            topic_aliases: HashMap::new(),
            min_topic_frequency: 1,
            skip_tools: vec![],
            enable_skip_signal: true,
            pending_context_ttl_secs: default_pending_context_ttl(),
        }
    }
}

// ============================================================================
// COHERENCE CONFIG
// ============================================================================

/// Coherence-specific configuration.
/// Controls how the Guardian scores thematic coherence between consecutive captures.
///
/// Decision thresholds: >0.6=child, 0.4-0.6=orphan, <0.4=forget
///
/// Frequency: HIGH — called on every capture with pending context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoherenceConfig {
    pub llm: TaskLlmConfig,
    pub max_context_chars: usize,            // default: 1500
    /// Threshold for child decision (content related to parent).
    pub child_threshold: f64,                // default: 0.6
    /// Threshold for orphan decision (unrelated but substantial).
    pub orphan_threshold: f64,               // default: 0.4
    /// Fallback score returned on LLM error.
    pub fallback_score: f64,                 // default: 0.5
}

impl Default for CoherenceConfig {
    fn default() -> Self {
        Self {
            llm: TaskLlmConfig {
                model: ClaudeModel::Haiku,
                timeout_secs: 15,
                max_retries: 0,
                enabled: true,
                failure_mode: LlmFailureMode::RetryWithHaiku,
            },
            max_context_chars: 1500,
            child_threshold: 0.6,
            orphan_threshold: 0.4,
            fallback_score: 0.5,
        }
    }
}

// ============================================================================
// REACTIVATION CONFIG
// ============================================================================

/// Reactivation-specific configuration.
/// Controls LLM-assisted decision for borderline thread reactivation.
///
/// Three-tier: auto (>high), LLM (borderline-high), skip (<borderline)
///
/// Frequency: LOW — only for borderline similarity cases.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReactivationConfig {
    pub llm: TaskLlmConfig,
    pub auto_threshold: f64,                 // default: 0.35
    pub borderline_threshold: f64,           // default: 0.15
    pub max_context_chars: usize,            // default: 500
    pub max_topics: usize,                   // default: 5
    pub max_summary_chars: usize,            // default: 200
}

impl Default for ReactivationConfig {
    fn default() -> Self {
        Self {
            llm: TaskLlmConfig {
                model: ClaudeModel::Haiku,
                timeout_secs: 30,
                max_retries: 0,
                enabled: true,
                failure_mode: LlmFailureMode::RetryWithHaiku,
            },
            auto_threshold: 0.35,
            borderline_threshold: 0.15,
            max_context_chars: 500,
            max_topics: 5,
            max_summary_chars: 200,
        }
    }
}

// ============================================================================
// SYNTHESIS CONFIG
// ============================================================================

/// Synthesis-specific configuration.
/// Controls thread summarization at two points:
///   1. Session synthesis (at 95% context capacity)
///   2. Archive synthesis (when suspended threads are archived after 72h)
///
/// Frequency: MEDIUM — per-session or per-archive event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesisConfig {
    pub llm: TaskLlmConfig,
    pub max_messages: usize,                 // default: 10
    pub max_message_chars: usize,            // default: 500
    pub max_output_chars: usize,             // default: 1000
    pub language: String,                    // default: "en"
}

impl Default for SynthesisConfig {
    fn default() -> Self {
        Self {
            llm: TaskLlmConfig {
                model: ClaudeModel::Haiku,
                timeout_secs: 60,
                max_retries: 0,
                enabled: true,
                failure_mode: LlmFailureMode::RetryWithHaiku,
            },
            max_messages: 10,
            max_message_chars: 500,
            max_output_chars: 1000,
            language: "en".to_string(),
        }
    }
}

// ============================================================================
// LABEL SUGGESTION CONFIG
// ============================================================================

/// Label suggestion configuration.
/// LLM suggests labels for unlabeled threads.
///
/// Frequency: LOW — triggered by HealthGuard or on-demand via MCP tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelSuggestionConfig {
    pub llm: TaskLlmConfig,
    pub auto_suggest_on_extraction: bool,    // default: true
    pub label_vocabulary: Vec<String>,
    /// NOTE: Runtime filtering uses constants::LABEL_BLOCKLIST. This field is for GUI/config overrides.
    pub label_blocklist: Vec<String>,
    pub allow_custom_labels: bool,           // default: true
    pub batch_size: usize,                   // default: 10
}

impl Default for LabelSuggestionConfig {
    fn default() -> Self {
        Self {
            llm: TaskLlmConfig {
                model: ClaudeModel::Haiku,
                timeout_secs: 30,
                max_retries: 0,
                enabled: true,
                failure_mode: LlmFailureMode::RetryWithHaiku,
            },
            auto_suggest_on_extraction: true,
            label_vocabulary: vec![
                "bug-fix", "feature", "refactor", "architecture",
                "configuration", "database", "api", "cli",
                "hook-system", "agent-system", "documentation",
                "performance", "security", "testing",
            ].into_iter().map(String::from).collect(),
            label_blocklist: vec![
                "action", "decision", "metadata", "empty", "search result",
                "no matches", "empty result", "file-listing", "directory-listing",
                "grep-output", "search-config", "build-output", "code-snippet",
            ].into_iter().map(String::from).collect(),
            allow_custom_labels: true,
            batch_size: 10,
        }
    }
}

// ============================================================================
// IMPORTANCE RATING CONFIG
// ============================================================================

/// Importance auto-rating configuration.
/// LLM assigns importance score (0.0-1.0) to new threads.
///
/// Frequency: HIGH — piggybacks on extraction call (zero extra LLM cost).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportanceRatingConfig {
    pub llm: TaskLlmConfig,
    pub piggyback_on_extraction: bool,       // default: true
    pub fallback_score: f64,                 // default: 0.5
    pub score_map: ImportanceScoreMap,
}

/// Maps LLM-returned categories to numeric importance scores.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportanceScoreMap {
    pub critical: f64,    // "decisions", "blockers", "architecture" → 1.0
    pub high: f64,        // "implementation", "bug-fix", "config" → 0.8
    pub normal: f64,      // "exploration", "question", "learning" → 0.5
    pub low: f64,         // "chit-chat", "noise", "meta" → 0.3
    pub disposable: f64,  // "one-off debug", "transient log" → 0.1
}

impl Default for ImportanceScoreMap {
    fn default() -> Self {
        Self { critical: 1.0, high: 0.8, normal: 0.5, low: 0.3, disposable: 0.1 }
    }
}

impl Default for ImportanceRatingConfig {
    fn default() -> Self {
        Self {
            llm: TaskLlmConfig {
                model: ClaudeModel::Haiku,
                timeout_secs: 15,
                max_retries: 0,
                enabled: true,
                failure_mode: LlmFailureMode::RetryWithHaiku,
            },
            piggyback_on_extraction: true,
            fallback_score: 0.5,
            score_map: ImportanceScoreMap::default(),
        }
    }
}

// ============================================================================
// THREAD MATCHING MODE
// ============================================================================

/// Decision mode for thread matching (continue vs new thread).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum ThreadMatchingMode {
    /// Default: cosine similarity ONNX only.
    /// Fast (~1ms), zero LLM cost, ~80% precision.
    #[default]
    EmbeddingOnly,
    /// Embedding pre-filter + final LLM decision.
    /// More precise (~95%) but 1 extra LLM call per capture.
    EmbeddingPlusLlm,
}

// ============================================================================
// GOSSIP CONFIG (embedding-based bridge discovery)
// ============================================================================

/// Gossip v2 configuration — concept-based bridge discovery.
///
/// Pipeline:
///   1. ConceptIndex inverted index → find overlaps
///   2. Weight scoring → bridge creation
///   3. Merge candidate collection (score >= merge_evaluation_threshold)
///   4. Legacy topic overlap fallback for threads without concepts
///
/// Frequency: LOW — called in prune loop every 5 min.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipConfig {
    pub embedding: EmbeddingSystemConfig,
    // Concept gossip v2
    #[serde(default = "default_concept_overlap_min_shared")]
    pub concept_overlap_min_shared: usize,       // default: 3
    #[serde(default = "default_concept_min_bridge_weight")]
    pub concept_min_bridge_weight: f64,          // default: 0.20
    #[serde(default = "default_merge_evaluation_threshold")]
    pub merge_evaluation_threshold: f64,         // default: 0.60
    #[serde(default = "default_merge_auto_threshold")]
    pub merge_auto_threshold: f64,               // default: 0.85
    #[serde(default = "default_true")]
    pub concept_gossip_enabled: bool,            // default: true
    // Legacy (fallback for threads without concepts)
    pub topic_overlap_enabled: bool,             // default: true
    pub topic_overlap_min_shared: usize,         // default: 2
    /// Min bridges per thread (clamp floor for dynamic_limits).
    pub min_bridges_per_thread: usize,           // default: 3
    /// Max bridges per thread (clamp ceiling for dynamic_limits).
    pub max_bridges_per_thread: usize,           // default: 10
    /// Target ratio bridges/threads for dynamic limit calculation.
    pub target_bridge_ratio: f64,                // default: 3.0
    // Propagation (transitive bridge discovery via gossip Phase 3)
    /// Enable transitive propagation in gossip.
    #[serde(default = "default_true")]
    pub propagation_enabled: bool,               // default: true
    /// Max propagation depth (A→B→C = depth 1).
    #[serde(default = "default_propagation_max_depth")]
    pub propagation_max_depth: u32,              // default: 2
    /// Weight decay factor per propagation hop.
    #[serde(default = "default_propagation_decay_factor")]
    pub propagation_decay_factor: f64,           // default: 0.5
    /// Minimum weight for propagated bridges.
    #[serde(default = "default_propagation_min_weight")]
    pub propagation_min_weight: f64,             // default: 0.10
}

fn default_concept_overlap_min_shared() -> usize { 2 }
fn default_concept_min_bridge_weight() -> f64 { 0.20 }
fn default_merge_evaluation_threshold() -> f64 { 0.60 }
fn default_merge_auto_threshold() -> f64 { 0.85 }
fn default_propagation_max_depth() -> u32 { 2 }
fn default_propagation_decay_factor() -> f64 { 0.5 }
fn default_propagation_min_weight() -> f64 { 0.10 }

impl Default for GossipConfig {
    fn default() -> Self {
        Self {
            embedding: EmbeddingSystemConfig {
                mode: EmbeddingMode::OnnxWithFallback,
                onnx_threshold: 0.75,
                tfidf_threshold: 0.55,
            },
            concept_overlap_min_shared: 2,
            concept_min_bridge_weight: 0.20,
            merge_evaluation_threshold: 0.60,
            merge_auto_threshold: 0.85,
            concept_gossip_enabled: true,
            topic_overlap_enabled: true,
            topic_overlap_min_shared: 2,
            min_bridges_per_thread: 3,
            max_bridges_per_thread: 10,
            target_bridge_ratio: 3.0,
            propagation_enabled: true,
            propagation_max_depth: 2,
            propagation_decay_factor: 0.5,
            propagation_min_weight: 0.10,
        }
    }
}

// ============================================================================
// ENGRAM CONFIG (multi-validator retrieval — inspired by DeepSeek Engram)
// ============================================================================

/// Engram retrieval configuration — 8-validator consensus for memory injection.
///
/// Replaces single-signal cosine scoring with multi-validator voting:
///   Phase 1: TopicIndex hash lookup O(1) → candidate pre-filter
///   Phase 2: 8 validators vote (pass/fail + confidence)
///   Phase 3: Consensus → StrongInject (≥5/8) / WeakInject (3-4/8) / Skip (<3/8)
///
/// 7/8 validators are zero-cost (memory lookup). Only V1 (cosine) costs compute.
///
/// Frequency: HIGH — called on every user prompt (replaces RecallConfig pipeline).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngramConfig {
    /// Embedding config for V1 (SemanticSimilarity validator).
    pub embedding: EmbeddingSystemConfig,

    /// Per-validator weights (0.0=disabled, 1.0=full weight).
    /// Users can tune each validator independently in admin panel.
    pub validator_weights: ValidatorWeights,

    /// Consensus thresholds.
    pub strong_inject_min_votes: u8,             // default: 5 (out of 9)
    pub weak_inject_min_votes: u8,               // default: 3

    /// Max threads returned for injection.
    pub max_results: usize,                      // default: 5
    /// Max candidates scanned from hash index pre-filter.
    pub max_candidates: usize,                   // default: 50
    /// Max archived threads scanned.
    pub max_archived_scan: usize,                // default: 50

    /// Enable hash index pre-filter (TopicIndex).
    /// When disabled, falls back to full scan (legacy behavior).
    pub hash_index_enabled: bool,                // default: true
}

/// Per-validator weight configuration.
/// Each weight controls how much influence the validator has on the final score.
/// Set to 0.0 to disable a validator entirely.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorWeights {
    pub semantic_similarity: f64,    // V1 — cosine ONNX/TF-IDF (default: 1.0)
    pub topic_overlap: f64,          // V2 — shared topics (default: 0.8)
    pub temporal_proximity: f64,     // V3 — WorkContext freshness (default: 0.7)
    pub graph_connectivity: f64,     // V4 — bridge connectivity (default: 0.9)
    pub injection_history: f64,      // V5 — usage_ratio feedback (default: 0.6)
    pub decayed_relevance: f64,      // V6 — weight × importance (default: 0.5)
    pub label_coherence: f64,        // V7 — label matching (default: 0.4)
    pub focus_alignment: f64,        // V8 — ai_focus boost (default: 0.8)
    #[serde(default = "default_concept_coherence_weight")]
    pub concept_coherence: f64,      // V9 — concept overlap (default: 0.7)
}

fn default_concept_coherence_weight() -> f64 { 0.7 }

impl ValidatorWeights {
    /// Convert to Vec for indexed access by validator.
    pub fn to_vec(&self) -> Vec<f64> {
        vec![
            self.semantic_similarity,
            self.topic_overlap,
            self.temporal_proximity,
            self.graph_connectivity,
            self.injection_history,
            self.decayed_relevance,
            self.label_coherence,
            self.focus_alignment,
            self.concept_coherence,
        ]
    }
}

impl Default for ValidatorWeights {
    fn default() -> Self {
        Self {
            semantic_similarity: 1.0,
            topic_overlap: 0.8,
            temporal_proximity: 0.7,
            graph_connectivity: 0.9,
            injection_history: 0.6,
            decayed_relevance: 0.5,
            label_coherence: 0.4,
            focus_alignment: 0.8,
            concept_coherence: 0.7,
        }
    }
}

impl Default for EngramConfig {
    fn default() -> Self {
        Self {
            embedding: EmbeddingSystemConfig {
                mode: EmbeddingMode::OnnxWithFallback,
                onnx_threshold: 0.30,
                tfidf_threshold: 0.20,
            },
            validator_weights: ValidatorWeights::default(),
            strong_inject_min_votes: 5,
            weak_inject_min_votes: 3,
            max_results: 5,
            max_candidates: 50,
            max_archived_scan: 50,
            hash_index_enabled: true,
        }
    }
}

// ============================================================================
// RECALL CONFIG (legacy — kept for backward compat, delegates to EngramConfig)
// ============================================================================

/// Legacy recall config. New code should use EngramConfig.
/// Kept for config.json backward compatibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallConfig {
    pub embedding: EmbeddingSystemConfig,
    pub max_results: usize,
    pub max_candidates: usize,
    pub focus_boost: f64,
    pub status_penalty: f64,
}

impl Default for RecallConfig {
    fn default() -> Self {
        Self {
            embedding: EmbeddingSystemConfig {
                mode: EmbeddingMode::OnnxWithFallback,
                onnx_threshold: 0.30,
                tfidf_threshold: 0.20,
            },
            max_results: 5,
            max_candidates: 50,
            focus_boost: 0.15,
            status_penalty: 0.1,
        }
    }
}

// ============================================================================
// THREAD MATCHING CONFIG
// ============================================================================

/// Thread matching configuration — "continue" or "new thread" decision.
///
/// Frequency: HIGH — called on every capture.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadMatchingConfig {
    pub mode: ThreadMatchingMode,
    pub embedding: EmbeddingSystemConfig,
    #[serde(default = "default_continue_threshold")]
    pub continue_threshold: f64,
    #[serde(default = "default_reactivate_threshold")]
    pub reactivate_threshold: f64,
    #[serde(default = "default_capacity_suspend_threshold")]
    pub capacity_suspend_threshold: f64,
}

fn default_pending_context_ttl() -> u64 { 600 }

fn default_continue_threshold() -> f64 { 0.25 }
fn default_reactivate_threshold() -> f64 { 0.50 }
fn default_capacity_suspend_threshold() -> f64 { 0.85 }

impl Default for ThreadMatchingConfig {
    fn default() -> Self {
        Self {
            mode: ThreadMatchingMode::EmbeddingOnly,
            embedding: EmbeddingSystemConfig {
                mode: EmbeddingMode::OnnxWithFallback,
                onnx_threshold: 0.60,
                tfidf_threshold: 0.45,
            },
            continue_threshold: default_continue_threshold(),
            reactivate_threshold: default_reactivate_threshold(),
            capacity_suspend_threshold: default_capacity_suspend_threshold(),
        }
    }
}

// ============================================================================
// GUARDCODE CONFIG (content validation rules)
// ============================================================================

/// Action taken when content is blocked by a GuardCode rule.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum BlockAction {
    /// Reject content entirely (not stored).
    #[default]
    Reject,
    /// Store content but log a warning.
    WarnOnly,
    /// Truncate content to max_content_bytes.
    Truncate,
    /// Send to LLM for sanitization, then re-validate through GuardCode.
    /// Flow: blocked content → LLM sanitize → re-check rules → accept or reject.
    SanitizeLlm,
}

/// GuardCode configuration — content validation rules.
/// Controls MaxLengthRule and BlockedPatternRule enforcement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardCodeConfig {
    /// Enable GuardCode content validation.
    pub enabled: bool,
    /// Maximum content size in bytes (MaxLengthRule).
    pub max_content_bytes: usize,
    /// Blocked substring patterns (BlockedPatternRule).
    pub blocked_patterns: Vec<String>,
    /// Log warning when content is blocked.
    pub warn_on_block: bool,
    /// Action to take when content is blocked.
    pub action_on_block: BlockAction,
    /// LLM config for SanitizeLlm action.
    /// Used when action_on_block = SanitizeLlm.
    pub sanitize_llm: TaskLlmConfig,
    /// Max retries for the sanitize → re-validate loop (prevents infinite loops).
    pub sanitize_max_retries: u32,
    /// Custom messages (editable from GUI). Empty strings use defaults.
    #[serde(default)]
    pub messages: GuardCodeMessages,
}

/// Custom messages for GuardCode. Empty strings use defaults.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct GuardCodeMessages {
    /// General reject message. Default: "Content blocked by validation rules."
    #[serde(default)]
    pub reject: String,
    /// Max length warning. Placeholders: {size}, {max}. Default: "Content exceeds max length: {size} > {max}"
    #[serde(default)]
    pub max_length: String,
    /// Pattern match warning. Placeholder: {pattern}. Default: "Blocked pattern found: {pattern}"
    #[serde(default)]
    pub pattern_match: String,
}

impl Default for GuardCodeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_content_bytes: 50_000,
            blocked_patterns: vec![],
            warn_on_block: true,
            action_on_block: BlockAction::Reject,
            sanitize_llm: TaskLlmConfig {
                model: ClaudeModel::Haiku,
                timeout_secs: 30,
                max_retries: 1,
                enabled: true,
                failure_mode: LlmFailureMode::Skip,
            },
            sanitize_max_retries: 2,
            messages: GuardCodeMessages::default(),
        }
    }
}

// ============================================================================
// FALLBACK PATTERNS (legacy heuristic regex)
// ============================================================================

/// User-editable regex patterns for heuristic fallback.
/// Used when LLM subprocess is unavailable.
/// NOT RECOMMENDED — results are lower quality.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallbackPatterns {
    pub title_pattern: String,
    pub topic_patterns: Vec<String>,
    pub noise_patterns: Vec<String>,
    /// Keyword overlap ratio threshold for coherence fallback.
    pub coherence_keyword_threshold: f64,    // default: 0.3
}

impl Default for FallbackPatterns {
    fn default() -> Self {
        Self {
            title_pattern: r"^[#\s]*(.{10,80})[.\n]".to_string(),
            topic_patterns: vec![
                r"\b([A-Z][a-z]+(?:[A-Z][a-z]+)+)\b".to_string(),  // CamelCase
                r"#(\w+)".to_string(),                               // Hashtags
                r"`([a-z_]+)`".to_string(),                          // Code identifiers
            ],
            noise_patterns: vec![
                r"^\s*\{".to_string(),                // JSON blob
                r"Traceback \(most recent".to_string(), // Python stacktrace
                r"at\s+\S+:\d+:\d+".to_string(),       // JS stacktrace
            ],
            coherence_keyword_threshold: 0.3,
        }
    }
}

// ============================================================================
// DECAY & LIFECYCLE CONFIG
// ============================================================================

/// Decay & lifecycle parameters — configurable via GUI.
/// Controls thread weight decay, orphan acceleration, bridge decay, and archival.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecayConfig {
    /// Thread weight below this triggers auto-suspension. Default: 0.1
    #[serde(default = "default_thread_suspend_threshold")]
    pub thread_suspend_threshold: f64,
    /// Minimum half-life in days (importance=0, disposable). Default: 0.75
    #[serde(default = "default_thread_min_half_life")]
    pub thread_min_half_life: f64,
    /// Maximum half-life in days (importance=1, critical). Default: 7.0
    #[serde(default = "default_thread_max_half_life")]
    pub thread_max_half_life: f64,
    /// Weight boost on re-injection by Engram. Default: 0.1
    #[serde(default = "default_thread_use_boost")]
    pub thread_use_boost: f64,
    /// Hours without injection before orphan half-life halves. Default: 6.0
    #[serde(default = "default_orphan_halving_hours")]
    pub orphan_halving_hours: f64,
    /// Floor for orphan half-life (fraction of base). Default: 0.1
    #[serde(default = "default_orphan_min_half_life_factor")]
    pub orphan_min_half_life_factor: f64,
    /// Bridge half-life in days. Default: 2.0
    #[serde(default = "default_bridge_half_life")]
    pub bridge_half_life: f64,
    /// Bridge weight below this is marked invalid. Default: 0.05
    #[serde(default = "default_bridge_death_threshold")]
    pub bridge_death_threshold: f64,
    /// Bridge weight boost on traversal during recall. Default: 0.1
    #[serde(default = "default_bridge_use_boost")]
    pub bridge_use_boost: f64,
    /// Hours after suspension before archival. Default: 72.0
    #[serde(default = "default_archive_after_hours")]
    pub archive_after_hours: f64,
}

fn default_thread_suspend_threshold() -> f64 { 0.1 }
fn default_thread_min_half_life() -> f64 { 0.75 }
fn default_thread_max_half_life() -> f64 { 7.0 }
fn default_thread_use_boost() -> f64 { 0.1 }
fn default_orphan_halving_hours() -> f64 { 6.0 }
fn default_orphan_min_half_life_factor() -> f64 { 0.1 }
fn default_bridge_half_life() -> f64 { 4.0 }
fn default_bridge_death_threshold() -> f64 { 0.05 }
fn default_bridge_use_boost() -> f64 { 0.1 }
fn default_archive_after_hours() -> f64 { 72.0 }

impl Default for DecayConfig {
    fn default() -> Self {
        Self {
            thread_suspend_threshold: 0.1,
            thread_min_half_life: 0.75,
            thread_max_half_life: 7.0,
            thread_use_boost: 0.1,
            orphan_halving_hours: 6.0,
            orphan_min_half_life_factor: 0.1,
            bridge_half_life: 4.0,
            bridge_death_threshold: 0.05,
            bridge_use_boost: 0.1,
            archive_after_hours: 72.0,
        }
    }
}

// ============================================================================
// LLM HEALTH STATE
// ============================================================================

/// LLM health state — tracked by Guardian for HealthGuard alerts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct LlmHealthState {
    pub consecutive_failures: u32,
    pub last_success: Option<String>,
    pub last_failure_reason: Option<String>,
    pub in_fallback_mode: bool,
    pub task_failures: HashMap<String, u32>,
    pub task_successes: HashMap<String, u32>,
}


// ============================================================================
// GLOBAL GUARDIAN CONFIG
// ============================================================================

/// Complete Guardian configuration — all LLM tasks + global settings.
/// Loaded from config.json section "guardian".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardianConfig {
    // --- Per-task configs (LLM-based) ---
    pub extraction: ExtractionConfig,
    pub coherence: CoherenceConfig,
    pub reactivation: ReactivationConfig,
    pub synthesis: SynthesisConfig,
    pub label_suggestion: LabelSuggestionConfig,
    pub importance_rating: ImportanceRatingConfig,

    // --- Per-system configs (embedding-based) ---
    pub thread_matching: ThreadMatchingConfig,
    pub gossip: GossipConfig,
    pub recall: RecallConfig,

    // --- Engram retrieval (multi-validator consensus) ---
    pub engram: EngramConfig,

    // --- GuardCode (content validation) ---
    pub guardcode: GuardCodeConfig,

    // --- HealthGuard (proactive memory monitoring) ---
    #[serde(default)]
    pub healthguard: crate::healthguard::HealthGuardConfig,

    // --- Heartbeat (agent liveness thresholds) ---
    #[serde(default)]
    pub heartbeat: crate::registry::heartbeat::HeartbeatConfig,

    // --- Hooks (pretool toggles) ---
    #[serde(default)]
    pub hooks: HooksConfig,

    // --- Capture (per-tool toggles) ---
    #[serde(default)]
    pub capture: CaptureConfig,

    // --- Decay & Lifecycle (thread/bridge decay, archival) ---
    #[serde(default)]
    pub decay: DecayConfig,

    // --- Global settings ---
    pub enabled: bool,
    pub claude_cli_path: Option<String>,
    pub hook_guard_env: String,

    pub cache_enabled: bool,
    pub cache_ttl_secs: u64,
    pub cache_max_entries: usize,

    pub pattern_learning_enabled: bool,
    pub pattern_decay_days: f64,
    pub usage_tracking_enabled: bool,

    /// DEPRECATED — replaced by LlmFailureMode in each TaskLlmConfig.
    pub fallback_on_failure: bool,
    pub fallback_patterns: FallbackPatterns,

    pub alert_thresholds: AlertThresholds,
}

impl Default for GuardianConfig {
    fn default() -> Self {
        Self {
            extraction: ExtractionConfig::default(),
            coherence: CoherenceConfig::default(),
            reactivation: ReactivationConfig::default(),
            synthesis: SynthesisConfig::default(),
            label_suggestion: LabelSuggestionConfig::default(),
            importance_rating: ImportanceRatingConfig::default(),
            thread_matching: ThreadMatchingConfig::default(),
            gossip: GossipConfig::default(),
            recall: RecallConfig::default(),
            engram: EngramConfig::default(),
            guardcode: GuardCodeConfig::default(),
            healthguard: crate::healthguard::HealthGuardConfig::default(),
            heartbeat: crate::registry::heartbeat::HeartbeatConfig::default(),
            hooks: HooksConfig::default(),
            capture: CaptureConfig::default(),
            decay: DecayConfig::default(),
            enabled: true,
            claude_cli_path: None,
            hook_guard_env: "AI_SMARTNESS_HOOK_RUNNING".to_string(),
            cache_enabled: false,
            cache_ttl_secs: 300,
            cache_max_entries: 100,
            pattern_learning_enabled: true,
            pattern_decay_days: 30.0,
            usage_tracking_enabled: true,
            fallback_on_failure: false,
            fallback_patterns: FallbackPatterns::default(),
            alert_thresholds: AlertThresholds::default(),
        }
    }
}

// ============================================================================
// HOOKS CONFIG (pretool toggles)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HooksConfig {
    /// Enable Guard Write hook (plan mode enforcement for Edit/Write).
    #[serde(default = "default_true")]
    pub guard_write_enabled: bool,
    /// Auto-allow MCP tool calls without permission prompts.
    #[serde(default = "default_true")]
    pub mcp_auto_allow: bool,
}

impl Default for HooksConfig {
    fn default() -> Self {
        Self {
            guard_write_enabled: true,
            mcp_auto_allow: true,
        }
    }
}

// ============================================================================
// CAPTURE CONFIG (per-tool toggles)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct CaptureConfig {
    /// Per-tool capture toggles.
    #[serde(default)]
    pub tools: CaptureToolToggles,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureToolToggles {
    #[serde(default = "default_true")]
    pub read: bool,
    #[serde(default = "default_true")]
    pub edit: bool,
    #[serde(default = "default_true")]
    pub write: bool,
    #[serde(default = "default_true")]
    pub bash: bool,
    #[serde(default = "default_true")]
    pub grep: bool,
    #[serde(default = "default_true")]
    pub glob: bool,
    #[serde(default = "default_true")]
    pub web_fetch: bool,
    #[serde(default = "default_true")]
    pub web_search: bool,
    #[serde(default = "default_true")]
    pub task: bool,
    #[serde(default = "default_true")]
    pub notebook_edit: bool,
}

impl Default for CaptureToolToggles {
    fn default() -> Self {
        Self {
            read: true,
            edit: true,
            write: true,
            bash: true,
            grep: true,
            glob: true,
            web_fetch: true,
            web_search: true,
            task: true,
            notebook_edit: true,
        }
    }
}

impl CaptureToolToggles {
    pub fn is_enabled(&self, tool_name: &str) -> bool {
        match tool_name {
            "Read" => self.read,
            "Edit" => self.edit,
            "Write" => self.write,
            "Bash" => self.bash,
            "Grep" => self.grep,
            "Glob" => self.glob,
            "WebFetch" => self.web_fetch,
            "WebSearch" => self.web_search,
            "Task" => self.task,
            "NotebookEdit" => self.notebook_edit,
            _ => true,
        }
    }
}

fn default_true() -> bool {
    true
}


// ============================================================================
// DAEMON CONFIG (global daemon settings)
// ============================================================================

/// Configuration for the global daemon process.
/// Loaded from `{data_dir}/daemon_config.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    /// Auto-start daemon when GUI opens.
    pub auto_start: bool,
    /// Max concurrent agent DB connections in the pool.
    pub pool_max_connections: usize,
    /// Evict idle connections after this many seconds.
    pub pool_max_idle_secs: u64,
    /// Interval between prune cycles (seconds).
    pub prune_interval_secs: u64,
    /// Enable cross-project gossip (bridge discovery across projects).
    pub gossip_cross_project: bool,
    /// Number of worker threads for capture processing (LLM extraction).
    /// Each worker processes one capture at a time. Default: min(cpu_cores, 4).
    #[serde(default = "default_capture_workers")]
    pub capture_workers: usize,
    /// Max buffered capture jobs. If full, new jobs are dropped (non-blocking).
    #[serde(default = "default_capture_queue_capacity")]
    pub capture_queue_capacity: usize,
}

fn default_capture_workers() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get().min(4))
        .unwrap_or(2)
}

fn default_capture_queue_capacity() -> usize {
    100
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            auto_start: false,
            pool_max_connections: crate::constants::POOL_MAX_CONNECTIONS,
            pool_max_idle_secs: crate::constants::POOL_MAX_IDLE_SECS,
            prune_interval_secs: crate::constants::PRUNE_INTERVAL_SECS,
            gossip_cross_project: false,
            capture_workers: default_capture_workers(),
            capture_queue_capacity: default_capture_queue_capacity(),
        }
    }
}

impl DaemonConfig {
    /// Load daemon config from `{data_dir}/daemon_config.json`.
    /// Returns defaults if file is missing or invalid.
    pub fn load() -> Self {
        let config_path = crate::storage::path_utils::data_dir().join("daemon_config.json");
        match std::fs::read_to_string(&config_path) {
            Ok(content) => {
                serde_json::from_str(&content).unwrap_or_else(|e| {
                    tracing::warn!(
                        path = %config_path.display(),
                        error = %e,
                        "Invalid daemon config, using defaults"
                    );
                    Self::default()
                })
            }
            Err(_) => Self::default(),
        }
    }

    /// Save daemon config to `{data_dir}/daemon_config.json`.
    pub fn save(&self) -> Result<(), String> {
        let config_path = crate::storage::path_utils::data_dir().join("daemon_config.json");
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config dir: {}", e))?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;
        std::fs::write(&config_path, json)
            .map_err(|e| format!("Failed to write config: {}", e))?;
        Ok(())
    }
}

impl GuardianConfig {
    /// Load from config.json "guardian" section.
    /// Falls back to legacy "llm" section for backward compat.
    pub fn from_config(config: &serde_json::Value) -> Self {
        let mut gc = Self::default();

        // Try "guardian" section first (new format)
        let section = config.get("guardian")
            .or_else(|| config.get("llm"));  // legacy compat

        if let Some(s) = section {
            // Global settings
            if let Some(v) = s.get("enabled").and_then(|v| v.as_bool()) {
                gc.enabled = v;
            }
            if let Some(v) = s.get("claude_cli_path").and_then(|v| v.as_str()) {
                gc.claude_cli_path = Some(v.to_string());
            }
            if let Some(v) = s.get("cache_enabled").and_then(|v| v.as_bool()) {
                gc.cache_enabled = v;
            }
            if let Some(v) = s.get("pattern_learning").and_then(|v| v.as_bool()) {
                gc.pattern_learning_enabled = v;
            }
            if let Some(v) = s.get("usage_tracking").and_then(|v| v.as_bool()) {
                gc.usage_tracking_enabled = v;
            }
            if let Some(v) = s.get("fallback_on_failure").and_then(|v| v.as_bool()) {
                gc.fallback_on_failure = v;
            }

            // --- Embedding-based system configs ---

            // Gossip v2 config
            if let Some(g) = s.get("gossip").and_then(|v| v.as_object()) {
                if let Some(emb) = g.get("embedding").and_then(|v| v.as_object()) {
                    parse_embedding_config(emb, &mut gc.gossip.embedding);
                }
                // Concept gossip v2
                if let Some(v) = g.get("concept_overlap_min_shared").and_then(|v| v.as_u64()) {
                    gc.gossip.concept_overlap_min_shared = v as usize;
                }
                if let Some(v) = g.get("concept_min_bridge_weight").and_then(|v| v.as_f64()) {
                    gc.gossip.concept_min_bridge_weight = v;
                }
                if let Some(v) = g.get("merge_evaluation_threshold").and_then(|v| v.as_f64()) {
                    gc.gossip.merge_evaluation_threshold = v;
                }
                if let Some(v) = g.get("merge_auto_threshold").and_then(|v| v.as_f64()) {
                    gc.gossip.merge_auto_threshold = v;
                }
                if let Some(v) = g.get("concept_gossip_enabled").and_then(|v| v.as_bool()) {
                    gc.gossip.concept_gossip_enabled = v;
                }
                // Legacy
                if let Some(v) = g.get("topic_overlap_enabled").and_then(|v| v.as_bool()) {
                    gc.gossip.topic_overlap_enabled = v;
                }
                if let Some(v) = g.get("topic_overlap_min_shared").and_then(|v| v.as_u64()) {
                    gc.gossip.topic_overlap_min_shared = v as usize;
                }
                // Bridge limits
                if let Some(v) = g.get("min_bridges_per_thread").and_then(|v| v.as_u64()) {
                    gc.gossip.min_bridges_per_thread = v as usize;
                }
                if let Some(v) = g.get("max_bridges_per_thread").and_then(|v| v.as_u64()) {
                    gc.gossip.max_bridges_per_thread = v as usize;
                }
                if let Some(v) = g.get("target_bridge_ratio").and_then(|v| v.as_f64()) {
                    gc.gossip.target_bridge_ratio = v;
                }
                // Propagation config
                if let Some(v) = g.get("propagation_enabled").and_then(|v| v.as_bool()) {
                    gc.gossip.propagation_enabled = v;
                }
                if let Some(v) = g.get("propagation_max_depth").and_then(|v| v.as_u64()) {
                    gc.gossip.propagation_max_depth = v as u32;
                }
                if let Some(v) = g.get("propagation_decay_factor").and_then(|v| v.as_f64()) {
                    gc.gossip.propagation_decay_factor = v;
                }
                if let Some(v) = g.get("propagation_min_weight").and_then(|v| v.as_f64()) {
                    gc.gossip.propagation_min_weight = v;
                }
            }

            // Recall config
            if let Some(r) = s.get("recall").and_then(|v| v.as_object()) {
                if let Some(emb) = r.get("embedding").and_then(|v| v.as_object()) {
                    parse_embedding_config(emb, &mut gc.recall.embedding);
                }
                if let Some(v) = r.get("max_results").and_then(|v| v.as_u64()) {
                    gc.recall.max_results = v as usize;
                }
                if let Some(v) = r.get("focus_boost").and_then(|v| v.as_f64()) {
                    gc.recall.focus_boost = v;
                }
            }

            // Thread matching config
            if let Some(tm) = s.get("thread_matching").and_then(|v| v.as_object()) {
                if let Some(m) = tm.get("mode").and_then(|v| v.as_str()) {
                    gc.thread_matching.mode = match m {
                        "EmbeddingPlusLlm" => ThreadMatchingMode::EmbeddingPlusLlm,
                        _ => ThreadMatchingMode::EmbeddingOnly,
                    };
                }
                if let Some(emb) = tm.get("embedding").and_then(|v| v.as_object()) {
                    parse_embedding_config(emb, &mut gc.thread_matching.embedding);
                }
            }

            // Engram config (multi-validator retrieval)
            if let Some(eg) = s.get("engram").and_then(|v| v.as_object()) {
                if let Some(emb) = eg.get("embedding").and_then(|v| v.as_object()) {
                    parse_embedding_config(emb, &mut gc.engram.embedding);
                }
                if let Some(v) = eg.get("strong_inject_min_votes").and_then(|v| v.as_u64()) {
                    gc.engram.strong_inject_min_votes = v as u8;
                }
                if let Some(v) = eg.get("weak_inject_min_votes").and_then(|v| v.as_u64()) {
                    gc.engram.weak_inject_min_votes = v as u8;
                }
                if let Some(v) = eg.get("max_results").and_then(|v| v.as_u64()) {
                    gc.engram.max_results = v as usize;
                }
                if let Some(v) = eg.get("hash_index_enabled").and_then(|v| v.as_bool()) {
                    gc.engram.hash_index_enabled = v;
                }
                // Validator weights
                if let Some(w) = eg.get("validator_weights").and_then(|v| v.as_object()) {
                    let vw = &mut gc.engram.validator_weights;
                    if let Some(v) = w.get("semantic_similarity").and_then(|v| v.as_f64()) { vw.semantic_similarity = v; }
                    if let Some(v) = w.get("topic_overlap").and_then(|v| v.as_f64()) { vw.topic_overlap = v; }
                    if let Some(v) = w.get("temporal_proximity").and_then(|v| v.as_f64()) { vw.temporal_proximity = v; }
                    if let Some(v) = w.get("graph_connectivity").and_then(|v| v.as_f64()) { vw.graph_connectivity = v; }
                    if let Some(v) = w.get("injection_history").and_then(|v| v.as_f64()) { vw.injection_history = v; }
                    if let Some(v) = w.get("decayed_relevance").and_then(|v| v.as_f64()) { vw.decayed_relevance = v; }
                    if let Some(v) = w.get("label_coherence").and_then(|v| v.as_f64()) { vw.label_coherence = v; }
                    if let Some(v) = w.get("focus_alignment").and_then(|v| v.as_f64()) { vw.focus_alignment = v; }
                    if let Some(v) = w.get("concept_coherence").and_then(|v| v.as_f64()) { vw.concept_coherence = v; }
                }
            }

            // Decay & Lifecycle config
            if let Some(d) = s.get("decay").and_then(|v| v.as_object()) {
                if let Some(v) = d.get("thread_suspend_threshold").and_then(|v| v.as_f64()) { gc.decay.thread_suspend_threshold = v; }
                if let Some(v) = d.get("thread_min_half_life").and_then(|v| v.as_f64()) { gc.decay.thread_min_half_life = v; }
                if let Some(v) = d.get("thread_max_half_life").and_then(|v| v.as_f64()) { gc.decay.thread_max_half_life = v; }
                if let Some(v) = d.get("thread_use_boost").and_then(|v| v.as_f64()) { gc.decay.thread_use_boost = v; }
                if let Some(v) = d.get("orphan_halving_hours").and_then(|v| v.as_f64()) { gc.decay.orphan_halving_hours = v; }
                if let Some(v) = d.get("orphan_min_half_life_factor").and_then(|v| v.as_f64()) { gc.decay.orphan_min_half_life_factor = v; }
                if let Some(v) = d.get("bridge_half_life").and_then(|v| v.as_f64()) { gc.decay.bridge_half_life = v; }
                if let Some(v) = d.get("bridge_death_threshold").and_then(|v| v.as_f64()) { gc.decay.bridge_death_threshold = v; }
                if let Some(v) = d.get("bridge_use_boost").and_then(|v| v.as_f64()) { gc.decay.bridge_use_boost = v; }
                if let Some(v) = d.get("archive_after_hours").and_then(|v| v.as_f64()) { gc.decay.archive_after_hours = v; }
            }

            // Alert thresholds
            if let Some(a) = s.get("alerts").and_then(|v| v.as_object()) {
                if let Some(v) = a.get("warning_after").and_then(|v| v.as_u64()) {
                    gc.alert_thresholds.warning_after = v as u32;
                }
                if let Some(v) = a.get("critical_after").and_then(|v| v.as_u64()) {
                    gc.alert_thresholds.critical_after = v as u32;
                }
                if let Some(v) = a.get("per_system_cooldown_secs").and_then(|v| v.as_u64()) {
                    gc.alert_thresholds.per_system_cooldown_secs = v;
                }
            }

            // Per-task model shortcuts (simple format: "extraction": "sonnet")
            for (key, target) in [
                ("extraction", &mut gc.extraction.llm.model),
                ("coherence", &mut gc.coherence.llm.model),
                ("reactivation", &mut gc.reactivation.llm.model),
                ("synthesis", &mut gc.synthesis.llm.model),
                ("label_suggestion", &mut gc.label_suggestion.llm.model),
            ] {
                if let Some(v) = s.get(key).and_then(|v| v.as_str()) {
                    *target = parse_model(v);
                }
                // Nested format: "extraction": {"model": "sonnet", "timeout": 45}
                if let Some(sub) = s.get(key).and_then(|v| v.as_object()) {
                    if let Some(m) = sub.get("model").and_then(|v| v.as_str()) {
                        *target = parse_model(m);
                    }
                }
            }

            // Per-task detailed overrides
            if let Some(ext) = s.get("extraction").and_then(|v| v.as_object()) {
                if let Some(v) = ext.get("timeout").and_then(|v| v.as_u64()) {
                    gc.extraction.llm.timeout_secs = v;
                }
                if let Some(v) = ext.get("max_content_chars").and_then(|v| v.as_u64()) {
                    gc.extraction.max_content_chars = v as usize;
                }
                if let Some(v) = ext.get("min_capture_length").and_then(|v| v.as_u64()) {
                    gc.extraction.min_capture_length = v as usize;
                }
                if let Some(v) = ext.get("skip_tools").and_then(|v| v.as_array()) {
                    gc.extraction.skip_tools = v.iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect();
                }
                if let Some(v) = ext.get("topic_aliases").and_then(|v| v.as_object()) {
                    for (k, val) in v {
                        if let Some(alias) = val.as_str() {
                            gc.extraction.topic_aliases.insert(k.clone(), alias.to_string());
                        }
                    }
                }
            }

            // Legacy compat: "extraction_model" → extraction.model
            if let Some(v) = s.get("extraction_model").and_then(|v| v.as_str()) {
                gc.extraction.llm.model = parse_model(v);
            }
        }

        gc.validate();
        gc
    }

    /// Validate and clamp all config values to sensible ranges.
    /// Logs warnings for any out-of-range values.
    pub fn validate(&mut self) {
        self.validate_thresholds();
        self.validate_decay();
        self.validate_gossip();
        self.validate_engram();
        self.validate_thread_matching();
        self.validate_embeddings();
    }

    fn validate_thresholds(&mut self) {
        clamp_01(&mut self.decay.thread_suspend_threshold, "decay.thread_suspend_threshold");
        clamp_01(&mut self.decay.bridge_death_threshold, "decay.bridge_death_threshold");
        clamp_01(&mut self.decay.thread_use_boost, "decay.thread_use_boost");
        clamp_01(&mut self.decay.bridge_use_boost, "decay.bridge_use_boost");
        clamp_01(&mut self.decay.orphan_min_half_life_factor, "decay.orphan_min_half_life_factor");
    }

    fn validate_decay(&mut self) {
        // Positive strict
        if self.decay.thread_min_half_life <= 0.0 {
            tracing::warn!(field = "decay.thread_min_half_life", "Must be > 0, resetting to default");
            self.decay.thread_min_half_life = 0.75;
        }
        if self.decay.thread_max_half_life <= 0.0 {
            tracing::warn!(field = "decay.thread_max_half_life", "Must be > 0, resetting to default");
            self.decay.thread_max_half_life = 7.0;
        }
        if self.decay.bridge_half_life <= 0.0 {
            tracing::warn!(field = "decay.bridge_half_life", "Must be > 0, resetting to default");
            self.decay.bridge_half_life = 2.0;
        }
        if self.decay.archive_after_hours <= 0.0 {
            tracing::warn!(field = "decay.archive_after_hours", "Must be > 0, resetting to default");
            self.decay.archive_after_hours = 72.0;
        }
        if self.decay.orphan_halving_hours <= 0.0 {
            tracing::warn!(field = "decay.orphan_halving_hours", "Must be > 0, resetting to default");
            self.decay.orphan_halving_hours = 6.0;
        }
        // Ordering: min < max
        if self.decay.thread_min_half_life >= self.decay.thread_max_half_life {
            tracing::warn!(
                min = self.decay.thread_min_half_life,
                max = self.decay.thread_max_half_life,
                "decay.thread_min_half_life >= thread_max_half_life — swapping"
            );
            std::mem::swap(&mut self.decay.thread_min_half_life, &mut self.decay.thread_max_half_life);
        }
    }

    fn validate_gossip(&mut self) {
        clamp_01(&mut self.gossip.concept_min_bridge_weight, "gossip.concept_min_bridge_weight");
        clamp_01(&mut self.gossip.merge_evaluation_threshold, "gossip.merge_evaluation_threshold");
        clamp_01(&mut self.gossip.merge_auto_threshold, "gossip.merge_auto_threshold");
        clamp_01(&mut self.gossip.propagation_decay_factor, "gossip.propagation_decay_factor");
        clamp_01(&mut self.gossip.propagation_min_weight, "gossip.propagation_min_weight");

        // Positive strict
        if self.gossip.min_bridges_per_thread == 0 {
            tracing::warn!(field = "gossip.min_bridges_per_thread", "Must be > 0, resetting to default");
            self.gossip.min_bridges_per_thread = 3;
        }

        // target_bridge_ratio clamp
        if self.gossip.target_bridge_ratio < 0.5 || self.gossip.target_bridge_ratio > 20.0 {
            tracing::warn!(
                value = self.gossip.target_bridge_ratio,
                "gossip.target_bridge_ratio out of [0.5, 20.0] — clamping"
            );
            self.gossip.target_bridge_ratio = self.gossip.target_bridge_ratio.clamp(0.5, 20.0);
        }

        // Ordering: min_bridges < max_bridges
        if self.gossip.min_bridges_per_thread >= self.gossip.max_bridges_per_thread {
            tracing::warn!(
                min = self.gossip.min_bridges_per_thread,
                max = self.gossip.max_bridges_per_thread,
                "gossip.min_bridges >= max_bridges — swapping"
            );
            std::mem::swap(&mut self.gossip.min_bridges_per_thread, &mut self.gossip.max_bridges_per_thread);
        }

        // Ordering: merge_evaluation < merge_auto
        if self.gossip.merge_evaluation_threshold >= self.gossip.merge_auto_threshold {
            tracing::warn!(
                eval = self.gossip.merge_evaluation_threshold,
                auto = self.gossip.merge_auto_threshold,
                "gossip.merge_evaluation >= merge_auto — swapping"
            );
            std::mem::swap(&mut self.gossip.merge_evaluation_threshold, &mut self.gossip.merge_auto_threshold);
        }
    }

    fn validate_engram(&mut self) {
        // Votes in [1..9]
        self.engram.strong_inject_min_votes = self.engram.strong_inject_min_votes.clamp(1, 9);
        self.engram.weak_inject_min_votes = self.engram.weak_inject_min_votes.clamp(1, 9);

        // weak < strong
        if self.engram.weak_inject_min_votes >= self.engram.strong_inject_min_votes {
            tracing::warn!(
                weak = self.engram.weak_inject_min_votes,
                strong = self.engram.strong_inject_min_votes,
                "engram.weak_inject >= strong_inject — swapping"
            );
            std::mem::swap(&mut self.engram.weak_inject_min_votes, &mut self.engram.strong_inject_min_votes);
        }
    }

    fn validate_thread_matching(&mut self) {
        clamp_01(&mut self.thread_matching.continue_threshold, "thread_matching.continue_threshold");
        clamp_01(&mut self.thread_matching.reactivate_threshold, "thread_matching.reactivate_threshold");
        clamp_01(&mut self.thread_matching.capacity_suspend_threshold, "thread_matching.capacity_suspend_threshold");

        // Ordering: continue < reactivate < capacity_suspend
        if self.thread_matching.continue_threshold >= self.thread_matching.reactivate_threshold {
            tracing::warn!(
                cont = self.thread_matching.continue_threshold,
                react = self.thread_matching.reactivate_threshold,
                "thread_matching.continue >= reactivate — swapping"
            );
            std::mem::swap(
                &mut self.thread_matching.continue_threshold,
                &mut self.thread_matching.reactivate_threshold,
            );
        }
        if self.thread_matching.reactivate_threshold >= self.thread_matching.capacity_suspend_threshold {
            tracing::warn!(
                react = self.thread_matching.reactivate_threshold,
                cap = self.thread_matching.capacity_suspend_threshold,
                "thread_matching.reactivate >= capacity_suspend — swapping"
            );
            std::mem::swap(
                &mut self.thread_matching.reactivate_threshold,
                &mut self.thread_matching.capacity_suspend_threshold,
            );
        }
    }

    fn validate_embeddings(&mut self) {
        validate_embedding(&mut self.thread_matching.embedding, "thread_matching");
        validate_embedding(&mut self.gossip.embedding, "gossip");
        validate_embedding(&mut self.recall.embedding, "recall");
        validate_embedding(&mut self.engram.embedding, "engram");
    }
}

fn clamp_01(val: &mut f64, name: &str) {
    if *val < 0.0 || *val > 1.0 {
        tracing::warn!(field = name, value = *val, "Config out of range [0,1] — clamping");
        *val = val.clamp(0.0, 1.0);
    }
}

fn validate_embedding(cfg: &mut EmbeddingSystemConfig, context: &str) {
    clamp_01(&mut cfg.onnx_threshold, &format!("{}.onnx_threshold", context));
    clamp_01(&mut cfg.tfidf_threshold, &format!("{}.tfidf_threshold", context));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_valid_config() {
        let mut gc = GuardianConfig::default();
        gc.validate();
        // Defaults should pass validation unchanged
        assert_eq!(gc.decay.thread_suspend_threshold, 0.1);
        assert_eq!(gc.decay.thread_min_half_life, 0.75);
        assert_eq!(gc.decay.thread_max_half_life, 7.0);
        assert_eq!(gc.engram.strong_inject_min_votes, 5);
        assert_eq!(gc.engram.weak_inject_min_votes, 3);
        assert_eq!(gc.thread_matching.continue_threshold, 0.25);
    }

    #[test]
    fn test_validate_threshold_out_of_range() {
        let mut gc = GuardianConfig::default();
        gc.decay.thread_suspend_threshold = 1.5;
        gc.decay.bridge_death_threshold = -0.3;
        gc.gossip.concept_min_bridge_weight = 2.0;
        gc.validate();
        assert_eq!(gc.decay.thread_suspend_threshold, 1.0);
        assert_eq!(gc.decay.bridge_death_threshold, 0.0);
        assert_eq!(gc.gossip.concept_min_bridge_weight, 1.0);
    }

    #[test]
    fn test_validate_min_greater_than_max() {
        let mut gc = GuardianConfig::default();
        // Invert min/max half-life
        gc.decay.thread_min_half_life = 10.0;
        gc.decay.thread_max_half_life = 0.5;
        // Invert continue/reactivate
        gc.thread_matching.continue_threshold = 0.80;
        gc.thread_matching.reactivate_threshold = 0.20;
        gc.validate();
        // Should be swapped
        assert!(gc.decay.thread_min_half_life < gc.decay.thread_max_half_life);
        assert!(gc.thread_matching.continue_threshold < gc.thread_matching.reactivate_threshold);
    }

    #[test]
    fn test_validate_engram_votes() {
        let mut gc = GuardianConfig::default();
        // Invert weak/strong
        gc.engram.weak_inject_min_votes = 7;
        gc.engram.strong_inject_min_votes = 2;
        gc.validate();
        assert!(gc.engram.weak_inject_min_votes < gc.engram.strong_inject_min_votes);
        // Out of range
        gc.engram.strong_inject_min_votes = 20;
        gc.validate();
        assert!(gc.engram.strong_inject_min_votes <= 9);
    }

    #[test]
    fn test_guardian_config_default_values() {
        let gc = GuardianConfig::default();
        assert!(gc.enabled);
        assert_eq!(gc.extraction.llm.model, ClaudeModel::Haiku);
        assert_eq!(gc.extraction.max_content_chars, 15000);
        assert_eq!(gc.coherence.child_threshold, 0.6);
        assert_eq!(gc.decay.thread_suspend_threshold, 0.1);
        assert_eq!(gc.engram.strong_inject_min_votes, 5);
        assert!(gc.guardcode.enabled);
    }

    #[test]
    fn test_capture_tool_toggles() {
        let toggles = CaptureToolToggles::default();
        assert!(toggles.is_enabled("Read"));
        assert!(toggles.is_enabled("Bash"));
        // Unknown tools default to enabled
        assert!(toggles.is_enabled("UnknownTool"));
    }

    #[test]
    fn test_embedding_system_active_threshold() {
        let cfg = EmbeddingSystemConfig {
            mode: EmbeddingMode::OnnxWithFallback,
            onnx_threshold: 0.75,
            tfidf_threshold: 0.55,
        };
        assert_eq!(cfg.active_threshold(true), 0.75);
        assert_eq!(cfg.active_threshold(false), 0.55);
    }
}
