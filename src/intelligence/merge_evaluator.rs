//! Merge Evaluator — LLM hybrid merge for strongly-linked threads.
//!
//! Flow:
//!   1. Gossip v2 produces MergeCandidate (overlap_score >= 0.60)
//!   2. MergeEvaluator calls LLM to decide: merge or reject
//!   3. On merge: reuse existing merge logic (move messages, union metadata)
//!   4. On reject: reduce bridge confidence by penalty
//!
//! Threshold tiers:
//!   - score >= 0.85 → auto merge (daemon, no user intervention)
//!   - score 0.60-0.85 → stored for HealthGuard suggestion (future)
//!
//! Atomicity: merge is wrapped in transaction (conn.transaction()).

use crate::config::EmbeddingMode;
use crate::constants::*;
use crate::processing::embeddings::EmbeddingManager;
use crate::processing::llm_subprocess;
use crate::storage::bridges::BridgeStorage;
use crate::storage::threads::ThreadStorage;
use crate::thread::Thread;
use crate::{AiError, AiResult};
use rusqlite::Connection;

use super::gossip::MergeCandidate;

/// Max chars for the merge evaluation prompt (two threads combined).
const MERGE_MAX_CHARS: usize = 30_000;
/// Max messages per thread in the prompt.
const MERGE_MAX_MESSAGES: usize = 5;
/// Max chars per message in the prompt.
const MERGE_MSG_MAX_CHARS: usize = 500;

/// Result of the LLM merge evaluation.
#[derive(Debug)]
pub enum MergeDecision {
    Merge {
        survivor_id: String,
        absorbed_id: String,
        merged_title: String,
        merged_summary: String,
    },
    Reject {
        reason: String,
    },
}

pub struct MergeEvaluator;

impl MergeEvaluator {
    /// Evaluate a merge candidate and execute if approved.
    /// Returns Ok(true) if merge was executed, Ok(false) if rejected.
    pub fn evaluate_and_execute(conn: &Connection, candidate: &MergeCandidate, embed_mode: &EmbeddingMode) -> AiResult<bool> {
        let thread_a = ThreadStorage::get(conn, &candidate.thread_a)?
            .ok_or_else(|| AiError::ThreadNotFound(candidate.thread_a.clone()))?;
        let thread_b = ThreadStorage::get(conn, &candidate.thread_b)?
            .ok_or_else(|| AiError::ThreadNotFound(candidate.thread_b.clone()))?;

        let decision = Self::evaluate_via_llm(&thread_a, &thread_b, candidate, conn)?;

        match decision {
            MergeDecision::Merge {
                survivor_id,
                absorbed_id,
                merged_title,
                merged_summary,
            } => {
                tracing::info!(
                    survivor = %&survivor_id[..8.min(survivor_id.len())],
                    absorbed = %&absorbed_id[..8.min(absorbed_id.len())],
                    title = %merged_title,
                    "MergeEvaluator: executing merge"
                );

                Self::execute_merge(conn, &survivor_id, &absorbed_id, &merged_title, &merged_summary, embed_mode)?;
                Ok(true)
            }
            MergeDecision::Reject { reason } => {
                tracing::info!(
                    thread_a = %&candidate.thread_a[..8.min(candidate.thread_a.len())],
                    thread_b = %&candidate.thread_b[..8.min(candidate.thread_b.len())],
                    reason = %reason,
                    "MergeEvaluator: merge rejected"
                );

                // Reduce bridge confidence
                if let Ok(Some(bridge)) = BridgeStorage::get(conn, &candidate.bridge_id) {
                    let new_confidence = (bridge.confidence - GOSSIP_MERGE_REJECTION_PENALTY).max(0.0);
                    let new_reason = format!("{};merge_rejected:{}", bridge.reason, reason);
                    let mut updated = bridge;
                    updated.confidence = new_confidence;
                    updated.reason = new_reason;
                    BridgeStorage::update(conn, &updated)?;
                }
                Ok(false)
            }
        }
    }

    /// Call LLM to decide merge.
    fn evaluate_via_llm(
        thread_a: &Thread,
        thread_b: &Thread,
        candidate: &MergeCandidate,
        conn: &Connection,
    ) -> AiResult<MergeDecision> {
        let prompt = Self::build_prompt(thread_a, thread_b, candidate, conn)?;

        let response = match llm_subprocess::call_claude_with_model(&prompt, "haiku") {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("MergeEvaluator LLM call failed: {}", e);
                // On LLM failure, reject (safe default)
                return Ok(MergeDecision::Reject {
                    reason: format!("LLM unavailable: {}", e),
                });
            }
        };

        Self::parse_response(&response, thread_a, thread_b)
    }

    /// Build the LLM prompt for merge evaluation.
    fn build_prompt(
        thread_a: &Thread,
        thread_b: &Thread,
        candidate: &MergeCandidate,
        conn: &Connection,
    ) -> AiResult<String> {
        let msgs_a = Self::format_messages(conn, &thread_a.id)?;
        let msgs_b = Self::format_messages(conn, &thread_b.id)?;

        let prompt = format!(
            r#"You are a memory merge evaluator. Two memory threads share {shared_count} concepts.

## Thread A — "{title_a}"
Topics: {topics_a}
Labels: {labels_a}
Concepts: {concepts_a}
Summary: {summary_a}
Messages (recent):
{msgs_a}

## Thread B — "{title_b}"
Topics: {topics_b}
Labels: {labels_b}
Concepts: {concepts_b}
Summary: {summary_b}
Messages (recent):
{msgs_b}

## Shared concepts: {shared}
## Overlap score: {score:.2}

## Decision

Are these threads about the SAME subject captured at different moments, or DIFFERENT subjects that share vocabulary?

Rules:
- MERGE if they describe the same project, feature, bug, or conversation topic
- REJECT if they are about different subjects that happen to share technical terms
- When merging, choose the thread with more content as survivor

Return JSON only:
{{"decision":"merge","survivor":"A"|"B","title":"<merged title>","summary":"<1-sentence merged summary>"}}
or
{{"decision":"reject","reason":"<why different>"}}
"#,
            shared_count = candidate.shared_concepts.len(),
            title_a = thread_a.title,
            topics_a = thread_a.topics.join(", "),
            labels_a = thread_a.labels.join(", "),
            concepts_a = thread_a.concepts.join(", "),
            summary_a = thread_a.summary.as_deref().unwrap_or("(no summary)"),
            msgs_a = msgs_a,
            title_b = thread_b.title,
            topics_b = thread_b.topics.join(", "),
            labels_b = thread_b.labels.join(", "),
            concepts_b = thread_b.concepts.join(", "),
            summary_b = thread_b.summary.as_deref().unwrap_or("(no summary)"),
            msgs_b = msgs_b,
            shared = candidate.shared_concepts.join(", "),
            score = candidate.overlap_score,
        );

        // Truncate if too long
        if prompt.len() > MERGE_MAX_CHARS {
            Ok(truncate_safe(&prompt, MERGE_MAX_CHARS).to_string())
        } else {
            Ok(prompt)
        }
    }

    /// Format recent messages for a thread (truncated).
    fn format_messages(conn: &Connection, thread_id: &str) -> AiResult<String> {
        let msgs = ThreadStorage::get_messages(conn, thread_id)?;
        if msgs.is_empty() {
            return Ok("(no messages)".to_string());
        }

        // Take first 2 + last 3 if >5 messages, otherwise all
        let selected: Vec<&crate::thread::ThreadMessage> = if msgs.len() > MERGE_MAX_MESSAGES {
            let mut sel = Vec::new();
            sel.extend(msgs.iter().take(2));
            sel.extend(msgs.iter().rev().take(3).collect::<Vec<_>>().into_iter().rev());
            sel
        } else {
            msgs.iter().collect()
        };

        let lines: Vec<String> = selected
            .iter()
            .map(|m| {
                let content = if m.content.len() > MERGE_MSG_MAX_CHARS {
                    format!("{}...", truncate_safe(&m.content, MERGE_MSG_MAX_CHARS - 3))
                } else {
                    m.content.clone()
                };
                format!("- {}", content)
            })
            .collect();

        Ok(lines.join("\n"))
    }

    /// Parse LLM response into MergeDecision.
    fn parse_response(
        response: &str,
        thread_a: &Thread,
        thread_b: &Thread,
    ) -> AiResult<MergeDecision> {
        // Extract JSON from response
        let json_str = if let Some(start) = response.find('{') {
            if let Some(end) = response.rfind('}') {
                &response[start..=end]
            } else {
                response
            }
        } else {
            return Ok(MergeDecision::Reject {
                reason: "LLM response not JSON".to_string(),
            });
        };

        let v: serde_json::Value = match serde_json::from_str(json_str) {
            Ok(v) => v,
            Err(e) => {
                return Ok(MergeDecision::Reject {
                    reason: format!("Failed to parse LLM JSON: {}", e),
                });
            }
        };

        let decision = v.get("decision").and_then(|d| d.as_str()).unwrap_or("reject");

        if decision == "merge" {
            let survivor_choice = v.get("survivor").and_then(|s| s.as_str()).unwrap_or("A");
            let (survivor_id, absorbed_id) = if survivor_choice == "B" {
                (thread_b.id.clone(), thread_a.id.clone())
            } else {
                (thread_a.id.clone(), thread_b.id.clone())
            };

            let title = v
                .get("title")
                .and_then(|t| t.as_str())
                .unwrap_or(&thread_a.title)
                .to_string();
            let summary = v
                .get("summary")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string();

            Ok(MergeDecision::Merge {
                survivor_id,
                absorbed_id,
                merged_title: title,
                merged_summary: summary,
            })
        } else {
            let reason = v
                .get("reason")
                .and_then(|r| r.as_str())
                .unwrap_or("LLM rejected merge")
                .to_string();
            Ok(MergeDecision::Reject { reason })
        }
    }

    /// Execute the merge: move messages, union metadata, recalculate embedding, delete absorbed.
    /// Wrapped in a transaction for atomicity.
    fn execute_merge(
        conn: &Connection,
        survivor_id: &str,
        absorbed_id: &str,
        new_title: &str,
        new_summary: &str,
        embed_mode: &EmbeddingMode,
    ) -> AiResult<()> {
        // Note: rusqlite Connection.execute_batch runs in implicit transaction.
        // For explicit transaction, we use conn.execute("BEGIN") pattern.
        conn.execute_batch("BEGIN IMMEDIATE")
            .map_err(|e| AiError::Storage(format!("Transaction begin failed: {}", e)))?;

        let result = Self::execute_merge_inner(conn, survivor_id, absorbed_id, new_title, new_summary, embed_mode);

        match result {
            Ok(()) => {
                conn.execute_batch("COMMIT")
                    .map_err(|e| AiError::Storage(format!("Transaction commit failed: {}", e)))?;
                Ok(())
            }
            Err(e) => {
                conn.execute_batch("ROLLBACK").ok();
                Err(e)
            }
        }
    }

    fn execute_merge_inner(
        conn: &Connection,
        survivor_id: &str,
        absorbed_id: &str,
        new_title: &str,
        new_summary: &str,
        embed_mode: &EmbeddingMode,
    ) -> AiResult<()> {
        let survivor = ThreadStorage::get(conn, survivor_id)?
            .ok_or_else(|| AiError::ThreadNotFound(survivor_id.to_string()))?;
        let absorbed = ThreadStorage::get(conn, absorbed_id)?
            .ok_or_else(|| AiError::ThreadNotFound(absorbed_id.to_string()))?;

        // 1. Move messages from absorbed to survivor
        let msgs = ThreadStorage::get_messages(conn, absorbed_id)?;
        for mut msg in msgs {
            msg.thread_id = survivor_id.to_string();
            ThreadStorage::add_message(conn, &msg)?;
        }

        // 2. Union metadata with consolidation (dedup + substring removal + caps)
        let mut merged = survivor.clone();
        merged.title = new_title.to_string();
        if !new_summary.is_empty() {
            merged.summary = Some(new_summary.to_string());
        }
        super::merge_metadata::consolidate_after_merge(&mut merged, &absorbed);

        // 3. Recalculate embedding for survivor (respects GUI mode toggle)
        let embed_text = format!(
            "{} {}",
            merged.title,
            merged.summary.as_deref().unwrap_or("")
        );
        if let Some(embedding) = EmbeddingManager::global().embed_with_mode(&embed_text, embed_mode) {
            merged.embedding = Some(embedding);
        }

        // 4. Update survivor, delete absorbed
        ThreadStorage::update(conn, &merged)?;
        // Move bridges from absorbed to survivor (or delete)
        BridgeStorage::delete_for_thread(conn, absorbed_id)?;
        ThreadStorage::delete(conn, absorbed_id)?;

        tracing::info!(
            survivor = %&survivor_id[..8.min(survivor_id.len())],
            absorbed = %&absorbed_id[..8.min(absorbed_id.len())],
            title = %new_title,
            topics = merged.topics.len(),
            concepts = merged.concepts.len(),
            "Merge executed"
        );

        Ok(())
    }
}
