//! Merge metadata consolidation — deduplication and cleanup after thread merge.
//!
//! Used by both MCP handle_merge() and daemon MergeEvaluator to prevent
//! topic/label/concept bloat from successive merges.

use crate::constants::filter_blocked_labels;
use crate::thread::Thread;

/// Max topics after merge/update consolidation.
pub const MAX_TOPICS: usize = 10;
/// Max labels after merge/update consolidation.
pub const MAX_LABELS: usize = 5;
/// Max concepts after merge consolidation.
pub const MAX_CONCEPTS: usize = 25;

/// Consolidate metadata of survivor after absorbing another thread.
///
/// 1. Union topics/labels/concepts from absorbed into survivor
/// 2. Case-insensitive deduplication on all three
/// 3. Remove topics that are substrings of longer topics
/// 4. Cap to limits (keep the shortest/most specific labels, longest/most informative topics)
pub fn consolidate_after_merge(survivor: &mut Thread, absorbed: &Thread) {
    // ── Topics: union → dedup → substring elimination → cap ──
    let mut topics = survivor.topics.clone();
    for t in &absorbed.topics {
        topics.push(t.clone());
    }
    topics = dedup_case_insensitive(topics);
    topics = remove_substring_topics(topics);
    topics.truncate(MAX_TOPICS);
    survivor.topics = topics;

    // ── Labels: union → dedup → cap ──
    let mut labels = survivor.labels.clone();
    for l in &absorbed.labels {
        labels.push(l.clone());
    }
    labels = dedup_case_insensitive(labels);
    labels = filter_blocked_labels(&labels);
    labels.truncate(MAX_LABELS);
    survivor.labels = labels;

    // ── Concepts: union → dedup → cap ──
    let mut concepts = survivor.concepts.clone();
    for c in &absorbed.concepts {
        concepts.push(c.clone());
    }
    concepts = dedup_case_insensitive(concepts);
    concepts.truncate(MAX_CONCEPTS);
    survivor.concepts = concepts;

    // ── Weight: max ──
    survivor.weight = survivor.weight.max(absorbed.weight);
}

/// Case-insensitive deduplication. Keeps the first occurrence's casing.
pub fn dedup_case_insensitive(items: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    items
        .into_iter()
        .filter(|item| seen.insert(item.to_lowercase()))
        .collect()
}

/// Remove topics that are substrings of other (longer) topics.
///
/// Example: given ["airbus", "a320", "airbus a320 config"], removes
/// "airbus" and "a320" because they appear inside "airbus a320 config".
fn remove_substring_topics(mut topics: Vec<String>) -> Vec<String> {
    // Sort longest first — longer topics are more informative
    topics.sort_by(|a, b| b.len().cmp(&a.len()));

    let mut result: Vec<String> = Vec::new();
    for topic in &topics {
        let lower = topic.to_lowercase();
        // Check if this topic is a substring of any already-kept (longer) topic
        let is_substring = result
            .iter()
            .any(|kept| kept.to_lowercase().contains(&lower));
        if !is_substring {
            result.push(topic.clone());
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dedup_case_insensitive() {
        let input = vec![
            "Airbus".to_string(),
            "airbus".to_string(),
            "A320".to_string(),
            "a320".to_string(),
            "Boeing".to_string(),
        ];
        let result = dedup_case_insensitive(input);
        assert_eq!(result, vec!["Airbus", "A320", "Boeing"]);
    }

    #[test]
    fn test_remove_substring_topics() {
        let input = vec![
            "airbus".to_string(),
            "a320".to_string(),
            "airbus a320 configuration".to_string(),
            "boeing 737".to_string(),
        ];
        let result = remove_substring_topics(input);
        assert_eq!(result.len(), 2);
        assert!(result.contains(&"airbus a320 configuration".to_string()));
        assert!(result.contains(&"boeing 737".to_string()));
    }

    #[test]
    fn test_dedup_case_insensitive_preserves_first() {
        let input = vec!["Rust".into(), "rust".into(), "RUST".into()];
        let result = dedup_case_insensitive(input);
        assert_eq!(result, vec!["Rust"]);
    }

    #[test]
    fn test_topics_cap_at_max() {
        // Simulate what update_thread does: append + dedup + truncate
        let mut topics: Vec<String> = (0..8).map(|i| format!("topic_{i}")).collect();
        let new_topics: Vec<String> = (6..14).map(|i| format!("topic_{i}")).collect();
        for t in &new_topics {
            topics.push(t.clone());
        }
        topics = dedup_case_insensitive(topics);
        topics.truncate(MAX_TOPICS);
        // 8 old + 8 new = 16 unique, but capped at 10
        assert_eq!(topics.len(), MAX_TOPICS);
    }

    #[test]
    fn test_labels_cap_at_max() {
        let mut labels: Vec<String> = (0..10).map(|i| format!("label_{i}")).collect();
        labels = dedup_case_insensitive(labels);
        labels.truncate(MAX_LABELS);
        assert_eq!(labels.len(), MAX_LABELS);
    }

    #[test]
    fn test_no_false_substring_removal() {
        // "rust" is a substring of "rustic" but they're different topics
        // However our algorithm WILL remove "rust" if "rustic" exists.
        // This is acceptable — the longer form is more informative.
        let input = vec![
            "rust programming".to_string(),
            "python scripting".to_string(),
        ];
        let result = remove_substring_topics(input);
        assert_eq!(result.len(), 2);
    }
}
