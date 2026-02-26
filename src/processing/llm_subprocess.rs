//! LLM Provider — local llama.cpp inference only.
//!
//! Used by: extraction, coherence, synthesis, reactivation decisions.
//! Zero API cost. No fallback — if local LLM is unavailable, calls fail cleanly.

use crate::{AiError, AiResult};

/// Call LLM with a prompt and return the response text.
/// Uses local llama.cpp only. No fallback.
pub fn call_llm(prompt: &str) -> AiResult<String> {
    let start = std::time::Instant::now();
    tracing::info!(prompt_len = prompt.len(), "LLM call starting");

    let local = super::local_llm::LocalLlm::global();
    if !local.is_available() {
        return Err(AiError::Provider(
            "Local LLM not available (model not loaded). No fallback configured.".into(),
        ));
    }

    tracing::info!(
        prompt_len = prompt.len(),
        model = %local.model_path().display(),
        "LLM routing → local llama.cpp"
    );
    let result = local.generate(prompt, 512);
    tracing::info!(
        success = result.is_ok(),
        elapsed_ms = start.elapsed().as_millis(),
        "LLM call complete"
    );
    result
}
