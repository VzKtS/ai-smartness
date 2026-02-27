//! Tool Extractor — LLM-based summarization for tool captures.
//!
//! Separate pipeline from extractor.rs (which handles human/agent exchanges).
//! Produces a single-pass summary + reference (file_path/URL) for memory storage.
//! The agent keeps a concise summary and can re-access full data via the reference.

use crate::config::ExtractionConfig;
use crate::processing::extractor::{
    self, Extraction, ExtractionMode, ExtractionResult,
};
use crate::processing::llm_subprocess;
use crate::AiResult;

/// Summarize tool output in a single LLM pass.
///
/// Returns an `Extraction` ready for ThreadManager (same struct as extractor.rs).
/// Content is truncated to `max_tool_content_chars` — no chunking, no re-queue.
pub fn summarize_tool_output(
    content: &str,
    source_type: &str,
    file_path: Option<&str>,
    agent_context: Option<&str>,
    extraction_cfg: &ExtractionConfig,
) -> AiResult<Option<Extraction>> {
    if !extraction_cfg.llm.enabled {
        tracing::info!(mode = "disabled", "Tool extraction: LLM disabled, skipping");
        return Ok(None);
    }

    let max_chars = extraction_cfg.max_tool_content_chars;
    let truncated: String = if content.chars().count() > max_chars {
        tracing::info!(
            original_chars = content.chars().count(),
            truncated_to = max_chars,
            "Tool extraction: content truncated to max_tool_content_chars"
        );
        content.chars().take(max_chars).collect()
    } else {
        content.to_string()
    };

    let prompt = build_tool_prompt(&truncated, source_type, file_path, agent_context);

    tracing::info!(
        source_type = source_type,
        file_path = ?file_path,
        content_chars = truncated.chars().count(),
        prompt_len = prompt.len(),
        "Tool extraction: calling LLM"
    );

    let response = match llm_subprocess::call_llm(&prompt) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(
                error = %e,
                "Tool extraction: LLM call failed"
            );
            return Err(e);
        }
    };

    tracing::info!(
        response_len = response.len(),
        "Tool extraction: LLM response received"
    );

    // Reuse the same parser as extractor.rs (handles JSON repair, etc.)
    match extractor::parse_tool_extraction_response(&response) {
        Ok(ExtractionResult::Extracted(mut ext)) => {
            // Force Summary mode for all tool extractions
            ext.extraction_mode = ExtractionMode::Summary;

            // Post-processing: detect truncated JSON (both fields at serde default 0.0).
            // Pattern is impossible from real LLM output — assign reduced scores
            // so the thread exists but with lower engram weight.
            if ext.confidence == 0.0 && ext.importance == 0.0 {
                ext.confidence = 0.3;
                ext.importance = 0.3;
                ext.from_partial = true;
                tracing::warn!(
                    title = %ext.title,
                    "Tool extraction: truncated output detected — from_partial=true, scores set to 0.3"
                );
            }

            tracing::info!(
                title = %ext.title,
                confidence = ext.confidence,
                importance = ext.importance,
                "Tool extraction: success"
            );
            Ok(Some(ext))
        }
        Ok(ExtractionResult::Skip) => {
            tracing::info!("Tool extraction: LLM decided to skip");
            Ok(None)
        }
        Err(e) => {
            tracing::warn!(error = %e, "Tool extraction: parse failed");
            Err(e)
        }
    }
}

/// Build the tool-specific prompt (shorter than extractor.rs prompt).
fn build_tool_prompt(
    content: &str,
    source_type: &str,
    file_path: Option<&str>,
    agent_context: Option<&str>,
) -> String {
    let tool_desc = match source_type {
        "Read" | "file_read" => "file content that was read",
        "Write" | "file_write" => "file content that was written or modified",
        "Edit" => "file diff (code modification)",
        "Bash" | "command" => "terminal command output",
        "WebFetch" | "fetch" => "web page content fetched from a URL",
        "WebSearch" => "web search results",
        "Task" | "task" => "delegated task result from a sub-agent",
        "NotebookEdit" => "Jupyter notebook cell modification",
        _ => "tool output",
    };

    let ref_line = match file_path {
        Some(fp) => format!("Reference: {}", fp),
        None => "Reference: none".to_string(),
    };

    // PROCEDURAL ORDER — same principle as extractor.rs:
    // Step 1: classify content WITHOUT agent context (unbiased)
    // Step 2: score importance WITH agent context
    // This prevents context from biasing classification for Engram validity.
    let context_block = match agent_context {
        Some(ctx) if !ctx.is_empty() => format!(
            "The agent was recently working on:\n---\n{}\n---\nScore importance based on alignment with agent's current activity.",
            crate::constants::truncate_safe(ctx, 500)
        ),
        _ => String::from("No agent context available. Score based on content richness alone."),
    };

    format!(
        r#"You are a memory assistant. Summarize the following tool output.

## Step 1 — Classification (analyze content below, NO external context)

Tool: {source_type} ({tool_desc})
{ref_line}

Content:
---
{content}
---

- title: max 50 chars, descriptive
- summary: max 250 chars, what it contains and why it matters
- subjects: 2-3 key subjects
- labels: 1-3 classification labels (e.g. "architecture", "config", "test-output")
- concepts: 5-15 associative concepts (synonyms, related domains, hyponym, technologies)
- confidence: 0.0 to 1.0 — how well you understood the content

## Step 2 — Importance scoring

{context_block}

- importance: 0.0 to 1.0 — how important is this for the agent

Output ONLY a single JSON object, nothing else:
{{"title":"...","summary":"...","subjects":[...],"labels":[...],"concepts":[...],"confidence":0.0,"importance":0.0}}"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_tool_prompt_with_file_path() {
        let prompt = build_tool_prompt(
            "fn main() { println!(\"hello\"); }",
            "Read",
            Some("src/main.rs"),
            None,
        );
        assert!(prompt.contains("Reference: src/main.rs"));
        assert!(prompt.contains("file content that was read"));
        assert!(prompt.contains("fn main()"));
    }

    #[test]
    fn test_build_tool_prompt_without_file_path() {
        let prompt = build_tool_prompt("search results here", "WebSearch", None, None);
        assert!(prompt.contains("Reference: none"));
        assert!(prompt.contains("web search results"));
    }

    #[test]
    fn test_build_tool_prompt_with_agent_context() {
        let prompt = build_tool_prompt(
            "test content",
            "Task",
            None,
            Some("Working on refactoring the config module"),
        );
        assert!(prompt.contains("agent was recently working on"));
        assert!(prompt.contains("refactoring the config module"));
    }

    #[test]
    fn test_build_tool_prompt_bash() {
        let prompt = build_tool_prompt(
            "error: cannot find module",
            "Bash",
            None,
            None,
        );
        assert!(prompt.contains("terminal command output"));
    }

    #[test]
    fn test_build_tool_prompt_all_source_types() {
        let types = [
            ("Read", "file content that was read"),
            ("Write", "file content that was written"),
            ("Edit", "file diff"),
            ("Bash", "terminal command output"),
            ("WebFetch", "web page content"),
            ("WebSearch", "web search results"),
            ("Task", "delegated task result"),
            ("NotebookEdit", "Jupyter notebook"),
        ];
        for (src, expected_desc) in types {
            let prompt = build_tool_prompt("content", src, None, None);
            assert!(
                prompt.contains(expected_desc),
                "Source type '{}' should contain '{}'",
                src,
                expected_desc
            );
        }
    }
}
