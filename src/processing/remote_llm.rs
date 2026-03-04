//! Remote LLM — API-based inference via external providers.
//!
//! Supports: Anthropic (Messages API), OpenAI (Chat Completions), Custom (OpenAI-compatible).
//! API keys resolved from environment variables only (no secrets in config).

use crate::config::{RemoteLlmConfig, RemoteProvider};
use crate::{AiError, AiResult};
use std::time::Duration;

/// Create a ureq agent with the given timeout.
fn make_agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .build()
        .new_agent()
}

/// Generate text via remote LLM provider.
pub fn generate(prompt: &str, config: &RemoteLlmConfig) -> AiResult<String> {
    let api_key = resolve_api_key(&config.provider)?;
    let timeout = Duration::from_secs(config.timeout_secs);

    tracing::info!(
        provider = ?config.provider,
        model = %config.model,
        prompt_len = prompt.len(),
        "Remote LLM call"
    );

    let start = std::time::Instant::now();
    let result = match &config.provider {
        RemoteProvider::Anthropic => {
            call_anthropic(prompt, &api_key, &config.model, config.max_tokens, timeout)
        }
        RemoteProvider::OpenAI => {
            call_openai(prompt, &api_key, &config.model, config.max_tokens, timeout, None)
        }
        RemoteProvider::Custom { url } => {
            call_openai(prompt, &api_key, &config.model, config.max_tokens, timeout, Some(url))
        }
    };

    match &result {
        Ok(text) => tracing::info!(
            elapsed_ms = start.elapsed().as_millis(),
            output_len = text.len(),
            "Remote LLM call succeeded"
        ),
        Err(e) => tracing::warn!(
            elapsed_ms = start.elapsed().as_millis(),
            error = %e,
            "Remote LLM call failed"
        ),
    }

    result
}

/// Resolve API key from environment variable.
fn resolve_api_key(provider: &RemoteProvider) -> AiResult<String> {
    let var_name = match provider {
        RemoteProvider::Anthropic => "ANTHROPIC_API_KEY",
        RemoteProvider::OpenAI => "OPENAI_API_KEY",
        RemoteProvider::Custom { .. } => "AI_SMARTNESS_LLM_API_KEY",
    };
    std::env::var(var_name).map_err(|_| {
        AiError::Provider(format!(
            "Missing env var {} for remote LLM provider. Set it to use {:?} backend.",
            var_name, provider
        ))
    })
}

/// Anthropic Messages API (api.anthropic.com/v1/messages).
fn call_anthropic(
    prompt: &str,
    api_key: &str,
    model: &str,
    max_tokens: u32,
    timeout: Duration,
) -> AiResult<String> {
    let agent = make_agent(timeout);
    let body = serde_json::json!({
        "model": model,
        "max_tokens": max_tokens,
        "messages": [{"role": "user", "content": prompt}]
    });

    let mut resp = agent
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .send_json(&body)
        .map_err(|e| AiError::Provider(format!("Anthropic API error: {}", e)))?;

    let json: serde_json::Value = resp
        .body_mut()
        .read_json()
        .map_err(|e| AiError::Provider(format!("Anthropic response parse error: {}", e)))?;

    json["content"][0]["text"]
        .as_str()
        .map(|s: &str| s.to_string())
        .ok_or_else(|| AiError::Provider("Anthropic: no text in response".into()))
}

/// OpenAI Chat Completions API (also used for Custom OpenAI-compatible endpoints).
fn call_openai(
    prompt: &str,
    api_key: &str,
    model: &str,
    max_tokens: u32,
    timeout: Duration,
    custom_url: Option<&str>,
) -> AiResult<String> {
    let agent = make_agent(timeout);
    let url = custom_url
        .map(|u| format!("{}/chat/completions", u.trim_end_matches('/')))
        .unwrap_or_else(|| "https://api.openai.com/v1/chat/completions".into());

    let body = serde_json::json!({
        "model": model,
        "max_tokens": max_tokens,
        "temperature": 0.1,
        "messages": [{"role": "user", "content": prompt}]
    });

    let mut resp = agent
        .post(&url)
        .header("Authorization", &format!("Bearer {}", api_key))
        .header("content-type", "application/json")
        .send_json(&body)
        .map_err(|e| AiError::Provider(format!("OpenAI API error: {}", e)))?;

    let json: serde_json::Value = resp
        .body_mut()
        .read_json()
        .map_err(|e| AiError::Provider(format!("OpenAI response parse error: {}", e)))?;

    json["choices"][0]["message"]["content"]
        .as_str()
        .map(|s: &str| s.to_string())
        .ok_or_else(|| AiError::Provider("OpenAI: no content in response".into()))
}
