//! LLM Provider — routes to local llama.cpp or Claude CLI fallback.
//!
//! Used by: extraction, coherence, synthesis, reactivation decisions.
//! Priority: Local LLM (zero cost) → Claude CLI (API cost fallback).

use crate::{AiError, AiResult};
use std::process::Command;

/// Call LLM with a prompt and return the response text.
/// Routes to local llama.cpp first, falls back to Claude CLI.
pub fn call_claude(prompt: &str) -> AiResult<String> {
    // Try local LLM first (zero API cost)
    let local = super::local_llm::LocalLlm::global();
    if local.is_available() {
        tracing::debug!(prompt_len = prompt.len(), "LLM call via local llama.cpp");
        return local.generate(prompt, 512);
    }

    // Fallback: Claude CLI subprocess (haiku)
    tracing::info!(prompt_len = prompt.len(), "Local LLM unavailable, using Claude CLI fallback");
    execute_claude(prompt)
}

fn execute_claude(prompt: &str) -> AiResult<String> {
    let child = Command::new("claude")
        .args(["--model", "haiku", "-p", prompt])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| {
            AiError::Provider(format!(
                "Failed to spawn claude subprocess: {}. Is `claude` CLI installed?",
                e
            ))
        })?;

    let output = child
        .wait_with_output()
        .map_err(|e| AiError::Provider(format!("Claude subprocess wait failed: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AiError::Provider(format!(
            "Claude subprocess failed (exit {}): {}",
            output.status, stderr
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    if stdout.trim().is_empty() {
        return Err(AiError::Provider("Claude returned empty response".into()));
    }

    Ok(stdout)
}
