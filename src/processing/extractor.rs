//! Extraction — LLM-based content extraction.
//!
//! Uses Guardian (claude subprocess) to extract structured metadata from raw content.
//! Config-driven: model, truncation, prompt quality all from GuardianConfig.
//! Fallback: heuristic extraction when LLM unavailable.

use crate::config::{ExtractionConfig, ImportanceRatingConfig, LabelSuggestionConfig};
use crate::constants::truncate_safe;
use crate::AiResult;
use serde::{Deserialize, Serialize};

/// Extraction result from LLM or heuristics.
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

/// Extract structured data from content using LLM subprocess.
/// Falls back to heuristic extraction if LLM call fails.
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
) -> AiResult<Extraction> {
    if !extraction_cfg.llm.enabled {
        let extraction = extract_heuristic(content);
        tracing::info!(mode = "heuristic", title = %extraction.title, "Extraction (LLM disabled)");
        return Ok(extraction);
    }

    match extract_via_llm(content, source, extraction_cfg, label_cfg, importance_cfg, agent_context) {
        Ok(mut extraction) => {
            // Prompt and Response: override summary with verbatim content (truncated).
            // These are already human-readable — LLM synthesis only degrades them.
            if matches!(source, ExtractionSource::Prompt | ExtractionSource::Response) {
                extraction.summary = truncate_safe(content, crate::constants::VERBATIM_SUMMARY_LIMIT).to_string();
            }
            tracing::info!(mode = "llm", title = %extraction.title, confidence = extraction.confidence, "Extraction complete");
            Ok(extraction)
        }
        Err(_) => {
            let extraction = extract_heuristic(content);
            tracing::info!(mode = "heuristic", title = %extraction.title, confidence = extraction.confidence, "Extraction complete (fallback)");
            Ok(extraction)
        }
    }
}

/// LLM-based extraction via claude subprocess.
fn extract_via_llm(
    content: &str,
    source: ExtractionSource,
    extraction_cfg: &ExtractionConfig,
    label_cfg: &LabelSuggestionConfig,
    importance_cfg: &ImportanceRatingConfig,
    agent_context: Option<&str>,
) -> AiResult<Extraction> {
    let prompt = build_extraction_prompt(content, source, extraction_cfg, label_cfg, importance_cfg, agent_context);
    let model = extraction_cfg.llm.model.as_cli_flag();

    match super::llm_subprocess::call_claude_with_model(&prompt, model) {
        Ok(response) => parse_extraction_response(&response),
        Err(e) => {
            tracing::warn!(model = %model, "LLM extraction failed: {}", e);
            Err(e)
        }
    }
}

/// Heuristic fallback extraction — no LLM needed.
fn extract_heuristic(content: &str) -> Extraction {
    tracing::debug!(content_len = content.len(), "Heuristic extraction");
    let clean = super::cleaner::clean_text(content);
    let words: Vec<&str> = clean.split_whitespace().collect();

    // Title: first ~60 chars
    let title = if clean.chars().count() > 60 {
        let t: String = clean.chars().take(57).collect();
        format!("{}...", t)
    } else {
        clean.clone()
    };

    // Topics from cleaner
    let subjects = super::cleaner::extract_topics(content);

    // Summary: first 200 chars
    let summary = if clean.chars().count() > 200 {
        let s: String = clean.chars().take(197).collect();
        format!("{}...", s)
    } else {
        clean.clone()
    };

    // Simple importance heuristic
    let importance = if words.len() > 100 {
        0.6
    } else if words.len() > 30 {
        0.5
    } else {
        0.4
    };

    Extraction {
        title,
        subjects,
        summary,
        confidence: 0.3,
        labels: vec![],
        concepts: vec![],
        importance,
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
{{"title":"...","subjects":["..."],"summary":"...","confidence":0.0,"labels":["..."],"concepts":["..."],"importance":0.0}}"#,
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

fn parse_extraction_response(response: &str) -> AiResult<Extraction> {
    // Try to find JSON in response
    let json_str = if let Some(start) = response.find('{') {
        if let Some(end) = response.rfind('}') {
            &response[start..=end]
        } else {
            response
        }
    } else {
        response
    };

    serde_json::from_str(json_str).map_err(|e| {
        crate::AiError::InvalidInput(format!("Failed to parse extraction: {}", e))
    })
}
