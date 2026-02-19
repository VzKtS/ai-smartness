//! Synthesis -- generate thread summaries from messages.

use crate::thread::ThreadMessage;

pub struct Synthesis;

impl Synthesis {
    /// Generate a heuristic summary from thread messages.
    pub fn summarize(messages: &[ThreadMessage]) -> String {
        tracing::debug!(message_count = messages.len(), "Generating summary");
        if messages.is_empty() {
            return String::new();
        }

        let first = &messages[0].content;
        let first_truncated = if first.len() > 200 {
            format!("{}...", &first[..197])
        } else {
            first.clone()
        };

        if messages.len() <= 3 {
            return first_truncated;
        }

        let last = &messages[messages.len() - 1].content;
        let last_truncated = if last.len() > 100 {
            format!("{}...", &last[..97])
        } else {
            last.clone()
        };

        format!("Origin: {} Latest: {}", first_truncated, last_truncated)
    }
}
