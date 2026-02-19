//! Merge candidate detection — reads gossip bridges to find merge-worthy pairs.
//!
//! Queries bridges created by gossip_v2 with weight in [0.60, auto_threshold)
//! and returns actionable merge findings for injection.

use crate::AiResult;
use crate::storage::bridges::BridgeStorage;
use crate::storage::threads::ThreadStorage;
use rusqlite::Connection;

use super::{HealthFinding, HealthPriority};

/// A merge candidate detected from gossip bridges.
#[derive(Debug, Clone)]
pub struct DetectedMergeCandidate {
    pub bridge_id: String,
    pub thread_a_id: String,
    pub thread_b_id: String,
    pub thread_a_title: String,
    pub thread_b_title: String,
    pub weight: f64,
    pub shared_concepts: Vec<String>,
}

/// Detect merge candidates from gossip bridges.
///
/// Scans active bridges created by gossip_v2 where weight is between
/// `merge_eval_threshold` (0.60) and `merge_auto_threshold` (0.85).
/// These are candidates too strong to ignore but too uncertain for auto-merge.
pub fn detect_merge_candidates(
    conn: &Connection,
    merge_eval_threshold: f64,
    merge_auto_threshold: f64,
    max_candidates: usize,
) -> AiResult<Vec<DetectedMergeCandidate>> {
    let bridges = BridgeStorage::list_active(conn)?;

    let mut candidates: Vec<DetectedMergeCandidate> = Vec::new();

    for bridge in &bridges {
        // Only gossip_v2 concept overlap bridges
        if !bridge.created_by.starts_with("gossip") {
            continue;
        }

        // Weight in [eval_threshold, auto_threshold)
        if bridge.weight < merge_eval_threshold || bridge.weight >= merge_auto_threshold {
            continue;
        }

        // Both threads must still exist and be active
        let thread_a = match ThreadStorage::get(conn, &bridge.source_id)? {
            Some(t) if t.status == crate::thread::ThreadStatus::Active => t,
            _ => continue,
        };
        let thread_b = match ThreadStorage::get(conn, &bridge.target_id)? {
            Some(t) if t.status == crate::thread::ThreadStatus::Active => t,
            _ => continue,
        };

        candidates.push(DetectedMergeCandidate {
            bridge_id: bridge.id.clone(),
            thread_a_id: bridge.source_id.clone(),
            thread_b_id: bridge.target_id.clone(),
            thread_a_title: thread_a.title,
            thread_b_title: thread_b.title,
            weight: bridge.weight,
            shared_concepts: bridge.shared_concepts.clone(),
        });

        if candidates.len() >= max_candidates {
            break;
        }
    }

    // Sort by weight descending — strongest candidates first
    candidates.sort_by(|a, b| b.weight.partial_cmp(&a.weight).unwrap_or(std::cmp::Ordering::Equal));

    Ok(candidates)
}

/// Convert detected merge candidates into HealthFindings for injection.
pub fn merge_candidates_to_findings(candidates: &[DetectedMergeCandidate]) -> Vec<HealthFinding> {
    candidates
        .iter()
        .map(|c| {
            let concepts_display = if c.shared_concepts.len() > 5 {
                let shown: Vec<&str> = c.shared_concepts.iter().take(5).map(|s| s.as_str()).collect();
                format!("{}, +{} more", shown.join(", "), c.shared_concepts.len() - 5)
            } else {
                c.shared_concepts.join(", ")
            };

            HealthFinding {
                priority: HealthPriority::High,
                category: "merge_candidate".to_string(),
                message: format!(
                    "Threads \"{}\" and \"{}\" share {} concepts ({}) with {:.0}% overlap",
                    truncate_title(&c.thread_a_title, 40),
                    truncate_title(&c.thread_b_title, 40),
                    c.shared_concepts.len(),
                    concepts_display,
                    c.weight * 100.0,
                ),
                action: format!(
                    "Evaluate with: ai_merge survivor_id=\"{}\" absorbed_id=\"{}\"",
                    c.thread_a_id, c.thread_b_id,
                ),
                metric_value: c.weight,
                threshold: 0.60,
            }
        })
        .collect()
}

fn truncate_title(title: &str, max: usize) -> String {
    if title.chars().count() <= max {
        title.to_string()
    } else {
        let truncated: String = title.chars().take(max - 3).collect();
        format!("{}...", truncated)
    }
}
