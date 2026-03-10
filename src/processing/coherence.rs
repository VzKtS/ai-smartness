//! Coherence checker — subject-aware thread matching decision.
//!
//! Determines whether a new capture is about the same subject as the previous one:
//!   - Child (score > child_threshold) → same subject, continuity chain continues
//!   - Orphan (score ≤ child_threshold) → subject changed, chain breaks intentionally
//!
//! Compares extraction metadata (title, subjects, concepts) instead of raw text.
//! Natural topic drift A→B→C is detected via subject overlap.

use crate::config::{CoherenceConfig, LocalModelSize};
use crate::processing::prompt_loader::{self, PromptName};
use crate::AiResult;
use serde::{Deserialize, Serialize};

/// Result of coherence check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoherenceResult {
    pub score: f64,
    pub reason: String,
    /// Labels re-evaluated during the check (zero extra LLM cost).
    pub updated_labels: Vec<String>,
}

/// Action derived from coherence score — binary decision.
#[derive(Debug, Clone, PartialEq)]
pub enum CoherenceAction {
    /// Same subject → continuity edge (chain continues)
    Child,
    /// Subject changed → no continuity edge (chain breaks)
    Orphan,
}

/// Enriched input for subject-aware coherence checking.
/// Carries extraction metadata from PendingContext and new capture.
pub struct CoherenceInput<'a> {
    /// Previous capture metadata
    pub prev_title: &'a str,
    pub prev_subjects: &'a [String],
    pub prev_concepts: &'a [String],
    pub prev_labels: &'a [String],
    pub prev_content: &'a str,
    /// New capture metadata
    pub new_title: &'a str,
    pub new_subjects: &'a [String],
    pub new_concepts: &'a [String],
    pub new_content: &'a str,
}

/// Check subject coherence between previous and new capture.
/// Uses LLM with structured metadata, falls back to embedding similarity.
pub fn check_coherence(
    input: &CoherenceInput,
    config: &CoherenceConfig,
    model: &LocalModelSize,
) -> AiResult<CoherenceResult> {
    if !config.llm.enabled {
        return check_via_embedding(input, config);
    }

    match check_via_llm(input, config, model) {
        Ok(result) => {
            tracing::debug!(score = result.score, reason = %result.reason, "Coherence check (LLM)");
            Ok(result)
        }
        Err(_) => check_via_embedding(input, config),
    }
}

fn check_via_embedding(
    input: &CoherenceInput,
    _config: &CoherenceConfig,
) -> AiResult<CoherenceResult> {
    let mgr = super::embeddings::EmbeddingManager::global();

    // Embed structured metadata (title + subjects + concepts) instead of raw content
    let prev_text = format!(
        "{} {} {}",
        input.prev_title,
        input.prev_subjects.join(" "),
        input.prev_concepts.join(" ")
    );
    let new_text = format!(
        "{} {} {}",
        input.new_title,
        input.new_subjects.join(" "),
        input.new_concepts.join(" ")
    );

    let prev_emb = mgr.embed(&prev_text);
    let new_emb = mgr.embed(&new_text);
    let score = mgr.similarity(&prev_emb, &new_emb);

    tracing::debug!(score = score, "Coherence check (embedding fallback)");

    Ok(CoherenceResult {
        score,
        reason: "embedding similarity on metadata (LLM fallback)".to_string(),
        updated_labels: input.prev_labels.to_vec(),
    })
}

fn check_via_llm(
    input: &CoherenceInput,
    _config: &CoherenceConfig,
    model: &LocalModelSize,
) -> AiResult<CoherenceResult> {
    let template = prompt_loader::get_template(model, PromptName::Coherence)?;
    let prompt = template
        .replace("{prev_title}", input.prev_title)
        .replace("{prev_subjects}", &format!("{:?}", input.prev_subjects))
        .replace("{prev_concepts}", &format!("{:?}", input.prev_concepts))
        .replace("{new_title}", input.new_title)
        .replace("{new_subjects}", &format!("{:?}", input.new_subjects))
        .replace("{new_concepts}", &format!("{:?}", input.new_concepts))
        .replace("{prev_labels}", &format!("{:?}", input.prev_labels));

    let response = super::llm_subprocess::call_llm(&prompt)?;

    // Parse JSON from response
    let json_str = if let Some(start) = response.find('{') {
        if let Some(end) = response.rfind('}') {
            &response[start..=end]
        } else {
            &response
        }
    } else {
        &response
    };

    serde_json::from_str(json_str).map_err(|e| {
        crate::AiError::InvalidInput(format!("Failed to parse coherence: {}", e))
    })
}

/// Determine action from coherence score — binary Child/Orphan.
/// Child = score > child_threshold (same subject, continuity chain continues).
/// Orphan = score <= child_threshold (subject changed, chain breaks).
pub fn determine_action(score: f64, child_threshold: f64) -> CoherenceAction {
    let action = if score > child_threshold {
        CoherenceAction::Child
    } else {
        CoherenceAction::Orphan
    };
    tracing::info!(score = score, threshold = child_threshold, action = ?action, "Coherence action");
    action
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_determine_action_child() {
        assert_eq!(determine_action(0.5, 0.3), CoherenceAction::Child);
    }

    #[test]
    fn test_determine_action_orphan() {
        assert_eq!(determine_action(0.2, 0.3), CoherenceAction::Orphan);
    }

    #[test]
    fn test_determine_action_boundary() {
        // score == threshold → Orphan (strictly >)
        assert_eq!(determine_action(0.3, 0.3), CoherenceAction::Orphan);
    }

    #[test]
    fn test_determine_action_high_score() {
        assert_eq!(determine_action(0.9, 0.3), CoherenceAction::Child);
    }

    #[test]
    fn test_determine_action_zero() {
        assert_eq!(determine_action(0.0, 0.3), CoherenceAction::Orphan);
    }
}
