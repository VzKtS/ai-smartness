//! Tool Extractor — LLM-based summarization for tool captures.
//!
//! Separate pipeline from extractor.rs (which handles human/agent exchanges).
//! Produces a single-pass summary + reference (file_path/URL) for memory storage.
//! The agent keeps a concise summary and can re-access full data via the reference.

use crate::config::{ExtractionConfig, LocalModelSize};
use crate::processing::extractor::{
    self, Extraction, ExtractionMode, ExtractionResult,
};
use crate::processing::llm_subprocess;
use crate::processing::prompt_loader::{self, PromptName};
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
    model: &LocalModelSize,
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

    let prompt = build_tool_prompt(&truncated, source_type, file_path, agent_context, model)?;

    tracing::info!(
        source_type = source_type,
        file_path = ?file_path,
        content_chars = truncated.chars().count(),
        prompt_len = prompt.len(),
        "Tool extraction: calling LLM"
    );

    // Retry loop: up to 3 attempts (2 retries max) on LLM failure or degenerate output
    let mut last_err = None;
    for attempt in 0..3u8 {
        let response = match llm_subprocess::call_llm(&prompt) {
            Ok(r) => r,
            Err(e) => {
                if attempt < 2 {
                    tracing::warn!(
                        attempt = attempt + 1,
                        error = %e,
                        "Tool extraction: LLM call failed, retrying"
                    );
                    last_err = Some(e);
                    continue;
                }
                tracing::warn!(error = %e, "Tool extraction: LLM call failed after 3 attempts");
                return Err(e);
            }
        };

        tracing::info!(
            response_len = response.len(),
            attempt = attempt + 1,
            "Tool extraction: LLM response received"
        );

        // Reuse the same parser as extractor.rs (handles JSON repair, etc.)
        match extractor::parse_tool_extraction_response(&response) {
            Ok(ExtractionResult::Extracted(mut ext)) => {
                // Force Summary mode for all tool extractions
                ext.extraction_mode = ExtractionMode::Summary;

                // Gate 1: detect degenerate extraction (LLM returned placeholders like "...").
                if extractor::is_degenerate_extraction(&ext) {
                    if attempt < 2 {
                        tracing::warn!(
                            attempt = attempt + 1,
                            title = %ext.title,
                            summary = %ext.summary,
                            "Tool extraction: degenerate output detected, retrying"
                        );
                        continue;
                    }
                    tracing::warn!(
                        title = %ext.title,
                        summary = %ext.summary,
                        "Tool extraction: degenerate output after 3 attempts — dropping"
                    );
                    return Ok(None);
                }

                // Gate 2: detect truncated JSON (both fields at serde default 0.0).
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
                return Ok(Some(ext));
            }
            Ok(ExtractionResult::Skip) => {
                tracing::info!("Tool extraction: LLM decided to skip");
                return Ok(None);
            }
            Err(e) => {
                if attempt < 2 {
                    tracing::warn!(
                        attempt = attempt + 1,
                        error = %e,
                        "Tool extraction: parse failed, retrying"
                    );
                    last_err = Some(e);
                    continue;
                }
                tracing::warn!(error = %e, "Tool extraction: parse failed after 3 attempts");
                return Err(e);
            }
        }
    }
    // All retries exhausted (should not reach here, but safety net)
    Err(last_err.unwrap_or_else(|| crate::AiError::Provider("All retries exhausted".into())))
}

/// Build the tool-specific prompt (shorter than extractor.rs prompt).
fn build_tool_prompt(
    content: &str,
    source_type: &str,
    file_path: Option<&str>,
    agent_context: Option<&str>,
    model: &LocalModelSize,
) -> AiResult<String> {
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
    // Load prompt template (need meta for max_context_chars)
    let loaded = prompt_loader::load_prompt(model, PromptName::ToolExtractor)?;
    let max_ctx = loaded.meta.max_context_chars;

    let context_block = match agent_context {
        Some(ctx) if !ctx.is_empty() => format!(
            "The agent was recently working on:\n---\n{}\n---\nScore importance based on alignment with agent's current activity.",
            crate::constants::truncate_safe(ctx, max_ctx)
        ),
        _ => String::from("No agent context available. Score based on content richness alone."),
    };

    Ok(loaded.template.prompt
        .replace("{source_type}", source_type)
        .replace("{tool_desc}", tool_desc)
        .replace("{ref_line}", &ref_line)
        .replace("{content}", content)
        .replace("{context_block}", &context_block))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_tool_prompt_with_file_path() {
        let model = LocalModelSize::Phi4Mini;
        let prompt = build_tool_prompt(
            "fn main() { println!(\"hello\"); }",
            "Read",
            Some("src/main.rs"),
            None,
            &model,
        ).unwrap();
        assert!(prompt.contains("Reference: src/main.rs"));
        assert!(prompt.contains("file content that was read"));
        assert!(prompt.contains("fn main()"));
    }

    #[test]
    fn test_build_tool_prompt_without_file_path() {
        let model = LocalModelSize::Phi4Mini;
        let prompt = build_tool_prompt("search results here", "WebSearch", None, None, &model).unwrap();
        assert!(prompt.contains("Reference: none"));
        assert!(prompt.contains("web search results"));
    }

    #[test]
    fn test_build_tool_prompt_with_agent_context() {
        let model = LocalModelSize::Phi4Mini;
        let prompt = build_tool_prompt(
            "test content",
            "Task",
            None,
            Some("Working on refactoring the config module"),
            &model,
        ).unwrap();
        assert!(prompt.contains("agent was recently working on"));
        assert!(prompt.contains("refactoring the config module"));
    }

    #[test]
    fn test_build_tool_prompt_bash() {
        let model = LocalModelSize::Phi4Mini;
        let prompt = build_tool_prompt(
            "error: cannot find module",
            "Bash",
            None,
            None,
            &model,
        ).unwrap();
        assert!(prompt.contains("terminal command output"));
    }

    #[test]
    fn test_build_tool_prompt_all_source_types() {
        let model = LocalModelSize::Phi4Mini;
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
            let prompt = build_tool_prompt("content", src, None, None, &model).unwrap();
            assert!(
                prompt.contains(expected_desc),
                "Source type '{}' should contain '{}'",
                src,
                expected_desc
            );
        }
    }
}
