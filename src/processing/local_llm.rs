//! Local LLM — in-process inference via llama.cpp (zero API cost).
//!
//! Handles Guardian tasks: extraction, coherence, reactivation, merge evaluation.
//! Hardware-agnostic: adapts GPU layers and context size to available VRAM.
//!
//! Model: GGUF format, auto-downloaded to {data_dir}/models/ on first use.
//! Singleton pattern (OnceLock) — same as EmbeddingManager.

use crate::config::{DeviceSelection, LocalLlmConfig, LocalModelSize};
use crate::{AiError, AiResult};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::{LlamaModelParams, LlamaSplitMode};
use llama_cpp_2::model::{AddBos, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;

use super::vram_probe;

// ============================================================================
// Constants (no hardware-specific values)
// ============================================================================

/// Fallback context size when auto-detection is not possible.
const FALLBACK_CTX_SIZE: u32 = 2048;

/// Default max output tokens for generation.
const FALLBACK_MAX_TOKENS: u32 = 768;

/// Temperature for sampling (low = more deterministic, better for JSON).
const SAMPLING_TEMP: f32 = 0.1;

/// Context sizes to try in descending order during fallback cascade.
const CTX_CASCADE: &[u32] = &[4096, 2048, 1024, 512];

/// Safety margin for VRAM allocation (MB).
/// Accounts for Vulkan overhead, fragmentation, desktop compositors.
const VRAM_SAFETY_MARGIN_MB: u64 = 300;

/// Maximum consecutive failures before circuit breaker opens.
const CIRCUIT_BREAKER_THRESHOLD: u32 = 5;

/// Cooldown duration when circuit breaker is open.
const CIRCUIT_BREAKER_COOLDOWN: std::time::Duration = std::time::Duration::from_secs(300);

// ============================================================================
// Resource Profile
// ============================================================================

/// Resolved hardware parameters for the current session.
#[derive(Debug, Clone)]
struct LlmResourceProfile {
    gpu_layers: u32,
    ctx_size: u32,
    max_tokens: u32,
    inference_threads: i32,
    source: ProfileSource,
}

#[derive(Debug, Clone, PartialEq)]
enum ProfileSource {
    Auto,
    UserOverride,
    Degraded,
}

// ============================================================================
// Circuit Breaker
// ============================================================================

#[derive(Debug)]
struct LlmCircuitBreaker {
    consecutive_failures: u32,
    state: CircuitState,
}

#[derive(Debug, Clone, PartialEq)]
enum CircuitState {
    Closed,
    Open { cooldown_until: std::time::Instant },
    HalfOpen,
}

impl LlmCircuitBreaker {
    fn new() -> Self {
        Self {
            consecutive_failures: 0,
            state: CircuitState::Closed,
        }
    }

    fn allow_call(&mut self) -> bool {
        match &self.state {
            CircuitState::Closed => true,
            CircuitState::Open { cooldown_until } => {
                if std::time::Instant::now() >= *cooldown_until {
                    self.state = CircuitState::HalfOpen;
                    tracing::info!("LLM circuit breaker: cooldown expired, entering half-open (probe)");
                    true
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => true,
        }
    }

    fn record_success(&mut self) {
        if self.state != CircuitState::Closed {
            tracing::info!("LLM circuit breaker: success — closing circuit");
        }
        self.consecutive_failures = 0;
        self.state = CircuitState::Closed;
    }

    fn record_failure(&mut self) {
        self.consecutive_failures += 1;
        if self.consecutive_failures >= CIRCUIT_BREAKER_THRESHOLD {
            let cooldown_until = std::time::Instant::now() + CIRCUIT_BREAKER_COOLDOWN;
            tracing::warn!(
                failures = self.consecutive_failures,
                cooldown_secs = CIRCUIT_BREAKER_COOLDOWN.as_secs(),
                "LLM circuit breaker: OPEN — too many consecutive failures"
            );
            self.state = CircuitState::Open { cooldown_until };
        }
    }

    fn status(&self) -> &'static str {
        match &self.state {
            CircuitState::Closed => "available",
            CircuitState::Open { .. } => "cooldown",
            CircuitState::HalfOpen => "degraded",
        }
    }
}

// ============================================================================
// LocalLlm
// ============================================================================

static GLOBAL: OnceLock<LocalLlm> = OnceLock::new();

/// Local LLM engine — wraps llama.cpp for in-process inference.
pub struct LocalLlm {
    /// Backend + model held together. None = unavailable.
    inner: Option<LlmInner>,
    model_path: PathBuf,
    /// Which model variant is loaded (needed for chat template wrapping).
    model_size: LocalModelSize,
    /// Persistent GPU context — reused across generate() calls via kv_cache_clear().
    /// Mutex serializes access — one inference at a time.
    persistent_ctx: Mutex<Option<LlamaContext<'static>>>,
    /// Resolved hardware profile (gpu_layers, ctx_size, etc.)
    profile: Mutex<LlmResourceProfile>,
    /// Circuit breaker for consecutive failure protection.
    circuit_breaker: Mutex<LlmCircuitBreaker>,
}

struct LlmInner {
    backend: LlamaBackend,
    model: LlamaModel,
}

// Safety: LlamaBackend and LlamaModel are thread-safe for read operations.
// LlamaContext is guarded by Mutex (exclusive access during generate()).
// The 'static lifetime on LlamaContext is sound: model lives in 'static OnceLock.
unsafe impl Send for LocalLlm {}
unsafe impl Sync for LocalLlm {}
unsafe impl Send for LlmInner {}
unsafe impl Sync for LlmInner {}

impl LocalLlm {
    /// Global singleton (initialized once on first access).
    pub fn global() -> &'static Self {
        GLOBAL.get_or_init(|| Self::new(None, LocalLlmConfig::default(), DeviceSelection::Auto))
    }

    /// Initialize with explicit model size, config, and device selection. Called by daemon.
    pub fn init_with_size(
        size: &LocalModelSize,
        config: &LocalLlmConfig,
        runtime_device: &DeviceSelection,
    ) -> &'static Self {
        GLOBAL.get_or_init(|| Self::new(Some(size.clone()), config.clone(), runtime_device.clone()))
    }

    /// Initialize: probe VRAM, find/download model, load with adaptive GPU layers.
    pub fn new(
        model_size: Option<LocalModelSize>,
        llm_config: LocalLlmConfig,
        runtime_device: DeviceSelection,
    ) -> Self {
        let size = model_size.unwrap_or_default();
        let model_dir = crate::storage::path_utils::data_dir().join("models");
        let model_path = model_dir.join(size.filename());

        let profile = Self::compute_resource_profile(&size, &llm_config, &runtime_device);
        tracing::info!(
            gpu_layers = profile.gpu_layers,
            ctx_size = profile.ctx_size,
            max_tokens = profile.max_tokens,
            threads = profile.inference_threads,
            source = ?profile.source,
            "Resource profile computed"
        );

        let unavailable = |model_path: PathBuf, size: LocalModelSize, profile: LlmResourceProfile| Self {
            inner: None,
            model_path,
            model_size: size,
            persistent_ctx: Mutex::new(None),
            profile: Mutex::new(profile),
            circuit_breaker: Mutex::new(LlmCircuitBreaker::new()),
        };

        // Initialize llama.cpp backend
        let backend = match LlamaBackend::init() {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("Failed to init llama backend: {:?}, local LLM unavailable", e);
                return unavailable(model_path, size, profile);
            }
        };

        // Try to find model file
        if !model_path.exists() {
            tracing::info!("Local LLM model not found at {}, attempting download...", model_path.display());
            if let Err(e) = Self::download_model(&model_dir, &model_path, size.download_url()) {
                tracing::warn!("Failed to download model: {}, local LLM unavailable", e);
                return unavailable(model_path, size, profile);
            }
        }

        // Load model with adaptive GPU layers — cascade: full → half → CPU-only
        let target_layers = profile.gpu_layers;
        let (model, actual_layers) = match Self::load_model_cascade(&backend, &model_path, target_layers, &runtime_device) {
            Some(result) => result,
            None => {
                tracing::warn!("All model load attempts failed, local LLM unavailable");
                return unavailable(model_path, size, profile);
            }
        };

        // Update profile with actual layers (may differ from computed)
        let mut final_profile = profile;
        if actual_layers != target_layers {
            final_profile.gpu_layers = actual_layers;
            final_profile.source = ProfileSource::Degraded;
        }

        tracing::info!(
            model = %model_path.display(),
            params = model.n_params(),
            size = %size.display_name(),
            gpu_layers = actual_layers,
            ctx_size = final_profile.ctx_size,
            "Local LLM loaded successfully"
        );

        Self {
            inner: Some(LlmInner { backend, model }),
            model_path,
            model_size: size,
            persistent_ctx: Mutex::new(None),
            profile: Mutex::new(final_profile),
            circuit_breaker: Mutex::new(LlmCircuitBreaker::new()),
        }
    }

    /// Whether local LLM is ready for inference.
    pub fn is_available(&self) -> bool {
        self.inner.is_some()
    }

    /// Current LLM operational status (for BeatState).
    pub fn status(&self) -> &'static str {
        if self.inner.is_none() {
            return "unavailable";
        }
        let cb = self.circuit_breaker.lock().unwrap_or_else(|e| e.into_inner());
        cb.status()
    }

    /// Current resolved context size.
    pub fn current_ctx_size(&self) -> u32 {
        self.profile.lock().map(|p| p.ctx_size).unwrap_or(0)
    }

    /// Current GPU layer count.
    pub fn current_gpu_layers(&self) -> u32 {
        self.profile.lock().map(|p| p.gpu_layers).unwrap_or(0)
    }

    /// Get the model file path.
    pub fn model_path(&self) -> &PathBuf {
        &self.model_path
    }

    // ========================================================================
    // Adaptive Resource Profile
    // ========================================================================

    /// Compute optimal resource profile based on available hardware.
    fn compute_resource_profile(
        model_size: &LocalModelSize,
        config: &LocalLlmConfig,
        runtime_device: &DeviceSelection,
    ) -> LlmResourceProfile {
        let inference_threads = if config.inference_threads != 0 {
            config.inference_threads as i32
        } else {
            Self::auto_threads()
        };

        // User overrides take precedence
        if config.gpu_layers != 0 || config.ctx_size != 0 {
            return LlmResourceProfile {
                gpu_layers: if config.gpu_layers != 0 { config.gpu_layers } else { 99 },
                ctx_size: if config.ctx_size != 0 { config.ctx_size } else { FALLBACK_CTX_SIZE },
                max_tokens: if config.max_tokens != 0 { config.max_tokens } else { FALLBACK_MAX_TOKENS },
                inference_threads,
                source: ProfileSource::UserOverride,
            };
        }

        // CpuOnly: skip VRAM probe entirely
        if matches!(runtime_device, DeviceSelection::CpuOnly) {
            tracing::info!("runtime_device=cpu — forcing CPU-only mode");
            return LlmResourceProfile {
                gpu_layers: 0,
                ctx_size: FALLBACK_CTX_SIZE,
                max_tokens: if config.max_tokens != 0 { config.max_tokens } else { FALLBACK_MAX_TOKENS },
                inference_threads,
                source: ProfileSource::UserOverride,
            };
        }

        // Auto-detect based on VRAM probe (target specific GPU if configured)
        let vram = match runtime_device {
            DeviceSelection::Gpu(idx) => {
                tracing::info!(gpu_index = idx, "runtime_device=gpu:{} — probing specific GPU", idx);
                vram_probe::probe_vram_for_gpu(*idx)
            }
            _ => vram_probe::probe_vram(),
        };

        match vram {
            None => {
                tracing::info!("No GPU detected — using CPU-only mode (0 gpu_layers)");
                LlmResourceProfile {
                    gpu_layers: 0,
                    ctx_size: FALLBACK_CTX_SIZE,
                    max_tokens: if config.max_tokens != 0 { config.max_tokens } else { FALLBACK_MAX_TOKENS },
                    inference_threads,
                    source: ProfileSource::Auto,
                }
            }
            Some(info) => {
                let free = info.free_mb();
                let model_vram = model_size.model_vram_mb();
                let kv_per_token = model_size.kv_bytes_per_token();

                tracing::info!(
                    gpu_total_mb = info.total_mb,
                    gpu_used_mb = info.used_mb,
                    gpu_free_mb = free,
                    model_vram_mb = model_vram,
                    "VRAM probe result"
                );

                // Step 1: Determine gpu_layers
                let gpu_layers = if free >= model_vram + VRAM_SAFETY_MARGIN_MB {
                    99 // full offload
                } else if free >= model_vram / 2 + VRAM_SAFETY_MARGIN_MB {
                    // Partial offload: proportional to available VRAM
                    let ratio = (free.saturating_sub(VRAM_SAFETY_MARGIN_MB)) as f64 / model_vram as f64;
                    (ratio * 40.0).min(40.0).max(1.0) as u32
                } else {
                    0 // not enough for meaningful GPU offload
                };

                // Step 2: Determine ctx_size from remaining VRAM after model
                let vram_for_kv = if gpu_layers > 0 {
                    let model_actual = if gpu_layers >= 99 {
                        model_vram
                    } else {
                        model_vram * gpu_layers as u64 / 40
                    };
                    free.saturating_sub(model_actual).saturating_sub(VRAM_SAFETY_MARGIN_MB)
                } else {
                    u64::MAX // CPU-only: KV cache in system RAM
                };

                let vram_for_kv_bytes = vram_for_kv.saturating_mul(1024 * 1024);
                let mut ctx_size = FALLBACK_CTX_SIZE;
                for &candidate in CTX_CASCADE {
                    let kv_needed = candidate as u64 * kv_per_token;
                    if vram_for_kv_bytes >= kv_needed || gpu_layers == 0 {
                        ctx_size = candidate;
                        break;
                    }
                }

                // CPU-only: use 2048 (RAM is abundant, but inference is slower)
                if gpu_layers == 0 {
                    ctx_size = FALLBACK_CTX_SIZE;
                }

                tracing::info!(gpu_layers, ctx_size, "Adaptive resource profile computed");

                LlmResourceProfile {
                    gpu_layers,
                    ctx_size,
                    max_tokens: if config.max_tokens != 0 { config.max_tokens } else { FALLBACK_MAX_TOKENS },
                    inference_threads,
                    source: ProfileSource::Auto,
                }
            }
        }
    }

    fn auto_threads() -> i32 {
        let cpus = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        (cpus / 2).max(1).min(4) as i32
    }

    // ========================================================================
    // Model Loading (cascading fallback)
    // ========================================================================

    /// Try loading model with target layers, then half, then CPU-only.
    fn load_model_cascade(
        backend: &LlamaBackend,
        model_path: &PathBuf,
        target_layers: u32,
        runtime_device: &DeviceSelection,
    ) -> Option<(LlamaModel, u32)> {
        // Apply GPU device selection to model params
        let apply_device = |params: LlamaModelParams| -> LlamaModelParams {
            if let DeviceSelection::Gpu(idx) = runtime_device {
                tracing::info!(main_gpu = idx, "Pinning model to GPU {}", idx);
                params
                    .with_split_mode(LlamaSplitMode::None)
                    .with_main_gpu(*idx as i32)
            } else {
                params
            }
        };

        // Attempt 1: target layers
        let params = apply_device(LlamaModelParams::default().with_n_gpu_layers(target_layers));
        match LlamaModel::load_from_file(backend, model_path, &params) {
            Ok(m) => return Some((m, target_layers)),
            Err(e) => tracing::warn!(gpu_layers = target_layers, error = ?e, "Model load failed"),
        }

        // Attempt 2: half layers
        if target_layers > 1 {
            let half = target_layers / 2;
            let params = apply_device(LlamaModelParams::default().with_n_gpu_layers(half));
            match LlamaModel::load_from_file(backend, model_path, &params) {
                Ok(m) => return Some((m, half)),
                Err(e) => tracing::warn!(gpu_layers = half, error = ?e, "Partial offload failed"),
            }
        }

        // Attempt 3: CPU-only (no GPU pinning needed)
        if target_layers > 0 {
            tracing::info!("Trying CPU-only model load (0 gpu_layers)");
            let params = LlamaModelParams::default().with_n_gpu_layers(0);
            match LlamaModel::load_from_file(backend, model_path, &params) {
                Ok(m) => return Some((m, 0)),
                Err(e) => tracing::warn!(error = ?e, "CPU-only model load also failed"),
            }
        }

        None
    }

    // ========================================================================
    // Context Creation (cascading fallback)
    // ========================================================================

    /// Try to create a context, falling back to smaller sizes on failure.
    fn try_create_context_cascade<'a>(
        &self,
        inner: &'a LlmInner,
        initial_ctx_size: u32,
        threads: i32,
    ) -> AiResult<LlamaContext<'a>> {
        let start_idx = CTX_CASCADE.iter().position(|&c| c <= initial_ctx_size).unwrap_or(0);
        let candidates = &CTX_CASCADE[start_idx..];

        for (i, &ctx_size) in candidates.iter().enumerate() {
            let params = LlamaContextParams::default()
                .with_n_ctx(std::num::NonZeroU32::new(ctx_size))
                .with_n_batch(ctx_size)
                .with_n_threads(threads)
                .with_n_threads_batch(threads);

            tracing::info!(ctx_size, attempt = i + 1, "Attempting context creation");

            match inner.model.new_context(&inner.backend, params) {
                Ok(ctx) => {
                    if ctx_size < initial_ctx_size {
                        tracing::warn!(
                            original = initial_ctx_size,
                            actual = ctx_size,
                            "Context created at reduced size (VRAM pressure)"
                        );
                        if let Ok(mut p) = self.profile.lock() {
                            p.ctx_size = ctx_size;
                            p.source = ProfileSource::Degraded;
                        }
                    }
                    return Ok(ctx);
                }
                Err(e) => {
                    tracing::warn!(ctx_size, error = ?e, "Context creation failed");
                    if i < candidates.len() - 1 {
                        std::thread::sleep(std::time::Duration::from_millis(500));
                    }
                }
            }
        }

        Err(AiError::Provider(
            "Failed to create LLM context at all sizes (512-4096). VRAM exhausted.".into(),
        ))
    }

    // ========================================================================
    // Generate (with circuit breaker)
    // ========================================================================

    /// Generate a response from a prompt.
    ///
    /// Reuses a persistent GPU context (kv_cache_clear between calls).
    /// Thread-safe: Mutex serializes access — one inference at a time.
    /// Circuit breaker: refuses calls after too many consecutive failures.
    pub fn generate(&self, prompt: &str, max_tokens: u32) -> AiResult<String> {
        // Circuit breaker check
        {
            let mut cb = self.circuit_breaker.lock().unwrap_or_else(|e| e.into_inner());
            if !cb.allow_call() {
                return Err(AiError::Provider(
                    "LLM circuit breaker open: too many consecutive failures. In cooldown.".into(),
                ));
            }
        }

        let result = self.generate_inner(prompt, max_tokens);

        match &result {
            Ok(_) => {
                let mut cb = self.circuit_breaker.lock().unwrap_or_else(|e| e.into_inner());
                cb.record_success();
            }
            Err(e) => {
                if Self::is_gpu_error(e) {
                    tracing::warn!(error = %e, "GPU error — dropping persistent context for recovery");
                    if let Ok(mut ctx_guard) = self.persistent_ctx.lock() {
                        *ctx_guard = None;
                    }
                }
                let mut cb = self.circuit_breaker.lock().unwrap_or_else(|e| e.into_inner());
                cb.record_failure();
            }
        }

        result
    }

    /// Check if an error is GPU-related (context creation, decode, batch).
    fn is_gpu_error(e: &AiError) -> bool {
        match e {
            AiError::Provider(msg) => {
                msg.contains("context") || msg.contains("decode")
                    || msg.contains("NullReturn") || msg.contains("batch")
                    || msg.contains("Batch") || msg.contains("Decode")
                    || msg.contains("VRAM exhausted")
            }
            _ => false,
        }
    }

    /// Inner generate — the actual inference pipeline.
    fn generate_inner(&self, prompt: &str, max_tokens: u32) -> AiResult<String> {
        let gen_start = std::time::Instant::now();

        // Read profile for this call
        let (ctx_size, prof_max_tokens, threads) = {
            let p = self.profile.lock().unwrap_or_else(|e| e.into_inner());
            (p.ctx_size, p.max_tokens, p.inference_threads)
        };

        // Wrap raw prompt in model-specific chat template
        let wrapped = self.model_size.wrap_chat_template(prompt);
        tracing::info!(
            raw_prompt_len = prompt.len(),
            wrapped_prompt_len = wrapped.len(),
            max_tokens = max_tokens,
            ctx_size = ctx_size,
            model = %self.model_path.display(),
            template = %match self.model_size {
                LocalModelSize::ThreeB | LocalModelSize::SevenB => "chatml",
                LocalModelSize::Phi4Mini => "phi4",
            },
            "Local LLM generate() called"
        );

        let inner = self.inner.as_ref().ok_or_else(|| {
            AiError::Provider("Local LLM model not loaded".into())
        })?;

        // Serialize all inference via persistent context lock
        let mut ctx_guard = self.persistent_ctx.lock().unwrap_or_else(|e| e.into_inner());

        let max_tokens = if max_tokens == 0 { prof_max_tokens } else { max_tokens };

        // Lazy-create persistent context on first call, reuse on subsequent calls.
        if ctx_guard.is_none() {
            let ctx = self.try_create_context_cascade(inner, ctx_size, threads)?;
            // Safety: model lives in 'static OnceLock (GLOBAL) — it is never dropped,
            // so the context's borrow of model is valid for 'static lifetime.
            let ctx: LlamaContext<'static> = unsafe { std::mem::transmute(ctx) };
            *ctx_guard = Some(ctx);
        } else {
            // Reuse existing context — clear KV cache to reset state.
            ctx_guard.as_mut().unwrap().clear_kv_cache();
            tracing::debug!("KV cache cleared — reusing persistent GPU context");
        }

        let ctx = ctx_guard.as_mut().unwrap();
        tracing::debug!(elapsed_ms = gen_start.elapsed().as_millis(), "LLM context ready");

        // Read actual ctx_size from profile (may have been degraded by cascade)
        let actual_ctx_size = self.profile.lock().map(|p| p.ctx_size).unwrap_or(ctx_size);

        // Tokenize prompt (using chat-template-wrapped version)
        let tokens = inner.model.str_to_token(&wrapped, AddBos::Always)
            .map_err(|e| AiError::Provider(format!("Tokenization failed: {:?}", e)))?;
        tracing::info!(prompt_tokens = tokens.len(), elapsed_ms = gen_start.elapsed().as_millis(), "Tokenization complete");

        // Reserve space for generation (prompt + max_tokens must fit in context)
        let max_input_tokens = actual_ctx_size.saturating_sub(max_tokens) as usize;
        let mut tokens = tokens;
        if tokens.len() >= max_input_tokens {
            tracing::warn!(
                prompt_tokens = tokens.len(),
                max_input = max_input_tokens,
                truncated_to = max_input_tokens,
                "Prompt exceeds context — truncating tokens to fit"
            );
            tokens.truncate(max_input_tokens);
        }

        // Create batch and add prompt tokens
        let mut batch = LlamaBatch::new(actual_ctx_size as usize, 1);
        let last_idx = tokens.len() - 1;
        for (i, &token) in tokens.iter().enumerate() {
            batch.add(token, i as i32, &[0], i == last_idx)
                .map_err(|e| AiError::Provider(format!("Batch add failed: {:?}", e)))?;
        }
        tracing::info!(
            n_tokens = tokens.len(),
            ctx_size = actual_ctx_size,
            "Batch prepared — entering llama.cpp decode"
        );
        use std::io::Write;
        let _ = std::io::stderr().flush();

        // Decode prompt (process all input tokens)
        let decode_start = std::time::Instant::now();
        ctx.decode(&mut batch)
            .map_err(|e| AiError::Provider(format!("Prompt decode failed: {:?}", e)))?;
        tracing::info!(
            decode_ms = decode_start.elapsed().as_millis(),
            elapsed_ms = gen_start.elapsed().as_millis(),
            "Prompt decode complete — starting generation"
        );

        // Setup sampler (low temperature + greedy for JSON reliability)
        let mut sampler = LlamaSampler::chain_simple([
            LlamaSampler::temp(SAMPLING_TEMP),
            LlamaSampler::greedy(),
        ]);

        // Generate output tokens
        let mut output = String::new();
        let mut n_pos = tokens.len() as i32;
        let eos = inner.model.token_eos();
        let mut piece_decoder = encoding_rs::UTF_8.new_decoder();
        let sample_start = std::time::Instant::now();

        // Track JSON brace depth for early stop
        let mut json_depth: i32 = 0;
        let mut json_started = false;

        for i in 0..max_tokens {
            let token = sampler.sample(ctx, -1);

            if token == eos {
                tracing::debug!(tokens_generated = i, "EOS token reached");
                break;
            }

            match inner.model.token_to_piece(token, &mut piece_decoder, false, None) {
                Ok(piece) => {
                    output.push_str(&piece);

                    // JSON early-stop: track brace depth
                    for ch in piece.chars() {
                        if ch == '{' {
                            json_depth += 1;
                            json_started = true;
                        } else if ch == '}' {
                            json_depth -= 1;
                            if json_started && json_depth <= 0 {
                                const REQUIRED_KEYS: &[&str] = &[
                                    "\"title\"", "\"confidence\"", "\"importance\"",
                                    "\"subjects\"", "\"labels\"", "\"summary\"",
                                ];
                                let missing: Vec<&&str> = REQUIRED_KEYS.iter()
                                    .filter(|k| !output.contains(**k))
                                    .collect();
                                if !missing.is_empty() {
                                    tracing::info!(
                                        missing = ?missing,
                                        tokens_generated = i + 1,
                                        "JSON closed but missing fields — skipping early-stop"
                                    );
                                    json_depth = 0;
                                    json_started = false;
                                    continue;
                                }
                                tracing::info!(tokens_generated = i + 1, "JSON complete — early stop");
                                if let Some(pos) = output.rfind('}') {
                                    output.truncate(pos + 1);
                                }
                                let total_generated = i as usize + 1;
                                tracing::info!(
                                    prompt_tokens = tokens.len(),
                                    output_tokens = total_generated,
                                    output_len = output.len(),
                                    total_ms = gen_start.elapsed().as_millis(),
                                    sample_ms = sample_start.elapsed().as_millis(),
                                    tokens_per_sec = if sample_start.elapsed().as_millis() > 0 {
                                        (total_generated as u128 * 1000) / sample_start.elapsed().as_millis()
                                    } else { 0 },
                                    "Local LLM generation complete (JSON early-stop)"
                                );
                                let preview_end = safe_preview_end(&output, 500);
                                tracing::info!(output_preview = %&output[..preview_end], "LLM raw output");
                                return Ok(output);
                            }
                        }
                    }
                }
                Err(_e) => {
                    tracing::info!(token_idx = i, "End-of-turn token — stopping generation");
                    break;
                }
            }

            if (i + 1) % 50 == 0 {
                tracing::debug!(
                    tokens_generated = i + 1,
                    output_len = output.len(),
                    elapsed_ms = sample_start.elapsed().as_millis(),
                    "Generation progress"
                );
            }

            // Prepare next iteration
            batch.clear();
            batch.add(token, n_pos, &[0], true)
                .map_err(|e| AiError::Provider(format!("Batch add failed: {:?}", e)))?;

            ctx.decode(&mut batch)
                .map_err(|e| AiError::Provider(format!("Decode failed: {:?}", e)))?;

            n_pos += 1;
        }

        let total_generated = n_pos as usize - tokens.len();

        if output.trim().is_empty() {
            tracing::warn!(elapsed_ms = gen_start.elapsed().as_millis(), "Local LLM returned empty response");
            return Err(AiError::Provider("Local LLM returned empty response".into()));
        }

        tracing::info!(
            prompt_tokens = tokens.len(),
            output_tokens = total_generated,
            output_len = output.len(),
            total_ms = gen_start.elapsed().as_millis(),
            sample_ms = sample_start.elapsed().as_millis(),
            tokens_per_sec = if sample_start.elapsed().as_millis() > 0 {
                (total_generated as u128 * 1000) / sample_start.elapsed().as_millis()
            } else { 0 },
            "Local LLM generation complete"
        );
        let preview_end = safe_preview_end(&output, 500);
        tracing::info!(output_preview = %&output[..preview_end], "LLM raw output");

        Ok(output)
    }

    /// Download a model from HuggingFace.
    fn download_model(model_dir: &PathBuf, dest: &PathBuf, url: &str) -> AiResult<()> {
        std::fs::create_dir_all(model_dir).map_err(|e| {
            AiError::Storage(format!("Failed to create models dir: {}", e))
        })?;

        tracing::info!(url = url, "Downloading local LLM model...");

        let status = std::process::Command::new("curl")
            .args(["-fSL", "--progress-bar", "-o"])
            .arg(dest.as_os_str())
            .arg(url)
            .status()
            .map_err(|e| AiError::Provider(format!("curl failed: {}", e)))?;

        if !status.success() {
            let _ = std::fs::remove_file(dest);
            return Err(AiError::Provider(format!(
                "Model download failed (curl exit {})",
                status.code().unwrap_or(-1)
            )));
        }

        if dest.exists() {
            let size = std::fs::metadata(dest)
                .map(|m| m.len())
                .unwrap_or(0);
            tracing::info!(path = %dest.display(), size_mb = size / 1_000_000, "Model downloaded");
            Ok(())
        } else {
            Err(AiError::Provider("Download completed but file not found".into()))
        }
    }
}

/// Safe char-boundary truncation for preview (avoid panic on multi-byte UTF-8).
fn safe_preview_end(s: &str, max: usize) -> usize {
    let max = s.len().min(max);
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    end
}
