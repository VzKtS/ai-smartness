//! Topic Index — hash-based O(1) lookup for thread candidates.
//!
//! Maintains an inverted index: topic → set of thread_ids.
//! Built incrementally from the threads table.
//! Used by EngramRetriever Phase 1 to pre-filter candidates
//! before running the 9 validators.
//!
//! Complexity:
//!   - Build: O(N × T) where N=threads, T=avg topics per thread
//!   - Lookup: O(K) where K=query topics count
//!   - Update: O(T) per thread insert/update

use std::collections::{HashMap, HashSet};
use crate::{AiError, AiResult};
use rusqlite::Connection;

/// Inverted topic index for O(1) candidate lookup.
#[derive(Debug, Default)]
pub struct TopicIndex {
    /// topic (lowercase) → set of thread_ids
    index: HashMap<String, HashSet<String>>,
    /// bigram "topicA+topicB" → set of thread_ids (for multi-topic precision)
    bigram_index: HashMap<String, HashSet<String>>,
}

impl TopicIndex {
    /// Build the index from all active/suspended threads in the database.
    /// Graceful: returns empty index if the threads table doesn't exist yet.
    pub fn build_from_db(conn: &Connection) -> AiResult<Self> {
        let mut idx = Self::default();

        let mut stmt = match conn.prepare(
            "SELECT id, topics FROM threads WHERE topics != '[]'"
        ) {
            Ok(s) => s,
            Err(_) => return Ok(idx), // Table likely doesn't exist yet
        };

        let rows = match stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let topics_json: String = row.get(1)?;
            Ok((id, topics_json))
        }) {
            Ok(r) => r,
            Err(e) => return Err(AiError::Storage(e.to_string())),
        };

        for row in rows {
            if let Ok((id, topics_json)) = row {
                let topics: Vec<String> = serde_json::from_str(&topics_json).unwrap_or_default();
                if !topics.is_empty() {
                    idx.insert(&id, &topics);
                }
            }
        }

        Ok(idx)
    }

    /// Extract topics from text that match existing indexed topics.
    /// Used by EngramRetriever Phase 1 to convert user message → query topics.
    /// Only returns words/phrases that exist in the index (known topics).
    pub fn extract_matching_topics(&self, text: &str) -> Vec<String> {
        let text_lower = text.to_lowercase();
        let words: HashSet<String> = text_lower
            .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
            .filter(|w| w.len() >= 3)
            .map(String::from)
            .collect();

        let mut matched = Vec::new();
        for topic in self.index.keys() {
            // Single-word topic: direct match
            if words.contains(topic) {
                matched.push(topic.clone());
                continue;
            }
            // Multi-word topic: check if all parts appear in text
            let parts: Vec<&str> = topic.split(|c: char| c == ' ' || c == '_' || c == '-')
                .filter(|p| p.len() >= 2)
                .collect();
            if parts.len() >= 2 && parts.iter().all(|p| text_lower.contains(p)) {
                matched.push(topic.clone());
                continue;
            }
            // Substring match for longer topics (e.g. "rustfmt" matches in "using rustfmt")
            if topic.len() >= 4 && text_lower.contains(topic.as_str()) {
                matched.push(topic.clone());
            }
        }
        matched
    }

    /// Lookup candidate thread_ids that share at least one topic with the query.
    pub fn lookup(&self, query_topics: &[String]) -> HashSet<String> {
        let mut candidates = HashSet::new();
        for topic in query_topics {
            let key = topic.to_lowercase();
            if let Some(thread_ids) = self.index.get(&key) {
                candidates.extend(thread_ids.iter().cloned());
            }
        }
        // Also check bigrams for higher precision
        if query_topics.len() >= 2 {
            for i in 0..query_topics.len() {
                for j in (i + 1)..query_topics.len() {
                    let bigram = Self::make_bigram(&query_topics[i], &query_topics[j]);
                    if let Some(thread_ids) = self.bigram_index.get(&bigram) {
                        candidates.extend(thread_ids.iter().cloned());
                    }
                }
            }
        }
        candidates
    }

    /// Add a thread to the index.
    pub fn insert(&mut self, thread_id: &str, topics: &[String]) {
        for topic in topics {
            let key = topic.to_lowercase();
            self.index.entry(key).or_default().insert(thread_id.to_string());
        }
        // Build bigrams
        if topics.len() >= 2 {
            for i in 0..topics.len() {
                for j in (i + 1)..topics.len() {
                    let bigram = Self::make_bigram(&topics[i], &topics[j]);
                    self.bigram_index.entry(bigram).or_default()
                        .insert(thread_id.to_string());
                }
            }
        }
    }

    /// Remove a thread from the index.
    pub fn remove(&mut self, thread_id: &str) {
        for ids in self.index.values_mut() {
            ids.remove(thread_id);
        }
        for ids in self.bigram_index.values_mut() {
            ids.remove(thread_id);
        }
        // Clean up empty entries
        self.index.retain(|_, v| !v.is_empty());
        self.bigram_index.retain(|_, v| !v.is_empty());
    }

    /// Update a thread's topics in the index.
    pub fn update(&mut self, thread_id: &str, topics: &[String]) {
        self.remove(thread_id);
        self.insert(thread_id, topics);
    }

    /// Number of distinct topics indexed.
    pub fn topic_count(&self) -> usize {
        self.index.len()
    }

    /// Number of distinct threads indexed.
    pub fn thread_count(&self) -> usize {
        let mut all: HashSet<&String> = HashSet::new();
        for ids in self.index.values() {
            all.extend(ids);
        }
        all.len()
    }

    fn make_bigram(a: &str, b: &str) -> String {
        let a_lower = a.to_lowercase();
        let b_lower = b.to_lowercase();
        if a_lower <= b_lower {
            format!("{}+{}", a_lower, b_lower)
        } else {
            format!("{}+{}", b_lower, a_lower)
        }
    }
}
