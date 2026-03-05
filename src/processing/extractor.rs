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
    /// LLM summarized technical content (code/logs/paths)
    Summary,
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
    #[serde(default)]
    pub summary: String,
    #[serde(default = "default_truncated_score", deserialize_with = "deserialize_score_lenient")]
    pub confidence: f64,
    pub labels: Vec<String>,
    #[serde(default)]
    pub concepts: Vec<String>,
    #[serde(default = "default_truncated_score", deserialize_with = "deserialize_score_lenient")]
    pub importance: f64,
    /// How the LLM processed this content.
    #[serde(default)]
    pub extraction_mode: ExtractionMode,
    /// True if extraction came from truncated LLM output (missing fields).
    /// Used by Engram v10 for weighted scoring of partial extractions.
    #[serde(default)]
    pub from_partial: bool,
}

/// Default score for truncated LLM output (missing confidence/importance fields).
/// 0.0 = pipeline will drop at confidence gate — better than polluting engram with fake scores.
fn default_truncated_score() -> f64 {
    0.0
}

/// Lenient deserializer for score fields (confidence, importance).
/// Accepts f64, i64, u64, and strings. Non-numeric strings → 0.0.
/// Prevents serde crash when LLM returns placeholders like "[Insert confidence value here]".
fn deserialize_score_lenient<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct ScoreVisitor;
    impl<'de> serde::de::Visitor<'de> for ScoreVisitor {
        type Value = f64;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a number or string parseable as f64")
        }
        fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<f64, E> {
            Ok(v)
        }
        fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<f64, E> {
            Ok(v as f64)
        }
        fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<f64, E> {
            Ok(v as f64)
        }
        fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<f64, E> {
            Ok(v.parse::<f64>().unwrap_or(0.0))
        }
    }
    deserializer.deserialize_any(ScoreVisitor)
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

    // Retry loop: up to 3 attempts (2 retries max) on degenerate extraction
    for attempt in 0..3u8 {
        match extract_via_llm(content, source, extraction_cfg, label_cfg, importance_cfg, agent_context) {
            Ok(ExtractionResult::Skip) => {
                tracing::info!(mode = "llm_skip", "Extraction: LLM decided to skip");
                return Ok(None);
            }
            Ok(ExtractionResult::Extracted(mut extraction)) => {
                // Gate: detect degenerate extraction (LLM returned placeholders like "...").
                if is_degenerate_extraction(&extraction) {
                    if attempt < 2 {
                        tracing::warn!(
                            attempt = attempt + 1,
                            title = %extraction.title,
                            summary = %extraction.summary,
                            "Extraction: degenerate output detected, retrying"
                        );
                        continue;
                    }
                    tracing::warn!(
                        title = %extraction.title,
                        summary = %extraction.summary,
                        "Extraction: degenerate output after 3 attempts — dropping"
                    );
                    return Ok(None);
                }
                // Prompt and Response: use verbatim fallback ONLY if LLM didn't produce a summary.
                if matches!(source, ExtractionSource::Prompt | ExtractionSource::Response)
                    && extraction.summary.trim().is_empty()
                {
                    extraction.summary = truncate_safe(content, crate::constants::VERBATIM_SUMMARY_LIMIT).to_string();
                }
                tracing::info!(mode = "llm", title = %extraction.title, confidence = extraction.confidence, "Extraction complete");
                return Ok(Some(extraction));
            }
            Err(e) => {
                if attempt < 2 {
                    tracing::warn!(
                        attempt = attempt + 1,
                        error = %e,
                        "Extraction: LLM/parse failed, retrying"
                    );
                    continue;
                }
                tracing::warn!("Extraction failed after 3 attempts: {} — dropping capture", e);
                return Ok(None);
            }
        }
    }
    Ok(None)
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
    let start = std::time::Instant::now();
    let prompt = build_extraction_prompt(content, source, extraction_cfg, label_cfg, importance_cfg, agent_context);
    tracing::info!(
        prompt_len = prompt.len(),
        content_len = content.len(),
        source = source.as_str(),
        max_content_chars = extraction_cfg.max_content_chars,
        "Extraction: prompt built, calling LLM"
    );

    // Retry loop: up to 3 attempts (2 retries max) on LLM transport failure
    let mut last_err = None;
    for attempt in 0..3u8 {
        match super::llm_subprocess::call_llm(&prompt) {
            Ok(response) => {
                tracing::info!(
                    response_len = response.len(),
                    attempt = attempt + 1,
                    elapsed_ms = start.elapsed().as_millis(),
                    "Extraction: LLM response received"
                );
                tracing::debug!(raw_response = %response, "Extraction: LLM raw output");
                let parse_result = parse_extraction_response(&response);
                match &parse_result {
                    Ok(ExtractionResult::Skip) => {
                        tracing::info!(elapsed_ms = start.elapsed().as_millis(), "Extraction: parsed → Skip");
                    }
                    Ok(ExtractionResult::Extracted(e)) => {
                        tracing::info!(
                            title = %e.title,
                            confidence = e.confidence,
                            action = ?e.extraction_mode,
                            elapsed_ms = start.elapsed().as_millis(),
                            "Extraction: parsed → Extracted"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            response_preview = %&response[..response.len().min(200)],
                            "Extraction: JSON parse failed"
                        );
                    }
                }
                return parse_result;
            }
            Err(e) => {
                if attempt < 2 {
                    tracing::warn!(
                        attempt = attempt + 1,
                        error = %e,
                        elapsed_ms = start.elapsed().as_millis(),
                        "Extraction: LLM call failed, retrying"
                    );
                } else {
                    tracing::warn!(
                        error = %e,
                        elapsed_ms = start.elapsed().as_millis(),
                        "Extraction: LLM call failed after 3 attempts"
                    );
                }
                last_err = Some(e);
            }
        }
    }
    Err(last_err.unwrap())
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
        r#"Your role is to process the content provided and follow these rules:
        
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
IMPORTANT: prefer single generic words over multi-word phrases.
Good: "rust", "memory", "config", "daemon", "gui", "bridge", "extraction", "testing".
Bad: "database connection pooling", "software development lifecycle".
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
{{"action":"verbatim|extract","title":"...","confidence":0.0,"importance":0.0,"subjects":["..."],"labels":["..."],"concepts":["..."],"summary":"..."}}"#,
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

/// Repair truncated JSON from LLM output that hit max_tokens.
/// Two strategies: (1) close unclosed string + close stack, (2) cut at last
/// complete value boundary + close stack. Returns None if repair fails.
fn repair_truncated_json(raw: &str) -> Option<String> {
    let start = raw.find('{')?;
    let json_part = &raw[start..];
    let json_bytes = json_part.as_bytes();

    let mut in_string = false;
    let mut escape_next = false;
    let mut stack: Vec<u8> = Vec::new(); // expected closers: b'}' or b']'
    let mut last_value_end = 0; // byte offset after last complete key:value or array element

    // Track whether we're after a ':' (next complete token is a value)
    let mut after_colon = false;

    for i in 0..json_bytes.len() {
        let ch = json_bytes[i];

        if escape_next {
            escape_next = false;
            continue;
        }

        if in_string {
            if ch == b'\\' {
                escape_next = true;
            } else if ch == b'"' {
                in_string = false;
                // A closed string after a colon = completed value
                // A closed string inside an array = completed element
                if after_colon || stack.last() == Some(&b']') {
                    last_value_end = i + 1;
                    after_colon = false;
                }
            }
            continue;
        }

        match ch {
            b'"' => {
                in_string = true;
            }
            b'{' => {
                stack.push(b'}');
                after_colon = false;
            }
            b'[' => {
                stack.push(b']');
                after_colon = false;
            }
            b'}' | b']' => {
                stack.pop();
                last_value_end = i + 1;
                after_colon = false;
                if stack.is_empty() {
                    return Some(json_part[..=i].to_string());
                }
            }
            b':' => {
                after_colon = true;
            }
            b',' => {
                after_colon = false;
            }
            b'0'..=b'9' | b'.' | b'-' => {
                if after_colon || stack.last() == Some(&b']') {
                    last_value_end = i + 1;
                }
            }
            // true/false/null
            b't' | b'f' | b'n' => {
                if after_colon || stack.last() == Some(&b']') {
                    last_value_end = i + 1;
                }
            }
            _ => {}
        }
    }

    if stack.is_empty() {
        return None;
    }

    // Strategy 1: If inside an unclosed string, close it + close stack
    if in_string {
        let mut attempt = json_part.to_string();
        attempt.push('"');
        for closer in stack.iter().rev() {
            attempt.push(*closer as char);
        }
        if serde_json::from_str::<serde_json::Value>(&attempt).is_ok() {
            return Some(attempt);
        }
    }

    // Strategy 2: Cut at last complete value boundary + close stack
    if last_value_end == 0 {
        return None;
    }

    let mut result = json_part[..last_value_end].to_string();

    // Strip trailing commas and whitespace
    while result.ends_with(|c: char| c == ',' || c.is_ascii_whitespace()) {
        result.pop();
    }

    // Close remaining open brackets/braces
    for closer in stack.iter().rev() {
        result.push(*closer as char);
    }

    // Verify it actually parses
    if serde_json::from_str::<serde_json::Value>(&result).is_err() {
        return None;
    }

    // Ratio guard: reject repair if >66% of JSON content was lost.
    // A 3435→251 repair is a false success — syntactically valid but semantically empty.
    if result.len() < json_part.len() / 3 {
        tracing::warn!(
            repaired_len = result.len(),
            original_len = json_part.len(),
            "JSON repair rejected: too much content lost (ratio < 0.33)"
        );
        return None;
    }

    Some(result)
}

/// Parse LLM extraction response JSON into ExtractionResult.
/// Public for reuse by toolextractor.rs (same JSON format + repair logic).
pub fn parse_tool_extraction_response(response: &str) -> AiResult<ExtractionResult> {
    parse_extraction_response(response)
}

/// Detect degenerate LLM extraction (placeholder values, empty metadata).
/// Returns true if the extraction is garbage and should be dropped.
///
/// Two detection modes:
/// 1. Both title AND summary are placeholders ("...", empty, etc.)
/// 2. Both subjects AND labels are empty — the LLM may have echoed a doc comment
///    as title but failed to extract any real metadata from the content.
pub fn is_degenerate_extraction(ext: &Extraction) -> bool {
    let both_placeholder = is_placeholder(&ext.title) && is_placeholder(&ext.summary);
    let no_metadata = ext.subjects.is_empty() && ext.labels.is_empty();
    both_placeholder || no_metadata
}

/// Check if a string is a placeholder (LLM couldn't produce real content).
/// Public for reuse in daemon quality scan (periodic_tasks.rs).
pub fn is_placeholder(s: &str) -> bool {
    let trimmed = s.trim();
    if trimmed.is_empty() || trimmed.len() < 3 {
        return true;
    }
    // Common LLM placeholder patterns
    let placeholders = ["...", "…", "n/a", "N/A", "none", "None", "null", "undefined"];
    placeholders.iter().any(|p| trimmed == *p)
}

fn parse_extraction_response(response: &str) -> AiResult<ExtractionResult> {
    // Extract the first complete JSON object by tracking brace depth.
    // Qwen sometimes appends trailing text or extra JSON after the main object,
    // e.g. `{"action":"skip"}{"explanation":"..."}` or `{"action":"skip"} done`.
    // The old `find('{')..rfind('}')` captured everything → "trailing characters" error.
    let json_str_opt = extract_first_json_object(response);

    // If no complete JSON found, try repair (LLM hit max_tokens → truncated JSON).
    let repaired_owned: Option<String>;
    let json_str = match json_str_opt {
        Some(s) => s,
        None => {
            repaired_owned = repair_truncated_json(response);
            match &repaired_owned {
                Some(repaired) => {
                    tracing::info!(
                        repaired_len = repaired.len(),
                        original_len = response.len(),
                        "JSON repair succeeded — using repaired output"
                    );
                    repaired.as_str()
                }
                None => {
                    return Err(crate::AiError::InvalidInput(format!(
                        "No JSON object found in LLM response (len={})",
                        response.len()
                    )));
                }
            }
        }
    };

    let value: serde_json::Value = serde_json::from_str(json_str).map_err(|e| {
        crate::AiError::InvalidInput(format!("Failed to parse extraction JSON: {}", e))
    })?;

    // Check action field (skip/verbatim/extract)
    let action = value.get("action").and_then(|v| v.as_str()).unwrap_or("extract");

    if action == "skip" {
        return Ok(ExtractionResult::Skip);
    }

    let extraction_mode = match action {
        "summary" | "verbatim" => ExtractionMode::Summary,
        _ => ExtractionMode::Extract,
    };

    // Parse the extraction fields — all required, no defaults
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
        let summary = ExtractionMode::Summary;
        let json = serde_json::to_string(&summary).unwrap();
        assert_eq!(json, "\"Summary\"");
        let back: ExtractionMode = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ExtractionMode::Summary);
    }

    #[test]
    fn test_parse_skip_action() {
        let json = r#"{"action":"skip"}"#;
        let result = parse_extraction_response(json).unwrap();
        assert!(matches!(result, ExtractionResult::Skip));
    }

    #[test]
    fn test_parse_summary_action() {
        let json = r#"{"action":"summary","title":"Test","subjects":["rust"],"summary":"A test","confidence":0.9,"labels":["dev"],"concepts":["testing"],"importance":0.7}"#;
        let result = parse_extraction_response(json).unwrap();
        match result {
            ExtractionResult::Extracted(e) => {
                assert_eq!(e.extraction_mode, ExtractionMode::Summary);
                assert_eq!(e.title, "Test");
                assert_eq!(e.confidence, 0.9);
            }
            ExtractionResult::Skip => panic!("Expected Extracted, got Skip"),
        }
    }

    #[test]
    fn test_parse_verbatim_action_maps_to_summary() {
        let json = r#"{"action":"verbatim","title":"Logs","subjects":["daemon"],"summary":"Daemon logs","confidence":0.5,"labels":["ops"],"concepts":["logging"],"importance":0.4}"#;
        let result = parse_extraction_response(json).unwrap();
        match result {
            ExtractionResult::Extracted(e) => {
                assert_eq!(e.extraction_mode, ExtractionMode::Summary);
                assert_eq!(e.title, "Logs");
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

    // --- repair_truncated_json ---

    #[test]
    fn test_repair_truncated_array_element() {
        // Simulates LLM hitting max_tokens mid-array
        let truncated = r#"{"action":"extract","title":"Test","subjects":["a","b"],"confidence":0.8,"importance":0.7,"labels":["x"],"concepts":["foo","bar","incomple"#;
        let repaired = repair_truncated_json(truncated).unwrap();
        let v: serde_json::Value = serde_json::from_str(&repaired).unwrap();
        assert_eq!(v["title"], "Test");
        assert_eq!(v["confidence"], 0.8);
        // concepts should have "foo" and "bar" (incomplete "incomple" dropped)
        let concepts = v["concepts"].as_array().unwrap();
        assert!(concepts.len() >= 2);
    }

    #[test]
    fn test_repair_truncated_mid_string() {
        // Truncated in the middle of a string value
        let truncated = r#"{"action":"extract","title":"Test","subjects":["a"],"confidence":0.8,"importance":0.7,"labels":["x"],"summary":"This is a long summa"#;
        let repaired = repair_truncated_json(truncated).unwrap();
        let v: serde_json::Value = serde_json::from_str(&repaired).unwrap();
        assert_eq!(v["title"], "Test");
        // summary should be included (string was closed by repair)
        assert!(v["summary"].as_str().unwrap().starts_with("This is a long summa"));
    }

    #[test]
    fn test_repair_no_json_returns_none() {
        assert!(repair_truncated_json("no json here at all").is_none());
    }

    #[test]
    fn test_repair_complete_json_returns_it() {
        let complete = r#"{"action":"skip"}"#;
        let repaired = repair_truncated_json(complete).unwrap();
        assert_eq!(repaired, complete);
    }

    #[test]
    fn test_repair_too_little_json() {
        assert!(repair_truncated_json("{").is_none());
    }

    #[test]
    fn test_parse_uses_repair_fallback() {
        // Truncated JSON (no closing brace) — parse_extraction_response should repair it
        let truncated = r#"{"action":"extract","title":"Repaired","subjects":["test"],"confidence":0.9,"importance":0.8,"labels":["dev"],"concepts":["repa"#;
        let result = parse_extraction_response(truncated).unwrap();
        match result {
            ExtractionResult::Extracted(e) => {
                assert_eq!(e.title, "Repaired");
                assert_eq!(e.confidence, 0.9);
            }
            ExtractionResult::Skip => panic!("Expected Extracted"),
        }
    }

    // --- is_degenerate_extraction / is_placeholder ---

    #[test]
    fn test_degenerate_both_placeholder() {
        let ext = Extraction {
            title: "...".into(),
            summary: "...".into(),
            subjects: vec!["...".into()],
            labels: vec!["...".into()],
            concepts: vec![],
            confidence: 0.3,
            importance: 0.3,
            extraction_mode: ExtractionMode::Summary,
            from_partial: true,
        };
        assert!(is_degenerate_extraction(&ext));
    }

    #[test]
    fn test_degenerate_empty_title_and_summary() {
        let ext = Extraction {
            title: "".into(),
            summary: "  ".into(),
            subjects: vec![],
            labels: vec![],
            concepts: vec![],
            confidence: 0.5,
            importance: 0.5,
            extraction_mode: ExtractionMode::Extract,
            from_partial: false,
        };
        assert!(is_degenerate_extraction(&ext));
    }

    #[test]
    fn test_not_degenerate_good_title_with_metadata() {
        let ext = Extraction {
            title: "Daemon IPC client implementation".into(),
            summary: "...".into(),
            subjects: vec!["IPC".into()],
            labels: vec!["architecture".into()],
            concepts: vec![],
            confidence: 0.8,
            importance: 0.7,
            extraction_mode: ExtractionMode::Extract,
            from_partial: false,
        };
        // title is good + has metadata — not degenerate
        assert!(!is_degenerate_extraction(&ext));
    }

    #[test]
    fn test_not_degenerate_good_summary_with_metadata() {
        let ext = Extraction {
            title: "...".into(),
            summary: "Implements IPC communication for daemon captures".into(),
            subjects: vec!["daemon".into()],
            labels: vec![],
            concepts: vec![],
            confidence: 0.8,
            importance: 0.7,
            extraction_mode: ExtractionMode::Extract,
            from_partial: false,
        };
        // summary is good + has subjects — not degenerate
        assert!(!is_degenerate_extraction(&ext));
    }

    #[test]
    fn test_degenerate_real_title_but_no_metadata() {
        // LLM echoed doc comment as title but couldn't extract any real metadata
        let ext = Extraction {
            title: "Thread Manager -- thread lifecycle management.".into(),
            summary: "Handles: NewThread / Continue / Fork / Reactivate decisions.".into(),
            subjects: vec![],
            labels: vec![],
            concepts: vec![],
            confidence: 0.3,
            importance: 0.3,
            extraction_mode: ExtractionMode::Summary,
            from_partial: true,
        };
        assert!(is_degenerate_extraction(&ext));
    }

    #[test]
    fn test_not_degenerate_placeholder_title_but_has_labels() {
        // title is "..." but labels exist — partial success, keep it
        let ext = Extraction {
            title: "...".into(),
            summary: "Some summary".into(),
            subjects: vec![],
            labels: vec!["config".into()],
            concepts: vec![],
            confidence: 0.5,
            importance: 0.5,
            extraction_mode: ExtractionMode::Extract,
            from_partial: false,
        };
        assert!(!is_degenerate_extraction(&ext));
    }

    #[test]
    fn test_placeholder_patterns() {
        assert!(is_placeholder("..."));
        assert!(is_placeholder("…"));
        assert!(is_placeholder("n/a"));
        assert!(is_placeholder("N/A"));
        assert!(is_placeholder("none"));
        assert!(is_placeholder("null"));
        assert!(is_placeholder(""));
        assert!(is_placeholder("  "));
        assert!(is_placeholder("ab")); // < 3 chars
        assert!(!is_placeholder("abc")); // 3 chars, not a placeholder
        assert!(!is_placeholder("Real title here"));
    }

    // --- deserialize_score_lenient (F2) ---

    #[test]
    fn test_deserialize_score_f64() {
        let json = r#"{"title":"T","subjects":[],"confidence":0.8,"labels":[],"importance":0.5}"#;
        let ext: Extraction = serde_json::from_str(json).unwrap();
        assert!((ext.confidence - 0.8).abs() < f64::EPSILON);
    }

    #[test]
    fn test_deserialize_score_string_numeric() {
        let json = r#"{"title":"T","subjects":[],"confidence":"0.8","labels":[],"importance":"0.5"}"#;
        let ext: Extraction = serde_json::from_str(json).unwrap();
        assert!((ext.confidence - 0.8).abs() < f64::EPSILON);
        assert!((ext.importance - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_deserialize_score_string_placeholder() {
        let json = r#"{"title":"T","subjects":[],"confidence":"[Insert confidence value here]","labels":[],"importance":"TBD"}"#;
        let ext: Extraction = serde_json::from_str(json).unwrap();
        assert!((ext.confidence - 0.0).abs() < f64::EPSILON);
        assert!((ext.importance - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_deserialize_score_integer() {
        let json = r#"{"title":"T","subjects":[],"confidence":1,"labels":[],"importance":0}"#;
        let ext: Extraction = serde_json::from_str(json).unwrap();
        assert!((ext.confidence - 1.0).abs() < f64::EPSILON);
        assert!((ext.importance - 0.0).abs() < f64::EPSILON);
    }

    // --- ratio guard (F1) ---

    #[test]
    fn test_repair_ratio_guard_rejects_aggressive_cut() {
        // Craft input where Strategy 2 cuts aggressively:
        // - in_string=false at end (so Strategy 1 is skipped)
        // - last_value_end is early (only first field completed)
        // - json_part is long (500+ chars of whitespace padding)
        //
        // {"a":"b", followed by 500 spaces → not in a string, stack=['}']
        // last_value_end ≈ 8 (after "b"). json_part.len() ≈ 510.
        // Strategy 2 result = {"a":"b"} ≈ 9 chars → 9 < 510/3 = 170 → REJECTED.
        let mut input = String::from(r#"{"a":"b","#);
        input.push_str(&" ".repeat(500));
        let result = repair_truncated_json(&input);
        assert!(result.is_none(), "Ratio guard should reject repair that loses >66% content");
    }

    #[test]
    fn test_repair_ratio_guard_accepts_minor_cut() {
        // JSON where only a small tail is truncated — repair keeps most content
        let json = r#"{"action":"extract","title":"Good Title","subjects":["rust","memory"],"confidence":0.8,"importance":0.7,"labels":["dev","architecture"],"concepts":["config","thread","engram","serde","parse"#;
        let result = repair_truncated_json(json);
        assert!(result.is_some(), "Ratio guard should accept repair that keeps >33% content");
        let repaired = result.unwrap();
        let v: serde_json::Value = serde_json::from_str(&repaired).unwrap();
        assert_eq!(v["title"], "Good Title");
    }
}
