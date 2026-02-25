//! Local LLM — in-process inference via llama.cpp (zero API cost).
//!
//! Replaces the Claude CLI subprocess for Guardian tasks (extraction,
//! coherence, reactivation, merge evaluation, etc.).
//!
//! Model: Qwen2.5-Instruct GGUF, auto-downloaded to {data_dir}/models/ on first use.
//! Sizes: 3B (default, ~2.1GB) or 7B (~4.7GB), selectable via config.
//! Singleton pattern (OnceLock) — same as EmbeddingManager.

use crate::config::LocalModelSize;
use crate::{AiError, AiResult};
use std::path::PathBuf;
use std::sync::OnceLock;

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;

/// Context size (tokens). 8192 is plenty for extraction prompts + 4K content + response.
const DEFAULT_CTX_SIZE: u32 = 8192;

/// Default max output tokens for generation.
const DEFAULT_MAX_TOKENS: u32 = 512;

/// Temperature for sampling (low = more deterministic, better for JSON).
const SAMPLING_TEMP: f32 = 0.1;

static GLOBAL: OnceLock<LocalLlm> = OnceLock::new();

/// Local LLM engine — wraps llama.cpp for in-process inference.
pub struct LocalLlm {
    /// Backend + model held together. None = unavailable.
    inner: Option<LlmInner>,
    model_path: PathBuf,
}

struct LlmInner {
    backend: LlamaBackend,
    model: LlamaModel,
}

// Safety: LlamaBackend and LlamaModel are thread-safe for read operations.
// Each generate() call creates its own mutable LlamaContext, so no shared
// mutable state. llama.cpp guarantees model reads are thread-safe.
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

        let unavailable = |model_path: PathBuf| Self { inner: None, model_path };

        // Initialize llama.cpp backend
        let backend = match LlamaBackend::init() {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("Failed to init llama backend: {:?}, local LLM unavailable", e);
                return unavailable(model_path);
            }
        };

        // Try to find model file
        if !model_path.exists() {
            tracing::info!("Local LLM model not found at {}, attempting download...", model_path.display());
            if let Err(e) = Self::download_model(&model_dir, &model_path, size.download_url()) {
                tracing::warn!("Failed to download model: {}, local LLM unavailable", e);
                return unavailable(model_path);
            }
        }

        // Load model
        let model_params = LlamaModelParams::default();
        match LlamaModel::load_from_file(&backend, &model_path, &model_params) {
            Ok(model) => {
                tracing::info!(
                    model = %model_path.display(),
                    params = model.n_params(),
                    size = %size.display_name(),
                    "Local LLM loaded successfully"
                );
                Self {
                    inner: Some(LlmInner { backend, model }),
                    model_path,
                }
            }
            Err(e) => {
                tracing::warn!("Failed to load model {}: {:?}", model_path.display(), e);
                unavailable(model_path)
            }
        }
    }

    /// Whether local LLM is ready for inference.
    pub fn is_available(&self) -> bool {
        self.inner.is_some()
    }

    /// Generate a response from a prompt.
    ///
    /// Creates a fresh context for each call (stateless — no conversation history).
    /// Thread-safe: each call creates its own LlamaContext from the shared model.
    pub fn generate(&self, prompt: &str, max_tokens: u32) -> AiResult<String> {
        let inner = self.inner.as_ref().ok_or_else(|| {
            AiError::Provider("Local LLM model not loaded".into())
        })?;

        let max_tokens = if max_tokens == 0 { DEFAULT_MAX_TOKENS } else { max_tokens };

        // Create context (one per call — cheap for small models)
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(std::num::NonZeroU32::new(DEFAULT_CTX_SIZE));
        let mut ctx = inner.model.new_context(&inner.backend, ctx_params)
            .map_err(|e| AiError::Provider(format!("Failed to create LLM context: {:?}", e)))?;

        // Tokenize prompt
        let tokens = inner.model.str_to_token(prompt, AddBos::Always)
            .map_err(|e| AiError::Provider(format!("Tokenization failed: {:?}", e)))?;

        if tokens.len() >= DEFAULT_CTX_SIZE as usize {
            return Err(AiError::Provider(format!(
                "Prompt too long: {} tokens (max {})",
                tokens.len(),
                DEFAULT_CTX_SIZE
            )));
        }

        // Create batch and add prompt tokens
        let mut batch = LlamaBatch::new(DEFAULT_CTX_SIZE as usize, 1);
        let last_idx = tokens.len() - 1;
        for (i, &token) in tokens.iter().enumerate() {
            batch.add(token, i as i32, &[0], i == last_idx)
                .map_err(|e| AiError::Provider(format!("Batch add failed: {:?}", e)))?;
        }

        // Decode prompt (process all input tokens)
        ctx.decode(&mut batch)
            .map_err(|e| AiError::Provider(format!("Prompt decode failed: {:?}", e)))?;

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

        for _ in 0..max_tokens {
            let token = sampler.sample(&ctx, -1);

            // Check end-of-sequence
            if token == eos {
                break;
            }

            // Decode token to string (stateful decoder handles multi-byte UTF-8)
            match inner.model.token_to_piece(token, &mut piece_decoder, false, None) {
                Ok(piece) => output.push_str(&piece),
                Err(_) => break,
            }

            // Prepare next iteration
            batch.clear();
            batch.add(token, n_pos, &[0], true)
                .map_err(|e| AiError::Provider(format!("Batch add failed: {:?}", e)))?;

            ctx.decode(&mut batch)
                .map_err(|e| AiError::Provider(format!("Decode failed: {:?}", e)))?;

            n_pos += 1;
        }

        if output.trim().is_empty() {
            return Err(AiError::Provider("Local LLM returned empty response".into()));
        }

        tracing::debug!(
            prompt_tokens = tokens.len(),
            output_tokens = n_pos as usize - tokens.len(),
            output_len = output.len(),
            "Local LLM generation complete"
        );

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
