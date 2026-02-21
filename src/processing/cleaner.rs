use std::sync::LazyLock;

use crate::constants::MIN_CAPTURE_LENGTH;

static RE_ANSI: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"\x1b\[[0-9;]*m").unwrap());
static RE_NEWLINES: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"\n{3,}").unwrap());
static RE_SPACES: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"[^\S\n]{2,}").unwrap());
static RE_WHITESPACE: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"\s+").unwrap());

/// Clean tool output for memory storage — remove ANSI codes, collapse whitespace.
pub fn clean_tool_output(raw: &str) -> String {
    // Remove ANSI escape codes
    let stripped = RE_ANSI.replace_all(raw, "");

    // Collapse multiple newlines
    let collapsed = RE_NEWLINES.replace_all(&stripped, "\n\n");

    // Collapse multiple spaces (not newlines)
    let cleaned = RE_SPACES.replace_all(&collapsed, " ");

    cleaned.trim().to_string()
}

/// Clean text for embedding/comparison — lowercase, remove punctuation excess.
pub fn clean_text(raw: &str) -> String {
    let stripped = raw.trim();
    // Collapse whitespace
    RE_WHITESPACE.replace_all(stripped, " ").to_string()
}

/// Determine if content is worth capturing (not too short, not binary junk).
/// Uses the hardcoded MIN_CAPTURE_LENGTH constant.
pub fn should_capture(content: &str) -> bool {
    should_capture_with_config(content, MIN_CAPTURE_LENGTH)
}

/// Config-driven capture filter. Uses `min_len` from GuardianConfig.extraction.min_capture_length.
pub fn should_capture_with_config(content: &str, min_len: usize) -> bool {
    let trimmed = content.trim();

    // Too short
    if trimmed.len() < min_len {
        tracing::debug!(len = trimmed.len(), min = min_len, "Capture rejected: too short");
        return false;
    }

    // Binary-looking (high ratio of non-ASCII)
    let non_ascii = trimmed.bytes().filter(|b| !b.is_ascii()).count();
    if non_ascii > trimmed.len() / 4 {
        tracing::debug!(non_ascii_ratio = non_ascii as f64 / trimmed.len() as f64, "Capture rejected: binary content");
        return false;
    }

    // Entirely whitespace or repetitive
    let unique_chars: std::collections::HashSet<char> = trimmed.chars().collect();
    if unique_chars.len() < 5 {
        tracing::debug!(unique_chars = unique_chars.len(), "Capture rejected: repetitive content");
        return false;
    }

    true
}

/// Extract simple topics from text using heuristics (used as LLM fallback).
pub fn extract_topics(text: &str) -> Vec<String> {
    let clean = clean_text(text).to_lowercase();
    let words: Vec<&str> = clean.split_whitespace().collect();

    // Extract capitalized words and repeated terms as candidate topics
    let mut counts = std::collections::HashMap::new();
    for word in &words {
        let w = word.trim_matches(|c: char| !c.is_alphanumeric());
        if w.len() >= 3 {
            *counts.entry(w.to_string()).or_insert(0u32) += 1;
        }
    }

    // Return words that appear 2+ times, sorted by frequency
    let mut topics: Vec<(String, u32)> = counts
        .into_iter()
        .filter(|(_, c)| *c >= 2)
        .collect();
    topics.sort_by(|a, b| b.1.cmp(&a.1));
    topics.into_iter().take(10).map(|(w, _)| w).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_tool_output_ansi() {
        let input = "\x1b[31mError\x1b[0m: something failed";
        let result = clean_tool_output(input);
        assert_eq!(result, "Error: something failed");
    }

    #[test]
    fn test_clean_tool_output_collapses_newlines() {
        let input = "line1\n\n\n\n\nline2";
        let result = clean_tool_output(input);
        assert_eq!(result, "line1\n\nline2");
    }

    #[test]
    fn test_clean_tool_output_collapses_spaces() {
        let input = "word1     word2    word3";
        let result = clean_tool_output(input);
        assert_eq!(result, "word1 word2 word3");
    }

    #[test]
    fn test_clean_text_whitespace() {
        let input = "  hello   world  \n\t foo  ";
        let result = clean_text(input);
        assert_eq!(result, "hello world foo");
    }

    #[test]
    fn test_should_capture_too_short() {
        assert!(!should_capture("short")); // < 20 chars
    }

    #[test]
    fn test_should_capture_valid() {
        let content = "This is a normal piece of text that should be captured by the system";
        assert!(should_capture(content));
    }

    #[test]
    fn test_should_capture_repetitive() {
        // Fewer than 5 unique chars
        let content = "aaaaaaaaaaaabbbbbbbbbbbb";
        assert!(!should_capture(content));
    }

    #[test]
    fn test_should_capture_with_config_custom_min() {
        let content = "This text is 50 chars long approximately here now.";
        assert!(should_capture_with_config(content, 20));
        assert!(!should_capture_with_config(content, 100));
    }

    #[test]
    fn test_extract_topics_frequency() {
        let text = "rust rust programming programming test";
        let topics = extract_topics(text);
        assert!(topics.contains(&"rust".to_string()));
        assert!(topics.contains(&"programming".to_string()));
        // "test" appears only once -> excluded
        assert!(!topics.contains(&"test".to_string()));
    }

    #[test]
    fn test_extract_topics_short_words_filtered() {
        let text = "go go go is is an an";
        let topics = extract_topics(text);
        // "go" and "is" and "an" are < 3 chars
        assert!(topics.is_empty());
    }
}
