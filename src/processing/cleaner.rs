use crate::constants::MIN_CAPTURE_LENGTH;

/// Clean tool output for memory storage — remove ANSI codes, collapse whitespace.
pub fn clean_tool_output(raw: &str) -> String {
    // Remove ANSI escape codes
    let re = regex::Regex::new(r"\x1b\[[0-9;]*m").unwrap();
    let stripped = re.replace_all(raw, "");

    // Collapse multiple newlines
    let re_newlines = regex::Regex::new(r"\n{3,}").unwrap();
    let collapsed = re_newlines.replace_all(&stripped, "\n\n");

    // Collapse multiple spaces (not newlines)
    let re_spaces = regex::Regex::new(r"[^\S\n]{2,}").unwrap();
    let cleaned = re_spaces.replace_all(&collapsed, " ");

    cleaned.trim().to_string()
}

/// Clean text for embedding/comparison — lowercase, remove punctuation excess.
pub fn clean_text(raw: &str) -> String {
    let stripped = raw.trim();
    // Collapse whitespace
    let re = regex::Regex::new(r"\s+").unwrap();
    re.replace_all(stripped, " ").to_string()
}

/// Determine if content is worth capturing (not too short, not binary junk).
pub fn should_capture(content: &str) -> bool {
    let trimmed = content.trim();

    // Too short
    if trimmed.len() < MIN_CAPTURE_LENGTH {
        tracing::debug!(len = trimmed.len(), min = MIN_CAPTURE_LENGTH, "Capture rejected: too short");
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
