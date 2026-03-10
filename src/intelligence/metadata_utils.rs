//! Metadata utilities — deduplication and limits for thread topics/labels/concepts.

/// Max topics after consolidation.
pub const MAX_TOPICS: usize = 10;
/// Max labels after consolidation.
pub const MAX_LABELS: usize = 5;
/// Max concepts after consolidation.
pub const MAX_CONCEPTS: usize = 25;

/// Case-insensitive deduplication. Keeps the first occurrence's casing.
pub fn dedup_case_insensitive(items: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    items
        .into_iter()
        .filter(|item| seen.insert(item.to_lowercase()))
        .collect()
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
    fn test_dedup_case_insensitive_preserves_first() {
        let input = vec!["Rust".into(), "rust".into(), "RUST".into()];
        let result = dedup_case_insensitive(input);
        assert_eq!(result, vec!["Rust"]);
    }

    #[test]
    fn test_topics_cap_at_max() {
        let mut topics: Vec<String> = (0..8).map(|i| format!("topic_{i}")).collect();
        let new_topics: Vec<String> = (6..14).map(|i| format!("topic_{i}")).collect();
        for t in &new_topics {
            topics.push(t.clone());
        }
        topics = dedup_case_insensitive(topics);
        topics.truncate(MAX_TOPICS);
        assert_eq!(topics.len(), MAX_TOPICS);
    }

    #[test]
    fn test_labels_cap_at_max() {
        let mut labels: Vec<String> = (0..10).map(|i| format!("label_{i}")).collect();
        labels = dedup_case_insensitive(labels);
        labels.truncate(MAX_LABELS);
        assert_eq!(labels.len(), MAX_LABELS);
    }
}
