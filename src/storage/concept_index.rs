//! Concept Index — hash-based O(1) lookup for thread candidates via semantic concepts.
//!
//! Maintains an inverted index: concept → set of thread_ids.
//! Built incrementally from the threads table (concepts column).
//! Shared between Gossip v2 (find_overlaps) and Engram V9 (extract_matching_concepts).
//!
//! Complexity:
//!   - Build: O(N × C) where N=threads, C=avg concepts per thread
//!   - Lookup: O(K) where K=query concepts count
//!   - find_overlaps: O(C × T_avg²) where C=concepts, T_avg=threads per concept

use std::collections::{HashMap, HashSet};
use crate::{AiError, AiResult};
use rusqlite::Connection;

/// Inverted concept index for O(1) candidate lookup.
#[derive(Debug, Default)]
pub struct ConceptIndex {
    /// concept (lowercase) → set of thread_ids
    index: HashMap<String, HashSet<String>>,
    /// thread_id → set of concepts (reverse lookup)
    thread_concepts: HashMap<String, HashSet<String>>,
}

impl ConceptIndex {
    /// Build the index from all active/suspended threads in the database.
    /// Graceful: returns empty index if the threads table doesn't exist yet.
    pub fn build_from_db(conn: &Connection) -> AiResult<Self> {
        let mut idx = Self::default();

        let mut stmt = match conn.prepare(
            "SELECT id, concepts FROM threads WHERE status IN ('Active', 'Suspended')"
        ) {
            Ok(s) => s,
            Err(_) => return Ok(idx),
        };

        let rows = match stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let concepts_json: String = row.get(1)?;
            Ok((id, concepts_json))
        }) {
            Ok(r) => r,
            Err(e) => return Err(AiError::Storage(e.to_string())),
        };

        for row in rows {
            if let Ok((id, concepts_json)) = row {
                let concepts: Vec<String> = serde_json::from_str(&concepts_json).unwrap_or_default();
                if !concepts.is_empty() {
                    idx.insert(&id, &concepts);
                }
            }
        }

        Ok(idx)
    }

    /// Extract concepts from text that match existing indexed concepts.
    /// Used by Engram V9 to convert user message → query concepts.
    /// Only returns words/phrases that exist in the index (known concepts).
    pub fn extract_matching_concepts(&self, text: &str) -> Vec<String> {
        let text_lower = text.to_lowercase();
        let words: HashSet<String> = text_lower
            .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
            .filter(|w| w.len() >= 3)
            .map(String::from)
            .collect();

        let mut matched = Vec::new();
        for concept in self.index.keys() {
            // Single-word concept: direct match
            if words.contains(concept) {
                matched.push(concept.clone());
                continue;
            }
            // Substring match for longer concepts (e.g. "rustfmt" in "using rustfmt")
            if concept.len() >= 4 && text_lower.contains(concept.as_str()) {
                matched.push(concept.clone());
            }
        }
        matched
    }

    /// Lookup candidate thread_ids that share at least one concept with the query.
    pub fn lookup(&self, query_concepts: &[String]) -> HashSet<String> {
        let mut candidates = HashSet::new();
        for concept in query_concepts {
            let key = concept.to_lowercase();
            if let Some(thread_ids) = self.index.get(&key) {
                candidates.extend(thread_ids.iter().cloned());
            }
        }
        candidates
    }

    /// Find all pairs of threads that share at least `min_shared` concepts.
    /// Returns: Vec<(thread_a, thread_b, shared_count, shared_concepts)>
    pub fn find_overlaps(&self, min_shared: usize) -> Vec<(String, String, usize, Vec<String>)> {
        // Count shared concepts for each pair using the inverted index
        let mut pair_shared: HashMap<(String, String), Vec<String>> = HashMap::new();

        for (concept, thread_ids) in &self.index {
            let ids: Vec<&String> = thread_ids.iter().collect();
            for i in 0..ids.len() {
                for j in (i + 1)..ids.len() {
                    let (a, b) = if ids[i] < ids[j] {
                        (ids[i].clone(), ids[j].clone())
                    } else {
                        (ids[j].clone(), ids[i].clone())
                    };
                    pair_shared.entry((a, b)).or_default().push(concept.clone());
                }
            }
        }

        pair_shared
            .into_iter()
            .filter(|(_, concepts)| concepts.len() >= min_shared)
            .map(|((a, b), concepts)| {
                let count = concepts.len();
                (a, b, count, concepts)
            })
            .collect()
    }

    /// Compute overlap score between two specific threads.
    /// Returns (shared_count, ratio, shared_concepts).
    /// ratio = shared / min(|A|, |B|)
    pub fn overlap_score(&self, thread_a: &str, thread_b: &str) -> (usize, f64, Vec<String>) {
        let concepts_a = match self.thread_concepts.get(thread_a) {
            Some(c) => c,
            None => return (0, 0.0, vec![]),
        };
        let concepts_b = match self.thread_concepts.get(thread_b) {
            Some(c) => c,
            None => return (0, 0.0, vec![]),
        };

        let shared: Vec<String> = concepts_a.intersection(concepts_b).cloned().collect();
        let count = shared.len();
        let min_size = concepts_a.len().min(concepts_b.len()).max(1);
        let ratio = count as f64 / min_size as f64;

        (count, ratio, shared)
    }

    /// Add a thread to the index.
    pub fn insert(&mut self, thread_id: &str, concepts: &[String]) {
        let mut concept_set = HashSet::new();
        for concept in concepts {
            let key = concept.to_lowercase();
            self.index.entry(key.clone()).or_default().insert(thread_id.to_string());
            concept_set.insert(key);
        }
        self.thread_concepts.insert(thread_id.to_string(), concept_set);
    }

    /// Remove a thread from the index.
    pub fn remove(&mut self, thread_id: &str) {
        if let Some(concepts) = self.thread_concepts.remove(thread_id) {
            for concept in &concepts {
                if let Some(ids) = self.index.get_mut(concept) {
                    ids.remove(thread_id);
                    if ids.is_empty() {
                        self.index.remove(concept);
                    }
                }
            }
        }
    }

    /// Update a thread's concepts in the index.
    pub fn update(&mut self, thread_id: &str, concepts: &[String]) {
        self.remove(thread_id);
        self.insert(thread_id, concepts);
    }

    /// Number of distinct concepts indexed.
    pub fn concept_count(&self) -> usize {
        self.index.len()
    }

    /// Number of distinct threads indexed.
    pub fn thread_count(&self) -> usize {
        self.thread_concepts.len()
    }
}
