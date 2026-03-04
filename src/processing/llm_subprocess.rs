//! LLM Provider — routes inference to Local or Remote backend.
//!
//! Used by: extraction, coherence, synthesis, reactivation decisions.
//! Backend selection via config.json `llm_backend`: Local, Remote, or Auto.
//! Auto mode: try local first, fallback to remote on failure.

use crate::config::{GuardianConfig, LlmBackend};
use crate::{AiError, AiResult};
use std::sync::OnceLock;

/// Cached guardian config for backend routing.
static GUARDIAN_CFG: OnceLock<GuardianConfig> = OnceLock::new();

/// Initialize routing config (called by daemon init).
pub fn init_routing(cfg: &GuardianConfig) {
    let _ = GUARDIAN_CFG.set(cfg.clone());
}

/// Call LLM with a prompt and return the response text.
/// Routes to local or remote based on configured backend.
pub fn call_llm(prompt: &str) -> AiResult<String> {
    let start = std::time::Instant::now();
    let cfg = GUARDIAN_CFG.get_or_init(GuardianConfig::default);

    tracing::info!(
        prompt_len = prompt.len(),
        backend = ?cfg.llm_backend,
        "LLM call starting"
    );

    let result = match cfg.llm_backend {
        LlmBackend::Local => call_local(prompt),
        LlmBackend::Remote => call_remote(prompt, cfg),
        LlmBackend::Auto => {
            match call_local(prompt) {
                Ok(r) => Ok(r),
                Err(local_err) => {
                    tracing::info!(
                        error = %local_err,
                        "Local LLM failed, falling back to remote"
                    );
                    call_remote(prompt, cfg).map_err(|remote_err| {
                        AiError::Provider(format!(
                            "Both local and remote failed. Local: {}. Remote: {}",
                            local_err, remote_err
                        ))
                    })
                }
            }
        }
    };

    tracing::info!(
        backend = ?cfg.llm_backend,
        success = result.is_ok(),
        elapsed_ms = start.elapsed().as_millis(),
        "LLM call complete"
    );
    result
}

fn call_local(prompt: &str) -> AiResult<String> {
    let local = super::local_llm::LocalLlm::global();
    if !local.is_available() {
        return Err(AiError::Provider(
            "Local LLM not available (model not loaded)".into(),
        ));
    }
    tracing::info!(
        prompt_len = prompt.len(),
        model = %local.model_path().display(),
        "LLM routing → local llama.cpp"
    );
    local.generate(prompt, 0)
}

fn call_remote(prompt: &str, cfg: &GuardianConfig) -> AiResult<String> {
    tracing::info!(
        provider = ?cfg.remote_llm.provider,
        model = %cfg.remote_llm.model,
        "LLM routing → remote provider"
    );
    super::remote_llm::generate(prompt, &cfg.remote_llm)
}
