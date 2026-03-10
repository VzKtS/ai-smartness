//! Embedding Manager — ONNX all-MiniLM-L6-v2 with TF-IDF hash fallback.
//!
//! Produces 384-dim embeddings for semantic similarity.
//! Tries ONNX first (high quality), falls back to TF-IDF hash (zero-dep).
//!
//! Model location: {data_dir}/models/all-MiniLM-L6-v2/
//!   - model.onnx
//!   - tokenizer.json

use md5::{Digest, Md5};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// Dimension of embedding vectors (all-MiniLM-L6-v2 native dim).
const EMBED_DIM: usize = 384;

/// Max token length for ONNX model input.
const MAX_TOKENS: usize = 128;

static GLOBAL: OnceLock<EmbeddingManager> = OnceLock::new();

/// Embedding manager — ONNX + TF-IDF fallback singleton.
pub struct EmbeddingManager {
    pub use_onnx: bool,
    onnx_session: Mutex<Option<ort::session::Session>>,
    tokenizer: Option<tokenizers::Tokenizer>,
}

impl EmbeddingManager {
    /// Initialize: try ONNX, fall back to TF-IDF.
    /// Wrapped in catch_unwind because `ort` with `load-dynamic` panics
    /// if `libonnxruntime.so` is not found (instead of returning Err).
    pub fn new() -> Self {
        let onnx_result = std::panic::catch_unwind(|| Self::try_init_onnx());

        match onnx_result {
            Ok(Ok((session, tokenizer))) => {
                tracing::info!("ONNX embedding engine loaded (all-MiniLM-L6-v2)");
                Self {
                    use_onnx: true,
                    onnx_session: Mutex::new(Some(session)),
                    tokenizer: Some(tokenizer),
                }
            }
            Ok(Err(e)) => {
                tracing::warn!("ONNX unavailable, using TF-IDF fallback: {}", e);
                Self {
                    use_onnx: false,
                    onnx_session: Mutex::new(None),
                    tokenizer: None,
                }
            }
            Err(_panic) => {
                tracing::warn!("ONNX init panicked (likely missing libonnxruntime.so), using TF-IDF fallback");
                Self {
                    use_onnx: false,
                    onnx_session: Mutex::new(None),
                    tokenizer: None,
                }
            }
        }
    }

    /// Global singleton (initialized once).
    pub fn global() -> &'static Self {
        GLOBAL.get_or_init(Self::new)
    }

    /// Embed a single text.
    /// Protected with catch_unwind: if ONNX panics at runtime, falls back to TF-IDF.
    pub fn embed(&self, text: &str) -> Vec<f32> {
        if self.use_onnx {
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.embed_onnx(text))) {
                Ok(Ok(v)) => v,
                Ok(Err(e)) => {
                    tracing::warn!("ONNX embed failed, TF-IDF fallback: {}", e);
                    self.embed_tfidf(text)
                }
                Err(_panic) => {
                    tracing::error!("ONNX embed panicked, TF-IDF fallback");
                    self.embed_tfidf(text)
                }
            }
        } else {
            self.embed_tfidf(text)
        }
    }

    /// Embed respecting the configured EmbeddingMode.
    /// Returns None for Disabled or OnnxOnly-when-ONNX-unavailable.
    pub fn embed_with_mode(&self, text: &str, mode: &crate::config::EmbeddingMode) -> Option<Vec<f32>> {
        use crate::config::EmbeddingMode;
        match mode {
            EmbeddingMode::Disabled => None,
            EmbeddingMode::TfidfOnly => Some(self.embed_tfidf(text)),
            EmbeddingMode::OnnxOnly => {
                if self.use_onnx {
                    Some(self.embed(text))
                } else {
                    None
                }
            }
            EmbeddingMode::OnnxWithFallback => Some(self.embed(text)),
        }
    }

    /// Embed a batch of texts.
    pub fn embed_batch(&self, texts: &[&str]) -> Vec<Vec<f32>> {
        texts.iter().map(|t| self.embed(t)).collect()
    }

    /// Cosine similarity between two vectors.
    pub fn similarity(&self, a: &[f32], b: &[f32]) -> f64 {
        cosine_similarity(a, b)
    }

    /// Find most similar vector from a set. Returns (index, similarity).
    pub fn find_most_similar(&self, query: &[f32], candidates: &[Vec<f32>]) -> Option<(usize, f64)> {
        if candidates.is_empty() {
            return None;
        }

        let mut best_idx = 0;
        let mut best_sim = f64::NEG_INFINITY;

        for (i, candidate) in candidates.iter().enumerate() {
            let sim = self.similarity(query, candidate);
            if sim > best_sim {
                best_sim = sim;
                best_idx = i;
            }
        }

        Some((best_idx, best_sim))
    }

    /// Returns the dimension of embeddings produced.
    pub fn dimension(&self) -> usize {
        EMBED_DIM
    }

    // ── ONNX ──

    fn model_dir() -> PathBuf {
        crate::storage::path_utils::data_dir()
            .join("models")
            .join("all-MiniLM-L6-v2")
    }

    fn try_init_onnx() -> Result<(ort::session::Session, tokenizers::Tokenizer), String> {
        let model_dir = Self::model_dir();
        let model_path = model_dir.join("model.onnx");
        let tokenizer_path = model_dir.join("tokenizer.json");

        if !model_path.exists() {
            return Err(format!("model.onnx not found at {}", model_path.display()));
        }
        if !tokenizer_path.exists() {
            return Err(format!("tokenizer.json not found at {}", tokenizer_path.display()));
        }

        let session = ort::session::Session::builder()
            .map_err(|e| format!("ONNX session builder: {}", e))?
            .with_intra_threads(1)
            .map_err(|e| format!("ONNX set threads: {}", e))?
            .commit_from_file(&model_path)
            .map_err(|e| format!("ONNX load model: {}", e))?;

        let tokenizer = tokenizers::Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| format!("Tokenizer load: {}", e))?;

        Ok((session, tokenizer))
    }

    fn embed_onnx(&self, text: &str) -> Result<Vec<f32>, String> {
        use ort::value::Tensor;

        let mut guard = self.onnx_session.lock().map_err(|e| format!("Mutex poisoned: {}", e))?;
        let session = guard.as_mut().ok_or("No ONNX session")?;
        let tokenizer = self.tokenizer.as_ref().ok_or("No tokenizer")?;

        // Tokenize
        let encoding = tokenizer.encode(text, true)
            .map_err(|e| format!("Tokenize failed: {}", e))?;

        let mut input_ids: Vec<i64> = encoding.get_ids().iter().map(|&id| id as i64).collect();
        let mut attention_mask: Vec<i64> = encoding.get_attention_mask().iter().map(|&m| m as i64).collect();
        let mut token_type_ids: Vec<i64> = encoding.get_type_ids().iter().map(|&t| t as i64).collect();

        // Truncate to max tokens
        if input_ids.len() > MAX_TOKENS {
            input_ids.truncate(MAX_TOKENS);
            attention_mask.truncate(MAX_TOKENS);
            token_type_ids.truncate(MAX_TOKENS);
        }

        let seq_len = input_ids.len();

        // Create ort Tensor inputs [1, seq_len]
        let input_ids_tensor = Tensor::from_array(([1, seq_len], input_ids))
            .map_err(|e| format!("input_ids tensor: {}", e))?;
        let attention_mask_tensor = Tensor::from_array(([1, seq_len], attention_mask.clone()))
            .map_err(|e| format!("attention_mask tensor: {}", e))?;
        let token_type_ids_tensor = Tensor::from_array(([1, seq_len], token_type_ids))
            .map_err(|e| format!("token_type_ids tensor: {}", e))?;

        // Run inference
        let outputs = session.run(ort::inputs! {
            "input_ids" => input_ids_tensor,
            "attention_mask" => attention_mask_tensor,
            "token_type_ids" => token_type_ids_tensor,
        })
        .map_err(|e| format!("ONNX run: {}", e))?;

        // Extract output: [1, seq_len, 384]
        let (shape, raw_data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("Extract tensor: {}", e))?;

        // shape should be [1, seq_len, EMBED_DIM]
        let dim2 = if shape.len() >= 3 { shape[2] as usize } else { EMBED_DIM };

        // Mean pooling with attention mask
        let mut pooled = vec![0.0f32; EMBED_DIM];
        let mut mask_sum = 0.0f32;

        for t in 0..seq_len {
            let mask_val = attention_mask[t] as f32;
            if mask_val > 0.0 {
                let offset = t * dim2;
                for d in 0..EMBED_DIM.min(dim2) {
                    pooled[d] += raw_data[offset + d] * mask_val;
                }
                mask_sum += mask_val;
            }
        }

        if mask_sum > 0.0 {
            for d in 0..EMBED_DIM {
                pooled[d] /= mask_sum;
            }
        }

        // L2 normalize
        let norm: f32 = pooled.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in pooled.iter_mut() {
                *v /= norm;
            }
        }

        Ok(pooled)
    }

    // ── TF-IDF hash fallback ──

    fn embed_tfidf(&self, text: &str) -> Vec<f32> {
        let mut vector = vec![0.0f32; EMBED_DIM];

        let lower = text.to_lowercase();
        let words: Vec<&str> = lower.split_whitespace().collect();

        if words.is_empty() {
            return vector;
        }

        // Unigrams
        for word in &words {
            let w = word.trim_matches(|c: char| !c.is_alphanumeric());
            if w.len() < 2 {
                continue;
            }
            hash_term_into(&mut vector, w, 1.0);
        }

        // Bigrams
        for pair in words.windows(2) {
            let bigram = format!(
                "{}_{}",
                pair[0].trim_matches(|c: char| !c.is_alphanumeric()),
                pair[1].trim_matches(|c: char| !c.is_alphanumeric())
            );
            hash_term_into(&mut vector, &bigram, 0.7);
        }

        // L2 normalize
        let norm: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in vector.iter_mut() {
                *v /= norm;
            }
        }

        vector
    }
}

impl Default for EmbeddingManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Hash a term into a fixed-dimension vector using MD5.
fn hash_term_into(vector: &mut [f32], term: &str, weight: f32) {
    let mut hasher = Md5::new();
    hasher.update(term.as_bytes());
    let hash = hasher.finalize();

    let idx = u32::from_le_bytes([hash[0], hash[1], hash[2], hash[3]]) as usize % vector.len();
    let sign = if hash[4] & 1 == 0 { 1.0f32 } else { -1.0f32 };
    vector[idx] += sign * weight;

    let idx2 = u32::from_le_bytes([hash[5], hash[6], hash[7], hash[8]]) as usize % vector.len();
    let sign2 = if hash[9] & 1 == 0 { 1.0f32 } else { -1.0f32 };
    vector[idx2] += sign2 * weight * 0.5;
}

/// Cosine similarity between two vectors.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| *x as f64 * *y as f64).sum();
    let norm_a: f64 = a.iter().map(|x| (*x as f64).powi(2)).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| (*x as f64).powi(2)).sum::<f64>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embed_produces_vector() {
        let mgr = EmbeddingManager::new();
        let v = mgr.embed("hello world");
        assert_eq!(v.len(), EMBED_DIM);
        assert!(v.iter().any(|x| *x != 0.0));
    }

    #[test]
    fn test_similar_texts() {
        let mgr = EmbeddingManager::new();
        let a = mgr.embed("rust programming language");
        let b = mgr.embed("rust programming tutorial");
        let c = mgr.embed("french cooking recipes");
        let sim_ab = mgr.similarity(&a, &b);
        let sim_ac = mgr.similarity(&a, &c);
        assert!(sim_ab > sim_ac, "sim_ab={} should be > sim_ac={}", sim_ab, sim_ac);
    }

    #[test]
    fn test_self_similarity() {
        let mgr = EmbeddingManager::new();
        let v = mgr.embed("test text");
        let sim = mgr.similarity(&v, &v);
        assert!((sim - 1.0).abs() < 0.001);
    }
}
