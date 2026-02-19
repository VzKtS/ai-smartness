//! Extraction — LLM-based content extraction.
//!
//! Uses Guardian (claude subprocess) to extract structured metadata from raw content.
//! Fallback: heuristic extraction when LLM unavailable.

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
}

/// Extract structured data from content using LLM subprocess.
/// Falls back to heuristic extraction if LLM call fails.
pub fn extract(content: &str, source: ExtractionSource) -> AiResult<Extraction> {
    // Try LLM extraction first
    match extract_via_llm(content, source) {
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
fn extract_via_llm(content: &str, source: ExtractionSource) -> AiResult<Extraction> {
    let prompt = build_extraction_prompt(content, source);

    match super::llm_subprocess::call_claude(&prompt) {
        Ok(response) => parse_extraction_response(&response),
        Err(e) => {
            tracing::warn!("LLM extraction failed, using heuristics: {}", e);
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

fn build_extraction_prompt(content: &str, source: ExtractionSource) -> String {
    let truncated = if content.len() > 4000 {
        &content[..4000]
    } else {
        content
    };

    format!(
        r#"Extract structured metadata from this {} content. Return JSON only:
{{"title":"<50 chars>","subjects":["topic1","topic2"],"summary":"<200 chars>","confidence":0.0-1.0,"labels":["label1"],"importance":0.0-1.0}}

Content:
{}"#,
        source.as_str(),
        truncated
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
