//! Local LLM — in-process inference via llama.cpp (zero API cost).
//!
//! Handles all Guardian tasks: extraction, coherence, reactivation,
//! merge evaluation, etc. No fallback — local only.
//!
//! Model: Qwen2.5-Instruct GGUF, auto-downloaded to {data_dir}/models/ on first use.
//! Sizes: 3B (default, ~2.1GB) or 7B (~4.7GB), selectable via config.
//! Singleton pattern (OnceLock) — same as EmbeddingManager.

use crate::config::LocalModelSize;
use crate::{AiError, AiResult};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;

/// Context size (tokens). 4096 keeps KV cache under ~384 MB on Phi-4-mini GQA,
/// total VRAM ~2.9 GB — safe margin on GTX 1650 (4 GB) even with Vulkan overhead.
/// Prompts exceeding (ctx - max_tokens) are truncated automatically.
const DEFAULT_CTX_SIZE: u32 = 4096;

/// Default max output tokens for generation.
/// 768 gives enough room for full extraction JSON (title, subjects, labels,
/// concepts, summary) while keeping latency reasonable (~25s on GTX 1650).
const DEFAULT_MAX_TOKENS: u32 = 768;

/// Temperature for sampling (low = more deterministic, better for JSON).
const SAMPLING_TEMP: f32 = 0.1;

/// Number of threads for llama.cpp context evaluation.
/// Explicit to avoid over-subscription in multi-threaded daemon.
const INFERENCE_THREADS: i32 = 2;

static GLOBAL: OnceLock<LocalLlm> = OnceLock::new();

/// Local LLM engine — wraps llama.cpp for in-process inference.
pub struct LocalLlm {
    /// Backend + model held together. None = unavailable.
    inner: Option<LlmInner>,
    model_path: PathBuf,
    /// Which model variant is loaded (needed for chat template wrapping).
    model_size: LocalModelSize,
    /// Persistent GPU context — reused across generate() calls via kv_cache_clear().
    /// Prevents Vulkan VRAM dealloc/realloc race condition on consecutive calls
    /// (GTX 1650: second new_context() hangs when previous VRAM not yet freed).
    /// Mutex serializes access — one inference at a time.
    persistent_ctx: Mutex<Option<LlamaContext<'static>>>,
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
        GLOBAL.get_or_init(|| Self::new(None))
    }

    /// Initialize with explicit model size. Called by daemon with config.
    pub fn init_with_size(size: &LocalModelSize) -> &'static Self {
        GLOBAL.get_or_init(|| Self::new(Some(size.clone())))
    }

    /// Initialize: find or download GGUF model, load into memory.
    pub fn new(model_size: Option<LocalModelSize>) -> Self {
        let size = model_size.unwrap_or_default();
        let model_dir = crate::storage::path_utils::data_dir().join("models");
        let model_path = model_dir.join(size.filename());

        let unavailable = |model_path: PathBuf, size: LocalModelSize| Self {
            inner: None,
            model_path,
            model_size: size,
            persistent_ctx: Mutex::new(None),
        };

        // Initialize llama.cpp backend
        let backend = match LlamaBackend::init() {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("Failed to init llama backend: {:?}, local LLM unavailable", e);
                return unavailable(model_path, size);
            }
        };

        // Try to find model file
        if !model_path.exists() {
            tracing::info!("Local LLM model not found at {}, attempting download...", model_path.display());
            if let Err(e) = Self::download_model(&model_dir, &model_path, size.download_url()) {
                tracing::warn!("Failed to download model: {}, local LLM unavailable", e);
                return unavailable(model_path, size);
            }
        }

        // Load model — offload layers to GPU (Vulkan).
        // GTX 1650 has 4GB VRAM. 3B fits fully (99 layers), 7B needs partial offload.
        // Try full offload first; if it fails, retry with partial (28 layers).
        let gpu_layers: u32 = 99;
        let model_params = LlamaModelParams::default()
            .with_n_gpu_layers(gpu_layers);
        let (model, actual_gpu_layers) = match LlamaModel::load_from_file(&backend, &model_path, &model_params) {
            Ok(m) => (m, gpu_layers),
            Err(e) => {
                // Full offload failed (likely VRAM overflow) — retry with partial
                let partial = 28;
                tracing::warn!(
                    gpu_layers = gpu_layers,
                    error = ?e,
                    "Full GPU offload failed, retrying with {} layers", partial
                );
                let partial_params = LlamaModelParams::default()
                    .with_n_gpu_layers(partial);
                match LlamaModel::load_from_file(&backend, &model_path, &partial_params) {
                    Ok(m) => (m, partial),
                    Err(e2) => {
                        tracing::warn!("Failed to load model {}: {:?}", model_path.display(), e2);
                        return unavailable(model_path, size);
                    }
                }
            }
        };
        tracing::info!(
            model = %model_path.display(),
            params = model.n_params(),
            size = %size.display_name(),
            gpu_layers = actual_gpu_layers,
            "Local LLM loaded successfully (Vulkan GPU offload)"
        );
        Self {
            inner: Some(LlmInner { backend, model }),
            model_path,
            model_size: size,
            persistent_ctx: Mutex::new(None),
        }
    }

    /// Whether local LLM is ready for inference.
    pub fn is_available(&self) -> bool {
        self.inner.is_some()
    }

    /// Generate a response from a prompt.
    ///
    /// Reuses a persistent GPU context (kv_cache_clear between calls).
    /// Thread-safe: Mutex serializes access — one inference at a time.
    pub fn generate(&self, prompt: &str, max_tokens: u32) -> AiResult<String> {
        let gen_start = std::time::Instant::now();

        // Wrap raw prompt in model-specific chat template
        let wrapped = self.model_size.wrap_chat_template(prompt);
        tracing::info!(
            raw_prompt_len = prompt.len(),
            wrapped_prompt_len = wrapped.len(),
            max_tokens = max_tokens,
            model = %self.model_path.display(),
            template = %match self.model_size {
                LocalModelSize::ThreeB | LocalModelSize::SevenB => "chatml",
                LocalModelSize::Phi4Mini => "phi4",
            },
            "Local LLM generate() called (chat template applied)"
        );

        let inner = self.inner.as_ref().ok_or_else(|| {
            AiError::Provider("Local LLM model not loaded".into())
        })?;

        // Serialize all inference via persistent context lock
        let mut ctx_guard = self.persistent_ctx.lock().unwrap_or_else(|e| e.into_inner());

        let max_tokens = if max_tokens == 0 { DEFAULT_MAX_TOKENS } else { max_tokens };

        // Lazy-create persistent context on first call, reuse on subsequent calls.
        // This eliminates Vulkan VRAM dealloc/realloc between calls — the KV cache
        // memory stays allocated, only its contents are cleared.
        if ctx_guard.is_none() {
            let ctx_params = LlamaContextParams::default()
                .with_n_ctx(std::num::NonZeroU32::new(DEFAULT_CTX_SIZE))
                .with_n_batch(DEFAULT_CTX_SIZE)
                .with_n_threads(INFERENCE_THREADS)
                .with_n_threads_batch(INFERENCE_THREADS);
            tracing::info!(
                ctx_size = DEFAULT_CTX_SIZE,
                n_batch = DEFAULT_CTX_SIZE,
                n_threads = INFERENCE_THREADS,
                "Creating persistent LLM context (Vulkan VRAM allocated once)"
            );
            let ctx = inner.model.new_context(&inner.backend, ctx_params)
                .map_err(|e| AiError::Provider(format!("Failed to create LLM context: {:?}", e)))?;
            // Safety: model lives in 'static OnceLock (GLOBAL) — it is never dropped,
            // so the context's borrow of model is valid for 'static lifetime.
            let ctx: LlamaContext<'static> = unsafe { std::mem::transmute(ctx) };
            *ctx_guard = Some(ctx);
        } else {
            // Reuse existing context — clear KV cache to reset state.
            // Lightweight GPU op: no VRAM dealloc/realloc, just zeroes the cache.
            ctx_guard.as_mut().unwrap().clear_kv_cache();
            tracing::debug!("KV cache cleared — reusing persistent GPU context");
        }

        let ctx = ctx_guard.as_mut().unwrap();
        tracing::debug!(elapsed_ms = gen_start.elapsed().as_millis(), "LLM context ready");

        // Tokenize prompt (using chat-template-wrapped version)
        tracing::debug!("Tokenizing prompt...");
        let tokens = inner.model.str_to_token(&wrapped, AddBos::Always)
            .map_err(|e| AiError::Provider(format!("Tokenization failed: {:?}", e)))?;
        tracing::info!(
            prompt_tokens = tokens.len(),
            elapsed_ms = gen_start.elapsed().as_millis(),
            "Tokenization complete"
        );

        // Reserve space for generation (prompt + max_tokens must fit in context)
        let max_input_tokens = DEFAULT_CTX_SIZE.saturating_sub(max_tokens) as usize;
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
        tracing::debug!(n_tokens = tokens.len(), "Creating batch and adding prompt tokens");
        let mut batch = LlamaBatch::new(DEFAULT_CTX_SIZE as usize, 1);
        let last_idx = tokens.len() - 1;
        for (i, &token) in tokens.iter().enumerate() {
            batch.add(token, i as i32, &[0], i == last_idx)
                .map_err(|e| AiError::Provider(format!("Batch add failed: {:?}", e)))?;
        }
        tracing::info!(
            n_tokens = tokens.len(),
            ctx_size = DEFAULT_CTX_SIZE,
            n_batch = DEFAULT_CTX_SIZE,
            n_threads = INFERENCE_THREADS,
            "Batch prepared — entering llama.cpp decode"
        );
        // Force flush: if llama.cpp segfaults, at least the last log line is visible.
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

        // Track JSON brace depth for early stop: once we've seen a complete
        // top-level JSON object (depth goes 0→N→0), stop immediately.
        // This prevents Qwen from repeating the same JSON endlessly.
        let mut json_depth: i32 = 0;
        let mut json_started = false;

        for i in 0..max_tokens {
            let token = sampler.sample(ctx, -1);

            // Check end-of-sequence
            if token == eos {
                tracing::debug!(tokens_generated = i, "EOS token reached");
                break;
            }

            // Decode token to string (stateful decoder handles multi-byte UTF-8)
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
                                // Only accept closure if all critical fields are present.
                                // Small LLMs close JSON prematurely — skip early-stop
                                // and let them keep generating if fields are missing.
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
                                    // Reset: let the LLM keep generating
                                    json_depth = 0;
                                    json_started = false;
                                    continue;
                                }
                                tracing::info!(
                                    tokens_generated = i + 1,
                                    "JSON complete — early stop"
                                );
                                // Truncate output to end at this closing brace
                                if let Some(pos) = output.rfind('}') {
                                    output.truncate(pos + 1);
                                }
                                // Return early — skip further generation
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
                                let preview_end = {
                                    let max = output.len().min(500);
                                    let mut end = max;
                                    while end > 0 && !output.is_char_boundary(end) {
                                        end -= 1;
                                    }
                                    end
                                };
                                tracing::info!(output_preview = %&output[..preview_end], "LLM raw output");
                                return Ok(output);
                            }
                        }
                    }
                }
                Err(_e) => {
                    // Special tokens like <|end|> (Phi-4-mini) can't be decoded to text.
                    // This is expected end-of-turn behavior, not an error.
                    tracing::info!(token_idx = i, "End-of-turn token — stopping generation");
                    break;
                }
            }

            // Progress logging every 50 tokens
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
            tracing::warn!(
                elapsed_ms = gen_start.elapsed().as_millis(),
                "Local LLM returned empty response"
            );
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
        // Safe char-boundary truncation for preview (avoid panic on multi-byte UTF-8)
        let preview_end = {
            let max = output.len().min(500);
            let mut end = max;
            while end > 0 && !output.is_char_boundary(end) {
                end -= 1;
            }
            end
        };
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

    /// Get the model file path.
    pub fn model_path(&self) -> &PathBuf {
        &self.model_path
    }
}
