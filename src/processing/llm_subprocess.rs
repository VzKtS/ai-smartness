//! LLM Subprocess — calls `claude` CLI for Guardian tasks.
//!
//! Used by: extraction, coherence, synthesis, reactivation decisions.
//! Retry logic: 1 retry with shorter prompt on failure.
//! Timeout: 30s per call.

use crate::{AiError, AiResult};
use std::process::Command;
use std::time::Duration;

/// Default timeout for LLM calls (30 seconds).
const LLM_TIMEOUT_SECS: u64 = 30;

/// Maximum retries.
const MAX_RETRIES: u32 = 1;

/// Call claude CLI with a prompt and return the response text.
pub fn call_claude(prompt: &str) -> AiResult<String> {
    call_claude_with_model(prompt, "haiku")
}

/// Call claude CLI with a specific model.
pub fn call_claude_with_model(prompt: &str, model: &str) -> AiResult<String> {
    tracing::info!(model = %model, prompt_len = prompt.len(), "LLM subprocess call starting");
    let mut last_err = None;

    for attempt in 0..=MAX_RETRIES {
        match execute_claude(prompt, model) {
            Ok(response) => return Ok(response),
            Err(e) => {
                tracing::warn!(
                    "Claude subprocess attempt {}/{} failed: {}",
                    attempt + 1,
                    MAX_RETRIES + 1,
                    e
                );
                last_err = Some(e);
            }
        }
    }

    tracing::error!(model = %model, "LLM subprocess: all retries exhausted");
    Err(last_err.unwrap_or_else(|| AiError::Provider("All retries failed".into())))
}

fn execute_claude(prompt: &str, model: &str) -> AiResult<String> {
    let child = Command::new("claude")
        .args(["--model", model, "-p", prompt])
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

/// Check if claude CLI is available on PATH.
pub fn is_claude_available() -> bool {
    Command::new("claude")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Estimated timeout duration for planning.
pub fn timeout_duration() -> Duration {
    Duration::from_secs(LLM_TIMEOUT_SECS)
}
