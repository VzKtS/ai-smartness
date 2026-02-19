//! Reactivation Decider -- determine whether a suspended/archived thread
//! should be reactivated based on embedding similarity and LLM judgment.

use crate::constants::REACTIVATION_HIGH_CONFIDENCE;
use crate::thread::Thread;
use crate::AiResult;
use crate::processing::embeddings::EmbeddingManager;
use crate::processing::llm_subprocess;
use rusqlite::Connection;

const REACTIVATION_BORDERLINE: f64 = 0.15;

pub struct ReactivationDecider;

impl ReactivationDecider {
    /// Quick embedding-based reactivation check.
    pub fn should_reactivate(thread: &Thread, context: &str) -> bool {
        let embeddings = EmbeddingManager::global();
        let ctx_emb = embeddings.embed(context);

        if let Some(ref thread_emb) = thread.embedding {
            let sim = embeddings.similarity(&ctx_emb, thread_emb);
            let result = sim >= REACTIVATION_HIGH_CONFIDENCE;
            tracing::debug!(thread_id = %thread.id, similarity = sim, threshold = REACTIVATION_HIGH_CONFIDENCE, reactivate = result, "Reactivation check");
            result
        } else {
            tracing::debug!(thread_id = %thread.id, "No embedding, skipping reactivation");
            false
        }
    }

    /// LLM-based reactivation for borderline cases (0.15 <= sim < 0.35).
    pub fn should_reactivate_llm(
        _conn: &Connection,
        context: &str,
        thread: &Thread,
        similarity: f64,
    ) -> AiResult<bool> {
        if similarity >= REACTIVATION_HIGH_CONFIDENCE {
            tracing::debug!(thread_id = %thread.id, similarity = similarity, "LLM reactivation: high confidence, auto-yes");
            return Ok(true);
        }
        if similarity < REACTIVATION_BORDERLINE {
            tracing::debug!(thread_id = %thread.id, similarity = similarity, "LLM reactivation: below borderline, auto-no");
            return Ok(false);
        }

        tracing::debug!(thread_id = %thread.id, similarity = similarity, "LLM reactivation: borderline, calling LLM");

        let ctx_preview = if context.len() > 500 {
            &context[..500]
        } else {
            context
        };

        let prompt = format!(
            "Is thread '{}' (topics: {}) relevant to context: '{}'?\n\
             Respond JSON only: {{\"relevant\": true/false}}",
            thread.title,
            thread.topics.join(", "),
            ctx_preview
        );

        match llm_subprocess::call_claude(&prompt) {
            Ok(response) => {
                if let Some(start) = response.find('{') {
                    if let Some(end) = response.rfind('}') {
                        let json_str = &response[start..=end];
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(json_str) {
                            return Ok(v
                                .get("relevant")
                                .and_then(|r| r.as_bool())
                                .unwrap_or(false));
                        }
                    }
                }
                Ok(false)
            }
            Err(e) => {
                tracing::warn!(error = %e, "LLM reactivation call failed, falling back to threshold");
                Ok(similarity >= (REACTIVATION_HIGH_CONFIDENCE + REACTIVATION_BORDERLINE) / 2.0)
            }
        }
    }
}
