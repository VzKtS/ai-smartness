//! Extraction — LLM-based content extraction.
//!
//! Uses local LLM to extract structured metadata from raw content.
//! Config-driven: model, truncation, prompt quality all from GuardianConfig.
//! No heuristic fallback — quality over quantity.

use crate::config::{ExtractionConfig, ImportanceRatingConfig, LabelSuggestionConfig};
use crate::constants::truncate_safe;
use crate::AiResult;
use serde::{Deserialize, Serialize};

/// How a thread's content was processed by the LLM.
/// No Heuristic variant — heuristic fallback is removed entirely.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum ExtractionMode {
    /// LLM returned content verbatim (short, already readable)
    Verbatim,
    /// LLM synthesized/extracted structured metadata
    #[default]
    Extract,
}

/// Parsed LLM extraction result — includes action decision.
pub enum ExtractionResult {
    Skip,
    Extracted(Extraction),
}

/// Extraction result from LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Extraction {
    pub title: String,
    pub subjects: Vec<String>,
    pub summary: String,
    pub confidence: f64,
    pub labels: Vec<String>,
    #[serde(default)]
    pub concepts: Vec<String>,
    pub importance: f64,
    /// How the LLM processed this content.
    #[serde(default)]
    pub extraction_mode: ExtractionMode,
}

/// Source type for extraction prompts.
#[derive(Debug, Clone, Copy)]
pub enum ExtractionSource {
    Prompt,
    FileRead,
    FileWrite,
    Task,
    Fetch,
    Response,
    Command,
}

impl ExtractionSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Prompt => "prompt",
            Self::FileRead => "file_read",
            Self::FileWrite => "file_write",
            Self::Task => "task",
            Self::Fetch => "fetch",
            Self::Response => "response",
            Self::Command => "command",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            Self::Prompt => "user message or prompt",
            Self::FileRead => "file content that was read",
            Self::FileWrite => "file content being written or modified",
            Self::Task => "delegated task result",
            Self::Fetch => "web content fetched from a URL",
            Self::Response => "AI assistant response",
            Self::Command => "terminal/shell command output",
        }
    }
}

/// Extract structured data from content using LLM.
/// Returns None if LLM decides to skip or if extraction fails.
/// No heuristic fallback — quality over quantity.
///
/// `agent_context` — optional recent context from the agent's activity.
/// Used ONLY for importance scoring (Step 2), never for classification (Step 1).
pub fn extract(
    content: &str,
    source: ExtractionSource,
    extraction_cfg: &ExtractionConfig,
    label_cfg: &LabelSuggestionConfig,
    importance_cfg: &ImportanceRatingConfig,
    agent_context: Option<&str>,
) -> AiResult<Option<Extraction>> {
    if !extraction_cfg.llm.enabled {
        tracing::info!(mode = "disabled", "Extraction: LLM disabled, skipping");
        return Ok(None);
    }

    match extract_via_llm(content, source, extraction_cfg, label_cfg, importance_cfg, agent_context) {
        Ok(ExtractionResult::Skip) => {
            tracing::info!(mode = "llm_skip", "Extraction: LLM decided to skip");
            Ok(None)
        }
        Ok(ExtractionResult::Extracted(mut extraction)) => {
            // Prompt and Response: override summary with verbatim content (truncated).
            // These are already human-readable — LLM synthesis only degrades them.
            if matches!(source, ExtractionSource::Prompt | ExtractionSource::Response) {
                extraction.summary = truncate_safe(content, crate::constants::VERBATIM_SUMMARY_LIMIT).to_string();
            }
            tracing::info!(mode = "llm", title = %extraction.title, confidence = extraction.confidence, "Extraction complete");
            Ok(Some(extraction))
        }
        Err(e) => {
            tracing::warn!("LLM extraction failed: {} — dropping capture (no heuristic fallback)", e);
            Ok(None)
        }
    }
}

/// LLM-based extraction via local LLM.
fn extract_via_llm(
    content: &str,
    source: ExtractionSource,
    extraction_cfg: &ExtractionConfig,
    label_cfg: &LabelSuggestionConfig,
    importance_cfg: &ImportanceRatingConfig,
    agent_context: Option<&str>,
) -> AiResult<ExtractionResult> {
    let prompt = build_extraction_prompt(content, source, extraction_cfg, label_cfg, importance_cfg, agent_context);

    match super::llm_subprocess::call_claude(&prompt) {
        Ok(response) => {
            tracing::info!(response_len = response.len(), "LLM extraction response received");
            tracing::debug!(raw_response = %response, "LLM raw output");
            parse_extraction_response(&response)
        }
        Err(e) => {
            tracing::warn!("LLM extraction failed: {}", e);
            Err(e)
        }
    }
}

fn build_extraction_prompt(
    content: &str,
    source: ExtractionSource,
    extraction_cfg: &ExtractionConfig,
    label_cfg: &LabelSuggestionConfig,
    importance_cfg: &ImportanceRatingConfig,
    agent_context: Option<&str>,
) -> String {
    let max_chars = extraction_cfg.max_content_chars;
    let truncated: String = if content.chars().count() > max_chars {
        content.chars().take(max_chars).collect()
    } else {
        content.to_string()
    };

    let noise_words: Vec<&str> = extraction_cfg.topic_noise_words.iter().map(|s| s.as_str()).collect();
    let score_map = &importance_cfg.score_map;

    // Label hint: only if vocabulary is non-empty
    let label_hint = if label_cfg.label_vocabulary.is_empty() {
        String::new()
    } else {
        let vocab: Vec<&str> = label_cfg.label_vocabulary.iter().map(|s| s.as_str()).collect();
        format!("\nOptional vocabulary hints (use ONLY if they genuinely match): [{}]", vocab.join(", "))
    };

    // Context block for Step 2 — only if agent_context is provided
    let context_block = match agent_context {
        Some(ctx) if !ctx.is_empty() => format!(
            r#"

The agent was recently working on:
---
{}
---
Use this context to judge how aligned the classified content is with the agent's current activity.
Higher alignment = higher importance. No alignment does NOT mean low importance — content may be independently valuable."#,
            ctx
        ),
        _ => String::from("\nNo additional context available. Score based on acquisition source and content richness alone."),
    };

    format!(
        r#"Your role is to process the content provided to you in order to extract only the text that are humanly understandable.
        Follow these rules:
        1. If not humanly comprehensible = skip
        2. If the number of humanly comprehensible characters is less than 150 characters = skip
        3. If the humanly comprehensible ratio is less than 20% of the capture = skip
        4. If the humanly comprehensible ratio is less than 50% of the capture = verbatim
        5. If the number of humanly comprehensible characters is greater than 150 characters and less than 500 = verbatim
        6. If the number of humanly comprehensible characters is greater than or equal to 500 characters = extract

## ÉTAPE 1 — Classification (analysez le contenu ci-dessous, SANS contexte externe)

### title (max 50 chars)
Specific, descriptive title capturing the core subject of the content.

### subjects (2-3 items)
Concrete topics, concepts, or entities present in the content.
Prefer specific terms over vague ones.
Exclude noise words: {noise_words}

## summary (max 250 chars)
Concise description of what this content contains.

### confidence (0.0-1.0)
Your self-assessment of how well YOU understood this content.
- 1.0 = fully understood, clear and coherent
- 0.7-0.9 = mostly understood, some ambiguity
- 0.4-0.6 = partially understood, fragmented or incomplete
- 0.1-0.3 = barely legible, very noisy
- 0.0 = NOT humanly comprehensible (binary data, encoded content, empty, gibberish)
Confidence measures YOUR comprehension, not content quality or relevance.
A perfectly clear text about any subject = high confidence.

### labels (1-3 items)
Describe WHAT the content covers. Must reflect the subject matter.{label_hint}

## STEP 1B — Semantic explosion

From the topics and labels you produced in Step 1, generate an associative concept cloud.
Include: synonyms, related domains, hypernyms, hyponyms, adjacent concepts.
Single lowercase words only, **in English only**. No duplicates. Do NOT repeat subjects or labels.
Between 5 and 25 items.

## Content to classify ({source_type}: {source_desc}):

{content}


## STEP 2 — Importance scoring

Now that you have classified the content above, assess its importance.
Acquisition source: {source_type}
{context_block}

### importance (0.0-1.0)
- {critical:.1} = critical, must be retained long-term
- {high:.1} = significant, strong retention value
- {normal:.1} = standard, normal retention
- {low:.1} = minor, weak retention
- {disposable:.1} = ephemeral, minimal value


## Output (JSON only, no markdown, no explanation)
If action is "skip": {{"action":"skip"}}
If action is "verbatim" or "extract":
{{"action":"verbatim|extract","title":"...","subjects":["..."],"summary":"...","confidence":0.0,"labels":["..."],"concepts":["..."],"importance":0.0}}"#,
        noise_words = noise_words.join(", "),
        label_hint = label_hint,
        source_type = source.as_str(),
        source_desc = source.description(),
        content = truncated,
        context_block = context_block,
        critical = score_map.critical,
        high = score_map.high,
        normal = score_map.normal,
        low = score_map.low,
        disposable = score_map.disposable,
    )
}

/// Extract the first complete JSON object from a string by tracking brace depth.
/// Handles strings with escaped characters correctly.
/// Returns None if no complete JSON object is found.
fn extract_first_json_object(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    let start = s.find('{')?;
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape_next = false;

    for i in start..bytes.len() {
        let ch = bytes[i];

        if escape_next {
            escape_next = false;
            continue;
        }

        if ch == b'\\' && in_string {
            escape_next = true;
            continue;
        }

        if ch == b'"' {
            in_string = !in_string;
            continue;
        }

        if in_string {
            continue;
        }

        match ch {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[start..=i]);
                }
            }
            _ => {}
        }
    }

    None
}

fn parse_extraction_response(response: &str) -> AiResult<ExtractionResult> {
    // Extract the first complete JSON object by tracking brace depth.
    // Qwen sometimes appends trailing text or extra JSON after the main object,
    // e.g. `{"action":"skip"}{"explanation":"..."}` or `{"action":"skip"} done`.
    // The old `find('{')..rfind('}')` captured everything → "trailing characters" error.
    let json_str = extract_first_json_object(response).ok_or_else(|| {
        crate::AiError::InvalidInput(format!(
            "No JSON object found in LLM response (len={})",
            response.len()
        ))
    })?;

    let value: serde_json::Value = serde_json::from_str(json_str).map_err(|e| {
        crate::AiError::InvalidInput(format!("Failed to parse extraction JSON: {}", e))
    })?;

    // Check action field (skip/verbatim/extract)
    let action = value.get("action").and_then(|v| v.as_str()).unwrap_or("extract");

    if action == "skip" {
        return Ok(ExtractionResult::Skip);
    }

    let extraction_mode = match action {
        "verbatim" => ExtractionMode::Verbatim,
        _ => ExtractionMode::Extract,
    };

    // Parse the extraction fields
    let mut extraction: Extraction = serde_json::from_value(value).map_err(|e| {
        crate::AiError::InvalidInput(format!("Failed to parse extraction fields: {}", e))
    })?;
    extraction.extraction_mode = extraction_mode;

    Ok(ExtractionResult::Extracted(extraction))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extraction_mode_default_is_extract() {
        assert_eq!(ExtractionMode::default(), ExtractionMode::Extract);
    }

    #[test]
    fn test_extraction_mode_serialize_roundtrip() {
        let verbatim = ExtractionMode::Verbatim;
        let json = serde_json::to_string(&verbatim).unwrap();
        assert_eq!(json, "\"Verbatim\"");
        let back: ExtractionMode = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ExtractionMode::Verbatim);
    }

    #[test]
    fn test_parse_skip_action() {
        let json = r#"{"action":"skip"}"#;
        let result = parse_extraction_response(json).unwrap();
        assert!(matches!(result, ExtractionResult::Skip));
    }

    #[test]
    fn test_parse_verbatim_action() {
        let json = r#"{"action":"verbatim","title":"Test","subjects":["rust"],"summary":"A test","confidence":0.9,"labels":["dev"],"concepts":["testing"],"importance":0.7}"#;
        let result = parse_extraction_response(json).unwrap();
        match result {
            ExtractionResult::Extracted(e) => {
                assert_eq!(e.extraction_mode, ExtractionMode::Verbatim);
                assert_eq!(e.title, "Test");
                assert_eq!(e.confidence, 0.9);
            }
            ExtractionResult::Skip => panic!("Expected Extracted, got Skip"),
        }
    }

    #[test]
    fn test_parse_extract_action() {
        let json = r#"{"action":"extract","title":"Summary","subjects":["ai"],"summary":"AI topic","confidence":0.8,"labels":["ml"],"concepts":["neural"],"importance":0.6}"#;
        let result = parse_extraction_response(json).unwrap();
        match result {
            ExtractionResult::Extracted(e) => {
                assert_eq!(e.extraction_mode, ExtractionMode::Extract);
            }
            ExtractionResult::Skip => panic!("Expected Extracted, got Skip"),
        }
    }

    #[test]
    fn test_parse_no_action_field_defaults_to_extract() {
        // Backward compat: old LLM responses without "action" field
        let json = r#"{"title":"Legacy","subjects":["old"],"summary":"Old format","confidence":0.7,"labels":[],"concepts":[],"importance":0.5}"#;
        let result = parse_extraction_response(json).unwrap();
        match result {
            ExtractionResult::Extracted(e) => {
                assert_eq!(e.extraction_mode, ExtractionMode::Extract);
                assert_eq!(e.title, "Legacy");
            }
            ExtractionResult::Skip => panic!("Expected Extracted, got Skip"),
        }
    }

    #[test]
    fn test_parse_invalid_json_returns_error() {
        let json = "not json at all";
        assert!(parse_extraction_response(json).is_err());
    }

    #[test]
    fn test_parse_json_with_surrounding_text() {
        // LLM sometimes wraps JSON in markdown or text
        let response = r#"Here is the result: {"action":"skip"} done"#;
        let result = parse_extraction_response(response).unwrap();
        assert!(matches!(result, ExtractionResult::Skip));
    }

    // --- extract_first_json_object ---

    #[test]
    fn test_extract_first_json_plain() {
        let s = r#"{"action":"skip"}"#;
        assert_eq!(extract_first_json_object(s), Some(r#"{"action":"skip"}"#));
    }

    #[test]
    fn test_extract_first_json_trailing_text() {
        // Qwen bug: trailing text after JSON
        let s = r#"{"action":"skip"} done"#;
        assert_eq!(extract_first_json_object(s), Some(r#"{"action":"skip"}"#));
    }

    #[test]
    fn test_extract_first_json_multiple_objects() {
        // Qwen bug: two JSON objects concatenated
        let s = r#"{"action":"skip"}{"explanation":"not needed"}"#;
        assert_eq!(extract_first_json_object(s), Some(r#"{"action":"skip"}"#));
    }

    #[test]
    fn test_extract_first_json_with_prefix() {
        let s = r#"Here is the result: {"action":"skip"}"#;
        assert_eq!(extract_first_json_object(s), Some(r#"{"action":"skip"}"#));
    }

    #[test]
    fn test_extract_first_json_nested_braces() {
        let s = r#"{"a":{"b":"c"},"d":"e"}"#;
        assert_eq!(extract_first_json_object(s), Some(r#"{"a":{"b":"c"},"d":"e"}"#));
    }

    #[test]
    fn test_extract_first_json_braces_in_strings() {
        // Braces inside JSON string values should not confuse depth tracking
        let s = r#"{"title":"fn() { return }"}"#;
        assert_eq!(extract_first_json_object(s), Some(r#"{"title":"fn() { return }"}"#));
    }

    #[test]
    fn test_extract_first_json_escaped_quotes() {
        let s = r#"{"title":"say \"hello\""}"#;
        assert_eq!(extract_first_json_object(s), Some(r#"{"title":"say \"hello\""}"#));
    }

    #[test]
    fn test_extract_first_json_no_json() {
        assert_eq!(extract_first_json_object("no json here"), None);
    }

    #[test]
    fn test_extract_first_json_incomplete() {
        assert_eq!(extract_first_json_object("{\"action\":\"skip\""), None);
    }

    #[test]
    fn test_parse_skip_with_trailing_json() {
        // Exact Qwen failure case: trailing characters at column 19
        let response = r#"{"action":"skip"}{"reason":"not useful"}"#;
        let result = parse_extraction_response(response).unwrap();
        assert!(matches!(result, ExtractionResult::Skip));
    }

    #[test]
    fn test_parse_extract_with_trailing_text() {
        let response = r#"{"action":"extract","title":"Test","subjects":["a"],"summary":"b","confidence":0.8,"labels":[],"concepts":[],"importance":0.5} I hope this helps!"#;
        let result = parse_extraction_response(response).unwrap();
        match result {
            ExtractionResult::Extracted(e) => assert_eq!(e.title, "Test"),
            ExtractionResult::Skip => panic!("Expected Extracted"),
        }
    }

    #[test]
    fn test_parse_markdown_wrapped_json() {
        let response = "```json\n{\"action\":\"skip\"}\n```";
        let result = parse_extraction_response(response).unwrap();
        assert!(matches!(result, ExtractionResult::Skip));
    }
}
