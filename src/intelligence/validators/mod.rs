//! Engram Validators — 8 independent signals for injection consensus.
//!
//! Each validator implements the Validator trait, returning a binary vote
//! (pass/fail) with a confidence score (0.0-1.0).
//!
//! | # | Validator             | Signal                              | Cost     |
//! |---|-----------------------|-------------------------------------|----------|
//! | 1 | SemanticSimilarity    | Cosine ONNX/TF-IDF                 | medium   |
//! | 2 | TopicOverlap          | Shared topics with query            | zero     |
//! | 3 | TemporalProximity     | WorkContext freshness               | zero     |
//! | 4 | GraphConnectivity     | Bridges to active thread            | low      |
//! | 5 | InjectionHistory      | injection_stats.usage_ratio()       | zero     |
//! | 6 | DecayedRelevance      | weight × importance                 | zero     |
//! | 7 | LabelCoherence        | Label matching (action→action)      | zero     |
//! | 8 | FocusAlignment        | ai_focus weight boost               | zero     |

use std::collections::HashMap;
use crate::thread::Thread;

/// Result of a single validator's vote.
#[derive(Debug, Clone)]
pub struct ValidatorVote {
    /// Whether this validator considers the thread relevant.
    pub pass: bool,
    /// Confidence in the vote (0.0=uncertain, 1.0=certain).
    pub confidence: f64,
}

/// Query context shared across all validators.
pub struct QueryContext {
    pub user_message: String,
    pub query_embedding: Vec<f32>,
    pub query_topics: Vec<String>,
    pub active_thread_id: Option<String>,
    pub focus_topics: Vec<(String, f64)>,  // (topic, weight)
    pub label_hint: Option<String>,
    /// Pre-computed bridge connections from active thread.
    /// Maps thread_id → max bridge weight for threads connected to the active thread.
    pub bridge_connections: HashMap<String, f64>,
}

/// Trait for each independent validator.
pub trait Validator: Send + Sync {
    fn name(&self) -> &'static str;
    fn validate(&self, thread: &Thread, ctx: &QueryContext) -> ValidatorVote;
}

// --- V1: Semantic Similarity (cosine ONNX/TF-IDF) ---

pub struct SemanticSimilarityValidator {
    pub threshold: f64,
}

impl Validator for SemanticSimilarityValidator {
    fn name(&self) -> &'static str { "semantic_similarity" }
    fn validate(&self, thread: &Thread, ctx: &QueryContext) -> ValidatorVote {
        let embedding = match &thread.embedding {
            Some(e) if !e.is_empty() => e,
            _ => return ValidatorVote { pass: false, confidence: 0.0 },
        };
        if ctx.query_embedding.is_empty() {
            // No query embedding available — neutral vote (don't penalize)
            return ValidatorVote { pass: true, confidence: 0.3 };
        }
        let sim = cosine_similarity(embedding, &ctx.query_embedding);
        ValidatorVote {
            pass: sim >= self.threshold,
            confidence: sim.clamp(0.0, 1.0),
        }
    }
}

/// Cosine similarity between two f32 vectors.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() { return 0.0; }
    let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| *x as f64 * *y as f64).sum();
    let norm_a: f64 = a.iter().map(|x| (*x as f64).powi(2)).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| (*x as f64).powi(2)).sum::<f64>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 { return 0.0; }
    dot / (norm_a * norm_b)
}

// --- V2: Topic Overlap ---

pub struct TopicOverlapValidator {
    pub min_shared: usize,
}

impl Validator for TopicOverlapValidator {
    fn name(&self) -> &'static str { "topic_overlap" }
    fn validate(&self, thread: &Thread, ctx: &QueryContext) -> ValidatorVote {
        let shared = thread.topics.iter()
            .filter(|t| {
                let t_lower = t.to_lowercase();
                ctx.query_topics.iter().any(|q| q.to_lowercase() == t_lower)
            })
            .count();
        ValidatorVote {
            pass: shared >= self.min_shared,
            confidence: (shared as f64 / ctx.query_topics.len().max(1) as f64).min(1.0),
        }
    }
}

// --- V3: Temporal Proximity (WorkContext freshness) ---

pub struct TemporalProximityValidator;

impl Validator for TemporalProximityValidator {
    fn name(&self) -> &'static str { "temporal_proximity" }
    fn validate(&self, thread: &Thread, _ctx: &QueryContext) -> ValidatorVote {
        let freshness = thread.work_context.as_ref()
            .map(|wc| wc.freshness_factor())
            .unwrap_or(0.0);
        ValidatorVote {
            pass: freshness > 0.1,
            confidence: freshness,
        }
    }
}

// --- V4: Graph Connectivity (bridges to active thread) ---

pub struct GraphConnectivityValidator;

impl Validator for GraphConnectivityValidator {
    fn name(&self) -> &'static str { "graph_connectivity" }
    fn validate(&self, thread: &Thread, ctx: &QueryContext) -> ValidatorVote {
        if ctx.active_thread_id.is_none() {
            // No active thread to check connectivity against — neutral
            return ValidatorVote { pass: true, confidence: 0.2 };
        }
        match ctx.bridge_connections.get(&thread.id) {
            Some(&weight) => ValidatorVote {
                pass: true,
                confidence: weight.clamp(0.0, 1.0),
            },
            None => ValidatorVote {
                pass: false,
                confidence: 0.0,
            },
        }
    }
}

// --- V5: Injection History (usage_ratio feedback) ---

pub struct InjectionHistoryValidator;

impl Validator for InjectionHistoryValidator {
    fn name(&self) -> &'static str { "injection_history" }
    fn validate(&self, thread: &Thread, _ctx: &QueryContext) -> ValidatorVote {
        match &thread.injection_stats {
            Some(stats) => {
                let ratio = stats.usage_ratio();
                ValidatorVote {
                    pass: ratio >= 0.2 || stats.injection_count < 3,
                    confidence: ratio,
                }
            }
            None => ValidatorVote { pass: true, confidence: 0.5 },  // no data = neutral
        }
    }
}

// --- V6: Decayed Relevance (weight × importance) ---

pub struct DecayedRelevanceValidator {
    pub min_score: f64,
}

impl Validator for DecayedRelevanceValidator {
    fn name(&self) -> &'static str { "decayed_relevance" }
    fn validate(&self, thread: &Thread, _ctx: &QueryContext) -> ValidatorVote {
        let score = thread.weight * thread.importance;
        ValidatorVote {
            pass: score >= self.min_score,
            confidence: score.min(1.0),
        }
    }
}

// --- V7: Label Coherence (action→action matching) ---

pub struct LabelCoherenceValidator;

impl Validator for LabelCoherenceValidator {
    fn name(&self) -> &'static str { "label_coherence" }
    fn validate(&self, thread: &Thread, ctx: &QueryContext) -> ValidatorVote {
        match &ctx.label_hint {
            Some(hint) => {
                let matches = thread.labels.iter().any(|l| l == hint);
                ValidatorVote {
                    pass: matches || thread.labels.is_empty(),
                    confidence: if matches { 0.9 } else if thread.labels.is_empty() { 0.3 } else { 0.1 },
                }
            }
            None => ValidatorVote { pass: true, confidence: 0.3 },  // no hint = neutral
        }
    }
}

// --- V8: Focus Alignment (ai_focus boost) ---

pub struct FocusAlignmentValidator;

impl Validator for FocusAlignmentValidator {
    fn name(&self) -> &'static str { "focus_alignment" }
    fn validate(&self, thread: &Thread, ctx: &QueryContext) -> ValidatorVote {
        if ctx.focus_topics.is_empty() {
            return ValidatorVote { pass: true, confidence: 0.3 };  // no focus = neutral
        }
        let mut max_match = 0.0f64;
        for (topic, weight) in &ctx.focus_topics {
            let topic_lower = topic.to_lowercase();
            if thread.topics.iter().any(|t| t.to_lowercase() == topic_lower)
                || thread.title.to_lowercase().contains(&topic_lower)
                || thread.id == *topic
            {
                max_match = max_match.max(*weight);
            }
        }
        ValidatorVote {
            pass: max_match > 0.0,
            confidence: max_match,
        }
    }
}
