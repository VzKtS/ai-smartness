//! Embedding Manager — TF-IDF hash-based implementation.
//!
//! SCOPE: calcule des embeddings (vecteurs numeriques) pour la
//! similarite vectorielle (cosine). NE fait PAS d'analyse semantique.
//! Les decisions semantiques sont TOUJOURS faites par le LLM (Guardian).
//!
//! Utilise par: gossip, thread matching, memory retrieval, reactivation.
//! ONNX Runtime support deferred — TF-IDF hash provides good baseline.

use md5::{Digest, Md5};
use std::sync::OnceLock;

/// Dimension du vecteur TF-IDF hash (fixed size).
const TFIDF_DIM: usize = 384;

static GLOBAL: OnceLock<EmbeddingManager> = OnceLock::new();

/// Gestionnaire d'embeddings — singleton global.
pub struct EmbeddingManager {
    pub use_onnx: bool,
}

impl EmbeddingManager {
    /// Initialise le manager — TF-IDF only for now.
    pub fn new() -> Self {
        Self { use_onnx: false }
    }

    /// Singleton global (initialise une seule fois).
    pub fn global() -> &'static Self {
        GLOBAL.get_or_init(|| Self::new())
    }

    /// Calcule l'embedding d'un texte via TF-IDF hash.
    pub fn embed(&self, text: &str) -> Vec<f32> {
        self.embed_tfidf(text)
    }

    /// Calcule les embeddings en batch.
    pub fn embed_batch(&self, texts: &[&str]) -> Vec<Vec<f32>> {
        texts.iter().map(|t| self.embed(t)).collect()
    }

    /// Cosine similarity entre deux vecteurs.
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

        tracing::debug!(best_idx = best_idx, best_similarity = best_sim, candidates = candidates.len(), "Most similar found");

        Some((best_idx, best_sim))
    }

    /// TF-IDF hash embedding: hash each n-gram to a fixed-dimension vector.
    /// Uses MD5 hash to deterministically map terms to vector positions.
    fn embed_tfidf(&self, text: &str) -> Vec<f32> {
        let mut vector = vec![0.0f32; TFIDF_DIM];

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

    // Use first 4 bytes as index, next 4 bytes for sign
    let idx = u32::from_le_bytes([hash[0], hash[1], hash[2], hash[3]]) as usize % vector.len();
    let sign = if hash[4] & 1 == 0 { 1.0f32 } else { -1.0f32 };
    vector[idx] += sign * weight;

    // Second hash position for better distribution
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
        assert_eq!(v.len(), TFIDF_DIM);
        // Not all zeros
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
        // Similar texts should have higher similarity
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
