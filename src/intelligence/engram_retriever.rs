//! Engram Retriever — multi-validator consensus for memory injection.
//!
//! Inspired by DeepSeek Engram (Conditional Memory via Scalable Lookup).
//! Replaces single-signal cosine scoring with 9-validator voting.
//!
//! Pipeline:
//!   Phase 1: TopicIndex + ConceptIndex hash lookup O(1) → candidate pre-filter
//!   Phase 2: 9 validators vote (pass/fail + confidence)
//!   Phase 3: Consensus → StrongInject / WeakInject / Skip
//!
//! 8/9 validators are zero-cost (memory lookup).
//! Only V1 (SemanticSimilarity) costs compute.

use std::collections::HashMap;

use crate::thread::{Thread, ThreadStatus, OriginType, WorkContext, InjectionStats};
use crate::config::EngramConfig;
use crate::AiResult;
use crate::processing::embeddings::EmbeddingManager;
use crate::storage::bridges::BridgeStorage;
use crate::storage::concept_index::ConceptIndex;
use crate::storage::topic_index::TopicIndex;
use chrono::{DateTime, Utc};
use rusqlite::Connection;

use crate::intelligence::validators::{
    QueryContext, Validator, ValidatorVote,
    SemanticSimilarityValidator, TopicOverlapValidator,
    TemporalProximityValidator, GraphConnectivityValidator,
    InjectionHistoryValidator, DecayedRelevanceValidator,
    LabelCoherenceValidator, FocusAlignmentValidator,
    ConceptCoherenceValidator,
};

/// Injection decision after multi-validator consensus.
#[derive(Debug, Clone, PartialEq)]
pub enum InjectionDecision {
    /// ≥5/8 validators pass → inject at top of context, full content.
    StrongInject,
    /// 3-4/8 validators pass → inject at bottom of context, condensed.
    WeakInject,
    /// <3/8 validators pass → skip injection.
    Skip,
}

/// Detailed Engram validation result for a thread candidate.
#[derive(Debug, Clone)]
pub struct EngramScore {
    pub thread_id: String,
    pub votes: Vec<ValidatorVote>,
    pub weighted_score: f64,
    pub pass_count: u8,
    pub decision: InjectionDecision,
}

/// Engram Retriever — replaces MemoryRetriever.
///
/// Uses TopicIndex + ConceptIndex (hash-based O(1) lookup) + 9 independent validators
/// for multi-signal consensus on memory injection decisions.
pub struct EngramRetriever {
    validators: Vec<Box<dyn Validator>>,
    validator_weights: [f64; 9],
    topic_index: TopicIndex,
    concept_index: ConceptIndex,
    config: EngramConfig,
    strong_inject_min: u8,
    weak_inject_min: u8,
}

impl EngramRetriever {
    /// Create a new EngramRetriever from config.
    /// Builds the TopicIndex + ConceptIndex from the database and initializes all 9 validators.
    pub fn new(conn: &Connection, config: EngramConfig) -> AiResult<Self> {
        let topic_index = TopicIndex::build_from_db(conn)?;
        let concept_index = ConceptIndex::build_from_db(conn)?;

        // V1 threshold: use active threshold based on ONNX availability
        let use_onnx = EmbeddingManager::global().use_onnx;
        let embedding_threshold = config.embedding.active_threshold(use_onnx);

        let validators: Vec<Box<dyn Validator>> = vec![
            Box::new(SemanticSimilarityValidator { threshold: embedding_threshold }),
            Box::new(TopicOverlapValidator { min_shared: 1 }),
            Box::new(TemporalProximityValidator),
            Box::new(GraphConnectivityValidator),
            Box::new(InjectionHistoryValidator),
            Box::new(DecayedRelevanceValidator { min_score: 0.1 }),
            Box::new(LabelCoherenceValidator),
            Box::new(FocusAlignmentValidator),
            Box::new(ConceptCoherenceValidator { min_shared: 2 }),  // V9
        ];

        let validator_weights = config.validator_weights.to_array();
        let strong = config.strong_inject_min_votes;
        let weak = config.weak_inject_min_votes;

        Ok(Self {
            validators,
            validator_weights,
            topic_index,
            concept_index,
            strong_inject_min: strong,
            weak_inject_min: weak,
            config,
        })
    }

    /// Refresh topic and concept indexes from the database.
    /// Called periodically by the daemon prune loop.
    pub fn refresh_index(&mut self, conn: &Connection) -> AiResult<()> {
        self.topic_index = TopicIndex::build_from_db(conn)?;
        self.concept_index = ConceptIndex::build_from_db(conn)?;
        Ok(())
    }

    /// Notify the indexes of a thread change (insert/update/remove).
    /// More efficient than full refresh for single-thread changes.
    pub fn notify_thread_change(
        &mut self,
        thread_id: &str,
        topics: Option<&[String]>,
        concepts: Option<&[String]>,
    ) {
        match topics {
            Some(t) => self.topic_index.update(thread_id, t),
            None => self.topic_index.remove(thread_id),
        }
        match concepts {
            Some(c) => self.concept_index.update(thread_id, c),
            None => self.concept_index.remove(thread_id),
        }
    }

    /// Main retrieval — Engram-inspired 3-phase pipeline.
    ///
    /// Phase 1: TopicIndex + ConceptIndex hash lookup O(1) → candidate pre-filter
    /// Phase 2: 9 validators vote on each candidate
    /// Phase 3: Consensus → StrongInject / WeakInject / Skip
    pub fn get_relevant_context(
        &self,
        conn: &Connection,
        user_message: &str,
        limit: usize,
    ) -> AiResult<Vec<Thread>> {
        tracing::info!(query_len = user_message.len(), limit = limit, "Engram retrieval starting");

        // === Phase 1: Topic + concept extraction + hash index pre-filter ===
        let query_topics = self.topic_index.extract_matching_topics(user_message);
        let query_concepts = self.concept_index.extract_matching_concepts(user_message);

        let candidate_ids = if self.config.hash_index_enabled
            && (!query_topics.is_empty() || !query_concepts.is_empty())
        {
            // Union of TopicIndex and ConceptIndex candidates
            let mut ids = self.topic_index.lookup(&query_topics);
            let concept_ids = self.concept_index.lookup(&query_concepts);
            ids.extend(concept_ids);
            // Cap candidates to avoid scanning too many threads
            if ids.len() > self.config.max_candidates {
                ids.into_iter().take(self.config.max_candidates).collect()
            } else {
                ids
            }
        } else {
            // Fallback: load all active thread IDs (limited)
            load_active_thread_ids(conn, self.config.max_candidates)?
        };

        tracing::debug!(
            candidates = candidate_ids.len(),
            query_topics = ?query_topics,
            query_concepts = ?query_concepts,
            "Phase 1 pre-filter complete"
        );

        if candidate_ids.is_empty() {
            tracing::debug!("No candidates found, returning empty");
            return Ok(Vec::new());
        }

        // Load full Thread objects for scoring
        let candidates = load_threads_by_ids(conn, &candidate_ids)?;

        // Pre-compute context for validators
        let active_thread_id = find_most_recent_active_thread(conn)?;
        let bridge_connections = load_bridge_connections(conn, active_thread_id.as_deref())?;
        let query_embedding = compute_query_embedding(user_message, &self.config.embedding.mode);

        let ctx = QueryContext {
            user_message: user_message.to_string(),
            query_embedding,
            query_topics,
            query_concepts,
            active_thread_id,
            focus_topics: load_focus_topics(conn),
            label_hint: None,
            bridge_connections,
        };

        // === Phase 2: Score each candidate with 9 validators ===
        let mut scores: Vec<EngramScore> = candidates.iter()
            .filter_map(|t| self.score_thread_engram(t, &ctx))
            .collect();

        // === Phase 3: Consensus → sort, filter, return ===
        scores.sort_by(|a, b| b.weighted_score.partial_cmp(&a.weighted_score)
            .unwrap_or(std::cmp::Ordering::Equal));

        let result: Vec<Thread> = scores.iter()
            .filter(|s| s.decision != InjectionDecision::Skip)
            .take(limit)
            .filter_map(|s| candidates.iter().find(|t| t.id == s.thread_id).cloned())
            .collect();

        tracing::info!(
            candidates_scored = scores.len(),
            injected = result.len(),
            strong = scores.iter().filter(|s| s.decision == InjectionDecision::StrongInject).count(),
            weak = scores.iter().filter(|s| s.decision == InjectionDecision::WeakInject).count(),
            "Engram retrieval complete"
        );

        // Hebbian reinforcement: strengthen bridges connecting injected threads.
        // This breaks the death spiral: bridges used during retrieval get
        // last_reinforced updated → decay clock resets → bridge stays alive.
        if !result.is_empty() {
            reinforce_used_bridges(conn, &result, &ctx.bridge_connections);
        }

        Ok(result)
    }

    /// Score a thread using all 8 validators.
    fn score_thread_engram(
        &self,
        thread: &Thread,
        query_ctx: &QueryContext,
    ) -> Option<EngramScore> {
        let votes: Vec<ValidatorVote> = self.validators.iter()
            .map(|v| v.validate(thread, query_ctx))
            .collect();

        let (pass_count, weighted_score, decision) = self.consensus(&votes);

        tracing::trace!(
            thread_id = %thread.id,
            pass_count = pass_count,
            weighted_score = weighted_score,
            decision = ?decision,
            "Engram score computed"
        );

        Some(EngramScore {
            thread_id: thread.id.clone(),
            votes,
            weighted_score,
            pass_count,
            decision,
        })
    }

    /// Compute consensus from validator votes.
    fn consensus(&self, votes: &[ValidatorVote]) -> (u8, f64, InjectionDecision) {
        let mut pass_count: u8 = 0;
        let mut weighted_score = 0.0;
        for (i, vote) in votes.iter().enumerate() {
            if vote.pass {
                pass_count += 1;
                if i < self.validator_weights.len() {
                    weighted_score += vote.confidence * self.validator_weights[i];
                }
            }
        }
        let decision = if pass_count >= self.strong_inject_min {
            InjectionDecision::StrongInject
        } else if pass_count >= self.weak_inject_min {
            InjectionDecision::WeakInject
        } else {
            InjectionDecision::Skip
        };
        (pass_count, weighted_score, decision)
    }

    /// Search (for ai_recall MCP tool).
    /// Similar to get_relevant_context but with broader matching
    /// and no StrongInject/WeakInject distinction in output.
    pub fn search(
        &self,
        conn: &Connection,
        query: &str,
        limit: usize,
    ) -> AiResult<Vec<Thread>> {
        let query_topics = self.topic_index.extract_matching_topics(query);
        let query_concepts = self.concept_index.extract_matching_concepts(query);

        let candidate_ids = if !query_topics.is_empty() || !query_concepts.is_empty() {
            let mut ids = self.topic_index.lookup(&query_topics);
            ids.extend(self.concept_index.lookup(&query_concepts));
            ids
        } else {
            // No topic/concept match — fall back to text search
            return search_threads_by_text(conn, query, limit);
        };

        if candidate_ids.is_empty() {
            return search_threads_by_text(conn, query, limit);
        }

        let candidates = load_threads_by_ids(conn, &candidate_ids)?;
        let query_embedding = compute_query_embedding(query, &self.config.embedding.mode);

        let ctx = QueryContext {
            user_message: query.to_string(),
            query_embedding,
            query_topics,
            query_concepts,
            active_thread_id: None,
            focus_topics: Vec::new(),
            label_hint: None,
            bridge_connections: HashMap::new(),
        };

        let mut scores: Vec<EngramScore> = candidates.iter()
            .filter_map(|t| self.score_thread_engram(t, &ctx))
            .collect();

        scores.sort_by(|a, b| b.weighted_score.partial_cmp(&a.weighted_score)
            .unwrap_or(std::cmp::Ordering::Equal));

        let result: Vec<Thread> = scores.iter()
            .take(limit)
            .filter_map(|s| candidates.iter().find(|t| t.id == s.thread_id).cloned())
            .collect();

        Ok(result)
    }

    /// Get the current index statistics: (topics, topic_threads, concepts, concept_threads).
    pub fn index_stats(&self) -> (usize, usize, usize, usize) {
        (
            self.topic_index.topic_count(),
            self.topic_index.thread_count(),
            self.concept_index.concept_count(),
            self.concept_index.thread_count(),
        )
    }
}

// =============================================================================
// DB Helper functions
// Temporary direct SQL queries — will be replaced by ThreadStorage/BridgeStorage
// once those are implemented.
// =============================================================================

/// Load active thread IDs, ordered by most recently active.
fn load_active_thread_ids(
    conn: &Connection,
    max: usize,
) -> AiResult<std::collections::HashSet<String>> {
    let mut ids = std::collections::HashSet::new();
    let sql = "SELECT id FROM threads \
               ORDER BY last_active DESC LIMIT ?1";

    let mut stmt = match conn.prepare(sql) {
        Ok(s) => s,
        Err(_) => return Ok(ids),
    };
    let rows = match stmt.query_map(rusqlite::params![max as i64], |row| {
        row.get::<_, String>(0)
    }) {
        Ok(r) => r,
        Err(_) => return Ok(ids),
    };
    for row in rows {
        if let Ok(id) = row {
            ids.insert(id);
        }
    }
    Ok(ids)
}

/// Load full Thread objects by their IDs.
fn load_threads_by_ids(
    conn: &Connection,
    ids: &std::collections::HashSet<String>,
) -> AiResult<Vec<Thread>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    let placeholders = std::iter::repeat("?").take(ids.len()).collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT id, title, status, weight, importance, importance_manually_set, \
         created_at, last_active, activation_count, split_locked, split_locked_until, \
         origin_type, drift_history, parent_id, child_ids, summary, topics, tags, labels, \
         concepts, embedding, relevance_score, ratings, work_context, injection_stats \
         FROM threads WHERE id IN ({})",
        placeholders
    );

    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(_) => return Ok(Vec::new()),
    };

    let id_vec: Vec<String> = ids.iter().cloned().collect();
    let rows = match stmt.query_map(
        rusqlite::params_from_iter(id_vec.iter()),
        row_to_thread,
    ) {
        Ok(r) => r,
        Err(_) => return Ok(Vec::new()),
    };

    let mut threads = Vec::new();
    for row in rows {
        if let Ok(t) = row {
            threads.push(t);
        }
    }
    Ok(threads)
}

/// Convert a SQLite row to a Thread struct.
fn row_to_thread(row: &rusqlite::Row) -> rusqlite::Result<Thread> {
    let status_str: String = row.get(2)?;
    let status = match status_str.as_str() {
        "suspended" => ThreadStatus::Suspended,
        "archived" => ThreadStatus::Archived,
        _ => ThreadStatus::Active,
    };

    let origin_str: String = row.get(11)?;
    let origin_type = match origin_str.as_str() {
        "file_read" => OriginType::FileRead,
        "file_write" => OriginType::FileWrite,
        "task" => OriginType::Task,
        "fetch" => OriginType::Fetch,
        "response" => OriginType::Response,
        "command" => OriginType::Command,
        "split" => OriginType::Split,
        "reactivation" => OriginType::Reactivation,
        _ => OriginType::Prompt,
    };

    let created_at_str: String = row.get(6)?;
    let last_active_str: String = row.get(7)?;
    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());
    let last_active = DateTime::parse_from_rfc3339(&last_active_str)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());

    let split_locked_until_str: Option<String> = row.get(10)?;
    let split_locked_until = split_locked_until_str.and_then(|s|
        DateTime::parse_from_rfc3339(&s).ok().map(|dt| dt.with_timezone(&Utc))
    );

    let drift_json: String = row.get(12)?;
    let drift_history: Vec<String> = serde_json::from_str(&drift_json).unwrap_or_default();

    let child_ids_json: String = row.get(14)?;
    let child_ids: Vec<String> = serde_json::from_str(&child_ids_json).unwrap_or_default();

    let topics_json: String = row.get(16)?;
    let topics: Vec<String> = serde_json::from_str(&topics_json).unwrap_or_default();

    let tags_json: String = row.get(17)?;
    let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();

    let labels_json: String = row.get(18)?;
    let labels: Vec<String> = serde_json::from_str(&labels_json).unwrap_or_default();

    let concepts_json: String = row.get(19)?;
    let concepts: Vec<String> = serde_json::from_str(&concepts_json).unwrap_or_default();

    // Embedding: stored as BLOB (raw little-endian f32 bytes)
    let embedding_blob: Option<Vec<u8>> = row.get(20)?;
    let embedding = embedding_blob.and_then(|blob| {
        if blob.len() % 4 != 0 || blob.is_empty() {
            return None;
        }
        Some(blob.chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect())
    });

    let ratings_json: String = row.get(22)?;
    let ratings: Vec<serde_json::Value> = serde_json::from_str(&ratings_json).unwrap_or_default();

    let wc_json: Option<String> = row.get(23)?;
    let work_context: Option<WorkContext> = wc_json.and_then(|s| serde_json::from_str(&s).ok());

    let is_json: Option<String> = row.get(24)?;
    let injection_stats: Option<InjectionStats> = is_json.and_then(|s| serde_json::from_str(&s).ok());

    Ok(Thread {
        id: row.get(0)?,
        title: row.get(1)?,
        status,
        weight: row.get(3)?,
        importance: row.get(4)?,
        importance_manually_set: row.get(5)?,
        created_at,
        last_active,
        activation_count: row.get(8)?,
        split_locked: row.get(9)?,
        split_locked_until,
        origin_type,
        drift_history,
        parent_id: row.get(13)?,
        child_ids,
        summary: row.get(15)?,
        topics,
        tags,
        labels,
        concepts,
        embedding,
        relevance_score: row.get(21)?,
        ratings,
        work_context,
        injection_stats,
    })
}

/// Hebbian reinforcement: for each injected thread that is connected via a bridge,
/// call increment_use() → updates use_count and resets last_reinforced timestamp.
/// This breaks the death spiral: bridges stay alive as long as they are useful.
fn reinforce_used_bridges(
    conn: &Connection,
    injected: &[Thread],
    bridge_connections: &HashMap<String, f64>,
) {
    let mut reinforced = 0u32;
    for thread in injected {
        if bridge_connections.contains_key(&thread.id) {
            // Find the actual bridge IDs to reinforce
            let sql = "SELECT id FROM bridges \
                       WHERE (source_id = ?1 OR target_id = ?1) \
                       AND status IN ('Active', 'Weak')";
            if let Ok(mut stmt) = conn.prepare(sql) {
                let ids: Vec<String> = stmt
                    .query_map(rusqlite::params![thread.id], |row| row.get(0))
                    .ok()
                    .map(|rows| rows.flatten().collect())
                    .unwrap_or_default();
                for bridge_id in ids {
                    if BridgeStorage::increment_use(conn, &bridge_id).is_ok() {
                        reinforced += 1;
                    }
                }
            }
        }
    }
    if reinforced > 0 {
        tracing::info!(bridges_reinforced = reinforced, "Hebbian bridge reinforcement");
    }
}

/// Find the most recently active thread ID.
fn find_most_recent_active_thread(conn: &Connection) -> AiResult<Option<String>> {
    let sql = "SELECT id FROM threads WHERE status = 'Active' ORDER BY last_active DESC LIMIT 1";
    match conn.query_row(sql, [], |row| row.get::<_, String>(0)) {
        Ok(id) => Ok(Some(id)),
        Err(_) => Ok(None),
    }
}

/// Load bridge connections for the active thread.
/// Returns a map of connected thread_id → max bridge weight.
fn load_bridge_connections(
    conn: &Connection,
    active_thread_id: Option<&str>,
) -> AiResult<HashMap<String, f64>> {
    let mut connections = HashMap::new();
    let thread_id = match active_thread_id {
        Some(id) => id,
        None => return Ok(connections),
    };

    // Include both Active AND Weak bridges — Weak bridges get 50% weight reduction.
    // Without this, bridges that decay to Weak become invisible to the validator,
    // creating a death spiral: no visibility → no use → no reinforcement → death.
    let sql = "SELECT source_id, target_id, weight, status FROM bridges \
               WHERE (source_id = ?1 OR target_id = ?1) AND status IN ('Active', 'Weak')";

    let mut stmt = match conn.prepare(sql) {
        Ok(s) => s,
        Err(_) => return Ok(connections),
    };

    let rows = match stmt.query_map(rusqlite::params![thread_id], |row| {
        let source: String = row.get(0)?;
        let target: String = row.get(1)?;
        let weight: f64 = row.get(2)?;
        let status: String = row.get(3)?;
        Ok((source, target, weight, status))
    }) {
        Ok(r) => r,
        Err(_) => return Ok(connections),
    };

    for row in rows {
        if let Ok((source, target, weight, status)) = row {
            // Weak bridges contribute at 50% weight — still visible but reduced influence
            let effective_weight = if status == "Weak" { weight * 0.5 } else { weight };
            let connected = if source == thread_id { target } else { source };
            let entry = connections.entry(connected).or_insert(0.0);
            *entry = entry.max(effective_weight);
        }
    }

    Ok(connections)
}

/// Compute query embedding from text, respecting the configured EmbeddingMode.
/// Returns zero-vec if disabled (V1 validator will score 0, effectively skipped).
fn compute_query_embedding(text: &str, mode: &crate::config::EmbeddingMode) -> Vec<f32> {
    EmbeddingManager::global()
        .embed_with_mode(text, mode)
        .unwrap_or_else(|| vec![0.0f32; EmbeddingManager::global().dimension()])
}

/// Load focus topics from the database.
/// TODO: Implement focus storage — for now returns empty.
fn load_focus_topics(_conn: &Connection) -> Vec<(String, f64)> {
    Vec::new()
}

/// Full-text search fallback when topic/concept index finds nothing.
/// Tokenises the query into words and searches each word individually.
/// For JSON fields (topics, labels, concepts), uses %"word% to match inside arrays.
fn search_threads_by_text(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> AiResult<Vec<Thread>> {
    let words: Vec<&str> = query
        .split_whitespace()
        .filter(|w| w.len() >= 2)
        .collect();

    if words.is_empty() {
        return Ok(Vec::new());
    }

    let mut conditions = Vec::new();
    let mut param_values: Vec<String> = Vec::new();

    for word in &words {
        let lower = word.to_lowercase();
        let idx_plain = param_values.len() + 1;
        param_values.push(format!("%{}%", lower));
        let idx_json = param_values.len() + 1;
        param_values.push(format!("%\"{}\"%", lower));

        conditions.push(format!(
            "(LOWER(title) LIKE ?{idx_plain} OR LOWER(summary) LIKE ?{idx_plain} \
             OR LOWER(topics) LIKE ?{idx_json} OR LOWER(labels) LIKE ?{idx_json} \
             OR LOWER(concepts) LIKE ?{idx_json})"
        ));
    }

    let sql = format!(
        "SELECT id, title, status, weight, importance, importance_manually_set, \
         created_at, last_active, activation_count, split_locked, split_locked_until, \
         origin_type, drift_history, parent_id, child_ids, summary, topics, tags, labels, \
         concepts, embedding, relevance_score, ratings, work_context, injection_stats \
         FROM threads WHERE ({}) ORDER BY last_active DESC LIMIT {}",
        conditions.join(" OR "),
        limit
    );

    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(_) => return Ok(Vec::new()),
    };

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = param_values
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();

    let rows = match stmt.query_map(param_refs.as_slice(), row_to_thread) {
        Ok(r) => r,
        Err(_) => return Ok(Vec::new()),
    };

    let mut threads = Vec::new();
    for row in rows {
        if let Ok(t) = row {
            threads.push(t);
        }
    }
    Ok(threads)
}
