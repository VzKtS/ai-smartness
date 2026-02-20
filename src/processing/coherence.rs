//! Coherence checker — LLM-based thread matching decision.
//!
//! Determine si un nouveau contenu est:
//!   - Child (related to parent) -> score >= child_threshold
//!   - Orphan (unrelated but substantial) -> score >= orphan_threshold
//!   - Continue (same topic as pending context) -> extend parent
//!   - Forget (noise) -> score < orphan_threshold, skip

use crate::config::CoherenceConfig;
use crate::constants::truncate_safe;
use crate::AiResult;
use serde::{Deserialize, Serialize};

/// Resultat de la verification de coherence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoherenceResult {
    pub score: f64,
    pub reason: String,
    /// Labels re-evalues lors du check (zero extra LLM cost).
    pub updated_labels: Vec<String>,
}

/// Action derivee du score de coherence.
#[derive(Debug, Clone, PartialEq)]
pub enum CoherenceAction {
    Child,
    Orphan,
    Continue,
    Forget,
}

/// Verifie la coherence entre un contexte parent et un nouveau contenu.
/// Appelle le LLM (claude subprocess) avec le COHERENCE_PROMPT.
/// Falls back to embedding similarity if LLM unavailable.
pub fn check_coherence(
    context: &str,
    content: &str,
    current_labels: &[String],
    config: &CoherenceConfig,
) -> AiResult<CoherenceResult> {
    if !config.llm.enabled {
        // LLM disabled — use embedding fallback directly
        return check_via_embedding(context, content, current_labels, config);
    }

    match check_via_llm(context, content, current_labels, config) {
        Ok(result) => {
            tracing::debug!(score = result.score, reason = %result.reason, "Coherence check (LLM)");
            Ok(result)
        }
        Err(_) => check_via_embedding(context, content, current_labels, config),
    }
}

fn check_via_embedding(
    context: &str,
    content: &str,
    current_labels: &[String],
    _config: &CoherenceConfig,
) -> AiResult<CoherenceResult> {
    let mgr = super::embeddings::EmbeddingManager::global();
    let ctx_emb = mgr.embed(context);
    let cnt_emb = mgr.embed(content);
    let score = mgr.similarity(&ctx_emb, &cnt_emb);

    tracing::debug!(score = score, "Coherence check (embedding fallback)");

    Ok(CoherenceResult {
        score,
        reason: "embedding similarity (LLM fallback)".to_string(),
        updated_labels: current_labels.to_vec(),
    })
}

fn check_via_llm(
    context: &str,
    content: &str,
    current_labels: &[String],
    config: &CoherenceConfig,
) -> AiResult<CoherenceResult> {
    let max_ctx = config.max_context_chars;
    let ctx_truncated = truncate_safe(context, max_ctx);
    let cnt_truncated = truncate_safe(content, max_ctx);

    let prompt = format!(
        r#"Rate coherence between existing context and new content. Return JSON only:
{{"score":0.0-1.0,"reason":"<why>","updated_labels":["label1","label2"]}}

Current labels: {:?}

Context:
{}

New content:
{}"#,
        current_labels, ctx_truncated, cnt_truncated
    );

    let model = config.llm.model.as_cli_flag();
    let response = super::llm_subprocess::call_claude_with_model(&prompt, model)?;

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

/// Determine l'action basee sur le score.
pub fn determine_action(
    score: f64,
    child_threshold: f64,
    orphan_threshold: f64,
) -> CoherenceAction {
    let action = if score >= child_threshold {
        CoherenceAction::Child
    } else if score >= orphan_threshold {
        CoherenceAction::Orphan
    } else {
        CoherenceAction::Forget
    };
    tracing::info!(score = score, action = ?action, "Coherence action determined");
    action
}
