//! Extraction — LLM-based content extraction.
//!
//! Uses Guardian (claude subprocess) to extract structured metadata from raw content.
//! Config-driven: model, truncation, prompt quality all from GuardianConfig.
//! Fallback: heuristic extraction when LLM unavailable.

use crate::config::{ExtractionConfig, ImportanceRatingConfig, LabelSuggestionConfig};
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

    fn guidance(&self) -> &'static str {
        match self {
            Self::Prompt => "This is a user prompt/message. Focus on the intent and requested action.",
            Self::FileRead => "This is file content that was read. Focus on what the file implements, its purpose, and key structures.",
            Self::FileWrite => "This is file content being written/modified. Focus on what changed and why.",
            Self::Task => "This is a delegated task result. Focus on the outcome and findings.",
            Self::Fetch => "This is fetched web content. Focus on the key information retrieved.",
            Self::Response => "This is an AI response. Focus on decisions made and actions taken.",
            Self::Command => "This is command output (shell/terminal). Focus on the result and any errors or significant output.",
        }
    }
}

/// Extract structured data from content using LLM subprocess.
/// Falls back to heuristic extraction if LLM call fails.
pub fn extract(
    content: &str,
    source: ExtractionSource,
    extraction_cfg: &ExtractionConfig,
    label_cfg: &LabelSuggestionConfig,
    importance_cfg: &ImportanceRatingConfig,
) -> AiResult<Extraction> {
    if !extraction_cfg.llm.enabled {
        let extraction = extract_heuristic(content);
        tracing::info!(mode = "heuristic", title = %extraction.title, "Extraction (LLM disabled)");
        return Ok(extraction);
    }

    match extract_via_llm(content, source, extraction_cfg, label_cfg, importance_cfg) {
        Ok(extraction) => {
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
) -> AiResult<Extraction> {
    let prompt = build_extraction_prompt(content, source, extraction_cfg, label_cfg, importance_cfg);
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
    let title = if clean.len() > 60 {
        format!("{}...", &clean[..57])
    } else {
        clean.clone()
    };

    // Topics from cleaner
    let subjects = super::cleaner::extract_topics(content);

    // Summary: first 200 chars
    let summary = if clean.len() > 200 {
        format!("{}...", &clean[..197])
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
        importance,
    }
}

fn build_extraction_prompt(
    content: &str,
    source: ExtractionSource,
    extraction_cfg: &ExtractionConfig,
    label_cfg: &LabelSuggestionConfig,
    importance_cfg: &ImportanceRatingConfig,
) -> String {
    let max_chars = extraction_cfg.max_content_chars;
    let truncated = if content.len() > max_chars {
        &content[..max_chars]
    } else {
        content
    };

    let label_vocab: Vec<&str> = label_cfg.label_vocabulary.iter().map(|s| s.as_str()).collect();
    let noise_words: Vec<&str> = extraction_cfg.topic_noise_words.iter().map(|s| s.as_str()).collect();
    let score_map = &importance_cfg.score_map;

    format!(
        r#"You are a memory extraction system. Analyze this {source_type} content and return structured metadata as JSON only.

## Source context
{guidance}

## Output format (JSON only, no markdown, no explanation)
{{"title":"...","subjects":["..."],"summary":"...","confidence":0.0-1.0,"labels":["..."],"importance":0.0-1.0}}

## Rules

### Title (max 50 chars)
- Be SPECIFIC and descriptive. Capture the core subject.
- Never start with generic prefixes like "Content:", "File:", "Analysis:", "Code:", "Output:".
- Good: "SQLite bridge storage insert logic", "Gossip cycle embedding similarity phase"
- Bad: "Code analysis", "File content", "Output review"

### Subjects (topics, 2-5 items)
- Extract concrete technical topics, concepts, or entities.
- Exclude noise words: {noise_words}
- Prefer specific terms (e.g. "rusqlite", "TF-IDF cosine") over generic ones ("code", "data").

### Confidence (0.0-1.0)
- Set 0.0 for noise that should NOT become a memory thread:
  build logs, test runner output, binary/encoded content, boilerplate < 3 meaningful phrases,
  repetitive output, dependency lists, lock files, auto-generated content.
- 0.3-0.5: low-value but potentially useful (short exchanges, routine operations).
- 0.6-0.8: substantial content worth remembering (implementations, decisions, debugging).
- 0.9-1.0: critical content (architecture decisions, bug root causes, key insights).

### Labels (from vocabulary)
Choose from: [{label_vocab}]
You may add 1 custom label if none fit.

### Importance (0.0-1.0)
- {critical:.1} = critical (architecture decisions, blockers, breaking changes)
- {high:.1} = high (implementation details, bug fixes, configuration)
- {normal:.1} = normal (exploration, questions, learning)
- {low:.1} = low (chit-chat, meta-discussion, routine)
- {disposable:.1} = disposable (one-off debug, transient logs, ephemeral)

### Summary (max 200 chars)
Concise description of what this content contains and why it matters.

## Content ({source_type}):
{content}"#,
        source_type = source.as_str(),
        guidance = source.guidance(),
        noise_words = noise_words.join(", "),
        label_vocab = label_vocab.join(", "),
        critical = score_map.critical,
        high = score_map.high,
        normal = score_map.normal,
        low = score_map.low,
        disposable = score_map.disposable,
        content = truncated,
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
