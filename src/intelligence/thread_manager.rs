//! Thread Manager -- thread lifecycle management.
//!
//! Handles: NewThread / Continue / Fork / Reactivate decisions.
//! Called by the daemon processor after extraction + coherence.

use std::collections::HashSet;
use crate::{id_gen, time_utils};
use crate::bridge::{BridgeStatus, BridgeType, ThinkBridge};
use crate::config::{EmbeddingMode, GossipConfig, GuardianConfig};
use crate::constants::*;
use crate::thread::{Thread, ThreadMessage, ThreadStatus, OriginType, WorkContext};
use crate::AiResult;
use crate::intelligence::gossip::Gossip;
use crate::intelligence::merge_metadata::{self, MAX_TOPICS, MAX_LABELS};
use crate::processing::embeddings::EmbeddingManager;
use crate::processing::extractor::Extraction;
use crate::storage::bridges::BridgeStorage;
use crate::storage::concept_index::find_threads_sharing_concepts_db;
use crate::storage::threads::ThreadStorage;
use chrono::Utc;
use rusqlite::Connection;

/// Action decided by the thread manager.
#[derive(Debug)]
pub enum ThreadAction {
    NewThread,
    Continue { thread_id: String },
    Fork { parent_id: String },
    Reactivate { thread_id: String },
}

use crate::config::ThreadMatchingConfig;

// LABEL_BLOCKLIST and filter_blocked_labels imported via `use crate::constants::*`

/// Build enriched embedding text from extraction: title 2x + subjects + labels + concepts (8 max).
fn build_enriched_embed_text(extraction: &Extraction) -> String {
    let concepts_limited: Vec<&str> = extraction.concepts.iter()
        .take(8).map(|s| s.as_str()).collect();
    format!(
        "{} {} {} {} {}",
        extraction.title,
        extraction.title,
        extraction.subjects.join(" "),
        extraction.labels.join(" "),
        concepts_limited.join(" "),
    )
}

/// Build enriched embedding text from an existing thread's metadata.
fn build_enriched_embed_text_from_thread(thread: &Thread) -> String {
    let concepts_limited: Vec<&str> = thread.concepts.iter()
        .take(8).map(|s| s.as_str()).collect();
    format!(
        "{} {} {} {} {}",
        thread.title,
        thread.title,
        thread.topics.join(" "),
        thread.labels.join(" "),
        concepts_limited.join(" "),
    )
}

// truncate_safe imported via `use crate::constants::*`

pub struct ThreadManager;

impl ThreadManager {
    /// Main entry point -- process extracted input into a thread.
    /// Returns the thread_id of the created/updated thread.
    pub fn process_input(
        conn: &Connection,
        extraction: &Extraction,
        content: &str,
        source_type: &str,
        file_path: Option<&str>,
        parent_hint: Option<&str>,
        thread_quota: usize,
        guardian: &GuardianConfig,
    ) -> AiResult<Option<String>> {
        tracing::info!(
            confidence = extraction.confidence,
            source_type = %source_type,
            title = %extraction.title,
            topics = ?extraction.subjects,
            content_len = content.len(),
            thread_quota = thread_quota,
            "ThreadManager: processing input"
        );

        if extraction.confidence == 0.0 {
            return Ok(None);
        }

        let embeddings = EmbeddingManager::global();
        let embed_mode = &guardian.thread_matching.embedding.mode;

        let action = if let Some(parent_id) = parent_hint {
            // Dual gate: coherent (already validated by processor) + similar?
            // If embedding similarity to parent is high enough, CONTINUE in parent.
            // If not similar despite coherence, search for a better match.
            let embed_text = build_enriched_embed_text(extraction);
            let query_emb = embeddings.embed_with_mode(&embed_text, embed_mode);

            let parent_sim = match &query_emb {
                Some(qe) => ThreadStorage::get(conn, parent_id)?
                    .and_then(|p| p.embedding.as_ref().map(|e| embeddings.similarity(qe, e)))
                    .unwrap_or(0.0),
                None => 0.0, // Embeddings disabled → skip similarity
            };

            let tm_config = &guardian.thread_matching;
            tracing::debug!(
                parent_id = %parent_id,
                parent_sim = parent_sim,
                threshold = tm_config.continue_threshold,
                "Dual gate: checking parent similarity"
            );

            if parent_sim >= tm_config.continue_threshold {
                // Coherent AND similar → continue in parent thread
                ThreadAction::Continue { thread_id: parent_id.to_string() }
            } else {
                // Coherent but not similar → search for better match
                Self::decide_action(conn, extraction, embeddings, embed_mode, tm_config)?
            }
        } else {
            Self::decide_action(conn, extraction, embeddings, embed_mode, &guardian.thread_matching)?
        };

        match action {
            ThreadAction::NewThread => {
                tracing::info!(action = "NewThread", "Action decided");
                Self::ensure_capacity(conn, thread_quota, embeddings, &guardian.thread_matching)?;
                let id = Self::create_thread(
                    conn, extraction, content, source_type, None, file_path, embed_mode,
                )?;
                // Birth bridges: immediate concept connections
                let birth = Self::create_birth_bridges(conn, &id, &extraction.concepts, &guardian.gossip)?;
                if birth > 0 {
                    tracing::info!(thread_id = %id, bridges = birth, "Birth bridges");
                }
                Ok(Some(id))
            }
            ThreadAction::Continue { thread_id } => {
                tracing::info!(action = "Continue", thread_id = %thread_id, "Action decided");
                // Detect new concepts before merge
                let old_concepts: HashSet<String> = ThreadStorage::get(conn, &thread_id)?
                    .map(|t| t.concepts.into_iter().collect())
                    .unwrap_or_default();
                Self::update_thread(conn, &thread_id, extraction, content, file_path, embed_mode)?;
                // Birth bridges for NEW concepts only
                let new_concepts: Vec<String> = extraction.concepts.iter()
                    .filter(|c| !old_concepts.contains(*c))
                    .cloned()
                    .collect();
                if !new_concepts.is_empty() {
                    let birth = Self::create_birth_bridges(conn, &thread_id, &new_concepts, &guardian.gossip)?;
                    if birth > 0 {
                        tracing::info!(thread_id = %thread_id, bridges = birth, "Incremental birth bridges");
                    }
                }
                Ok(Some(thread_id))
            }
            ThreadAction::Fork { parent_id } => {
                tracing::info!(action = "Fork", parent_id = %parent_id, "Action decided");
                Self::ensure_capacity(conn, thread_quota, embeddings, &guardian.thread_matching)?;
                let id = Self::create_thread(
                    conn,
                    extraction,
                    content,
                    source_type,
                    Some(&parent_id),
                    file_path,
                    embed_mode,
                )?;
                // Birth bridges: immediate concept connections
                let birth = Self::create_birth_bridges(conn, &id, &extraction.concepts, &guardian.gossip)?;
                if birth > 0 {
                    tracing::info!(thread_id = %id, bridges = birth, "Birth bridges (fork)");
                }
                Ok(Some(id))
            }
            ThreadAction::Reactivate { thread_id } => {
                tracing::info!(action = "Reactivate", thread_id = %thread_id, "Action decided");
                Self::reactivate_thread(conn, &thread_id)?;
                Self::update_thread(conn, &thread_id, extraction, content, file_path, embed_mode)?;
                Ok(Some(thread_id))
            }
        }
    }

    /// Create a new thread from extraction.
    pub fn create_thread(
        conn: &Connection,
        extraction: &Extraction,
        content: &str,
        source_type: &str,
        parent_id: Option<&str>,
        file_path: Option<&str>,
        embed_mode: &EmbeddingMode,
    ) -> AiResult<String> {
        let thread_id = id_gen::thread_id();
        let now = time_utils::now();
        let embeddings = EmbeddingManager::global();

        let work_context = file_path.map(|fp| WorkContext {
            files: vec![fp.to_string()],
            actions: vec![source_type.to_string()],
            goal: None,
            updated_at: now,
        });

        let relevance_score = if extraction.confidence >= 0.8 {
            1.0
        } else {
            0.5 + (extraction.confidence * 0.5)
        };

        let importance = extraction.importance.max(0.5);

        let embed_text = build_enriched_embed_text(extraction);
        let embedding = embeddings.embed_with_mode(&embed_text, embed_mode);

        let origin_type = source_type.parse().unwrap_or(OriginType::Prompt);

        let thread = Thread {
            id: thread_id.clone(),
            title: extraction.title.clone(),
            status: ThreadStatus::Active,
            weight: importance,
            importance,
            importance_manually_set: false,
            created_at: now,
            last_active: now,
            activation_count: 1,
            split_locked: false,
            split_locked_until: None,
            origin_type,
            drift_history: vec![],
            parent_id: parent_id.map(|s| s.to_string()),
            child_ids: vec![],
            summary: Some(extraction.summary.clone()),
            topics: {
                let mut t = extraction.subjects.clone();
                t.truncate(MAX_TOPICS);
                t
            },
            tags: vec![],
            labels: {
                let mut l = filter_blocked_labels(&extraction.labels);
                l.truncate(MAX_LABELS);
                l
            },
            concepts: extraction.concepts.clone(),
            embedding,
            relevance_score,
            ratings: vec![],
            work_context,
            injection_stats: None,
        };

        ThreadStorage::insert(conn, &thread)?;

        tracing::info!(thread_id = %thread_id, title = %extraction.title, topics = ?extraction.subjects, "Thread created");

        // Add initial message (truncate to 2000 chars)
        let truncated = content.len() > 2000;
        let msg_content = if truncated {
            tracing::warn!(thread_id = %thread_id, len = content.len(), "Message truncated to 2000 chars");
            truncate_safe(content, 2000).to_string()
        } else {
            content.to_string()
        };

        let msg = ThreadMessage {
            thread_id: thread_id.clone(),
            msg_id: id_gen::message_id(),
            content: msg_content,
            source: "capture".to_string(),
            source_type: source_type.to_string(),
            timestamp: now,
            metadata: serde_json::Value::Object(Default::default()),
            is_truncated: truncated,
        };
        ThreadStorage::add_message(conn, &msg)?;

        // Create child_of bridge if forked
        if let Some(pid) = parent_id {
            let bridge = ThinkBridge {
                id: id_gen::bridge_id(),
                source_id: thread_id.clone(),
                target_id: pid.to_string(),
                relation_type: BridgeType::ChildOf,
                reason: "fork".to_string(),
                shared_concepts: extraction.subjects.clone(),
                weight: 0.8,
                confidence: extraction.confidence,
                status: BridgeStatus::Active,
                propagated_from: None,
                propagation_depth: 0,
                created_by: "thread_manager".to_string(),
                use_count: 0,
                created_at: now,
                last_reinforced: None,
            };
            BridgeStorage::insert(conn, &bridge)?;
        }

        Ok(thread_id)
    }

    /// Update an existing thread with new content.
    pub fn update_thread(
        conn: &Connection,
        thread_id: &str,
        extraction: &Extraction,
        content: &str,
        file_path: Option<&str>,
        embed_mode: &EmbeddingMode,
    ) -> AiResult<()> {
        let mut thread = match ThreadStorage::get(conn, thread_id)? {
            Some(t) => t,
            None => return Ok(()),
        };

        let truncated = content.len() > 2000;
        let msg_content = if truncated {
            tracing::warn!(thread_id = %thread_id, len = content.len(), "Message truncated to 2000 chars");
            truncate_safe(content, 2000).to_string()
        } else {
            content.to_string()
        };

        let msg = ThreadMessage {
            thread_id: thread_id.to_string(),
            msg_id: id_gen::message_id(),
            content: msg_content,
            source: "capture".to_string(),
            source_type: "update".to_string(),
            timestamp: time_utils::now(),
            metadata: serde_json::Value::Object(Default::default()),
            is_truncated: truncated,
        };
        ThreadStorage::add_message(conn, &msg)?;

        // Boost weight
        thread.weight = (thread.weight + THREAD_USE_BOOST).min(1.0);

        // Merge topics (case-insensitive dedup + cap)
        for topic in &extraction.subjects {
            thread.topics.push(topic.clone());
        }
        thread.topics = merge_metadata::dedup_case_insensitive(thread.topics.clone());
        thread.topics.truncate(MAX_TOPICS);

        // Merge labels (case-insensitive dedup + cap, filter blocked)
        for label in &filter_blocked_labels(&extraction.labels) {
            thread.labels.push(label.clone());
        }
        thread.labels = merge_metadata::dedup_case_insensitive(thread.labels.clone());
        thread.labels.truncate(MAX_LABELS);

        // Update work context
        if let Some(fp) = file_path {
            let wc = thread.work_context.get_or_insert_with(|| WorkContext {
                files: vec![],
                actions: vec![],
                goal: None,
                updated_at: Utc::now(),
            });
            if !wc.files.contains(&fp.to_string()) {
                wc.files.push(fp.to_string());
            }
            wc.updated_at = Utc::now();
        }

        // Update embedding with enriched text (title 2x + topics + labels + concepts)
        let embeddings = EmbeddingManager::global();
        let embed_text = build_enriched_embed_text_from_thread(&thread);
        if let Some(emb) = embeddings.embed_with_mode(&embed_text, embed_mode) {
            thread.embedding = Some(emb);
        }

        // Auto-score importance
        let msg_count = ThreadStorage::message_count(conn, thread_id)?;
        Self::auto_score_importance(&mut thread, msg_count);

        ThreadStorage::update(conn, &thread)?;

        tracing::info!(thread_id = %thread_id, weight = thread.weight, "Thread updated");

        Ok(())
    }

    /// Decide what action to take based on similarity search.
    fn decide_action(
        conn: &Connection,
        extraction: &Extraction,
        embeddings: &EmbeddingManager,
        embed_mode: &EmbeddingMode,
        config: &ThreadMatchingConfig,
    ) -> AiResult<ThreadAction> {
        let embed_text = build_enriched_embed_text(extraction);
        let query_emb = match embeddings.embed_with_mode(&embed_text, embed_mode) {
            Some(emb) => emb,
            None => {
                // Embeddings disabled → always create new thread
                tracing::debug!("Embeddings disabled, creating new thread");
                return Ok(ThreadAction::NewThread);
            }
        };

        // Search active threads
        let active = ThreadStorage::list_active(conn)?;
        tracing::debug!(candidates = active.len(), "ThreadManager: searching active threads");
        let mut best_sim = 0.0f64;
        let mut best_id = None;

        for thread in &active {
            if let Some(ref emb) = thread.embedding {
                let sim = embeddings.similarity(&query_emb, emb);
                if sim > best_sim {
                    best_sim = sim;
                    best_id = Some(thread.id.clone());
                }
            }
        }

        tracing::debug!(best_sim = best_sim, threshold = config.continue_threshold, "Active similarity search");

        if best_sim >= config.continue_threshold {
            if let Some(id) = best_id {
                tracing::debug!(action = "Continue", thread_id = %id, similarity = best_sim, "Decided by similarity");
                return Ok(ThreadAction::Continue { thread_id: id });
            }
        }

        // Search suspended threads for reactivation
        let suspended = ThreadStorage::list_by_status(conn, &ThreadStatus::Suspended)?;
        tracing::debug!(candidates = suspended.len(), "ThreadManager: searching suspended threads");
        let mut best_susp_sim = 0.0f64;
        let mut best_susp_id = None;

        for thread in &suspended {
            if let Some(ref emb) = thread.embedding {
                let sim = embeddings.similarity(&query_emb, emb);
                if sim > best_susp_sim {
                    best_susp_sim = sim;
                    best_susp_id = Some(thread.id.clone());
                }
            }
        }

        tracing::debug!(best_susp_sim = best_susp_sim, threshold = config.reactivate_threshold, "Suspended similarity search");

        if best_susp_sim >= config.reactivate_threshold {
            if let Some(id) = best_susp_id {
                tracing::debug!(action = "Reactivate", thread_id = %id, similarity = best_susp_sim, "Decided by similarity");
                return Ok(ThreadAction::Reactivate { thread_id: id });
            }
        }

        tracing::debug!(action = "NewThread", "No similar thread found, creating new");
        Ok(ThreadAction::NewThread)
    }

    /// Ensure capacity for a new thread by merging or suspending.
    fn ensure_capacity(
        conn: &Connection,
        thread_quota: usize,
        embeddings: &EmbeddingManager,
        config: &ThreadMatchingConfig,
    ) -> AiResult<()> {
        let count = ThreadStorage::count_by_status(conn, &ThreadStatus::Active)?;
        tracing::debug!(active = count, quota = thread_quota, "ThreadManager: capacity check");
        if count < thread_quota {
            return Ok(());
        }

        tracing::info!(active = count, quota = thread_quota, "ThreadManager: at capacity, need to free slot");
        let active = ThreadStorage::list_active(conn)?;

        // Strategy 1: Find most similar pair for auto-merge
        let mut best_pair_sim = 0.0f64;
        let mut merge_target = None;

        for (i, a) in active.iter().enumerate() {
            if let Some(ref emb_a) = a.embedding {
                for b in active.iter().skip(i + 1) {
                    if let Some(ref emb_b) = b.embedding {
                        let sim = embeddings.similarity(emb_a, emb_b);
                        if sim > best_pair_sim {
                            best_pair_sim = sim;
                            // Suspend the lower-weight one
                            if a.weight <= b.weight {
                                merge_target = Some(a.id.clone());
                            } else {
                                merge_target = Some(b.id.clone());
                            }
                        }
                    }
                }
            }
        }

        if best_pair_sim >= config.capacity_suspend_threshold {
            if let Some(id) = merge_target {
                tracing::warn!(thread_id = %id, similarity = best_pair_sim, reason = "capacity_merge", "Thread suspended for capacity");
                ThreadStorage::update_status(conn, &id, ThreadStatus::Suspended)?;
                return Ok(());
            }
        }

        // Strategy 2: Suspend lightest thread
        if let Some(lightest) = active.iter().min_by(|a, b| {
            a.weight
                .partial_cmp(&b.weight)
                .unwrap_or(std::cmp::Ordering::Equal)
        }) {
            tracing::warn!(thread_id = %lightest.id, weight = lightest.weight, reason = "capacity_lightest", "Thread suspended for capacity");
            ThreadStorage::update_status(conn, &lightest.id, ThreadStatus::Suspended)?;
        }

        Ok(())
    }

    /// Reactivate a suspended/archived thread.
    pub fn reactivate_thread(conn: &Connection, id: &str) -> AiResult<()> {
        if let Some(mut thread) = ThreadStorage::get(conn, id)? {
            thread.status = ThreadStatus::Active;
            thread.weight = thread.weight.max(0.3);
            thread.activation_count += 1;
            thread.last_active = time_utils::now();
            ThreadStorage::update(conn, &thread)?;
            tracing::info!(thread_id = %id, "Thread reactivated");
        }
        Ok(())
    }

    /// Auto-score importance based on message count, labels, activation.
    fn auto_score_importance(thread: &mut Thread, msg_count: usize) {
        if thread.importance_manually_set {
            return;
        }

        let mut score = thread.importance.max(0.5);

        if msg_count > 7 {
            score = score.max(0.7);
        } else if msg_count > 3 {
            score = score.max(0.6);
        }

        if thread.activation_count > 3 {
            score += 0.1;
        }

        let high_labels = ["architecture", "security", "bug-fix", "performance"];
        let low_labels = ["joke", "social"];

        if thread
            .labels
            .iter()
            .any(|l| high_labels.contains(&l.as_str()))
        {
            score += 0.1;
        }
        if thread
            .labels
            .iter()
            .any(|l| low_labels.contains(&l.as_str()))
        {
            score -= 0.1;
        }

        if let Some(ref wc) = thread.work_context {
            score += wc.importance_boost();
        }

        thread.importance = score.clamp(0.0, 1.0);
    }

    /// Enforce a thread quota by suspending excess threads (least important first).
    /// Called when an agent's ThreadMode is lowered. Returns count of suspended threads.
    pub fn enforce_quota(conn: &Connection, new_quota: usize) -> AiResult<usize> {
        let active_count = ThreadStorage::count_by_status(conn, &ThreadStatus::Active)?;
        if active_count <= new_quota {
            tracing::debug!(active = active_count, quota = new_quota, "Quota OK, no threads to suspend");
            return Ok(0);
        }

        let excess = active_count - new_quota;
        tracing::info!(active = active_count, quota = new_quota, excess = excess, "Enforcing quota: suspending excess threads");

        let mut active = ThreadStorage::list_active(conn)?;
        // Sort by importance ASC, then weight ASC (least important first)
        active.sort_by(|a, b| {
            a.importance
                .partial_cmp(&b.importance)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(
                    a.weight
                        .partial_cmp(&b.weight)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
        });

        let to_suspend: Vec<String> = active.iter().take(excess).map(|t| t.id.clone()).collect();
        let suspended = ThreadStorage::update_status_batch(conn, &to_suspend, ThreadStatus::Suspended)?;

        tracing::info!(suspended = suspended, "Quota enforced: threads suspended");
        Ok(suspended)
    }

    /// Archive a thread (legacy interface).
    pub fn archive_thread(conn: &Connection, id: &str) -> AiResult<()> {
        ThreadStorage::update_status(conn, id, ThreadStatus::Archived)
    }

    /// Create birth bridges: immediate concept-based connections at thread creation.
    /// Connects the new/updated thread to existing threads sharing concepts.
    /// Returns the number of bridges created.
    fn create_birth_bridges(
        conn: &Connection,
        thread_id: &str,
        concepts: &[String],
        gossip_config: &GossipConfig,
    ) -> AiResult<u32> {
        if concepts.is_empty() {
            return Ok(0);
        }

        let candidates = find_threads_sharing_concepts_db(conn, concepts, Some(thread_id))?;
        if candidates.is_empty() {
            return Ok(0);
        }

        // Use same dynamic limits as gossip
        let thread_count = ThreadStorage::count_by_status(conn, &ThreadStatus::Active)?;
        let (max_per, _) = Gossip::dynamic_limits(thread_count, gossip_config);

        let mut created = 0u32;
        let new_concept_count = concepts.len();

        for (candidate_id, shared_concepts, candidate_total) in &candidates {
            if created as usize >= max_per {
                break;
            }

            // Dedup: skip if non-Invalid bridge already exists (allows rebirth of decayed connections)
            let existing = BridgeStorage::list_for_thread(conn, thread_id)?;
            let already_linked = existing.iter().any(|b| {
                (b.source_id == *candidate_id || b.target_id == *candidate_id)
                    && b.status != BridgeStatus::Invalid
            });
            if already_linked {
                continue;
            }

            // Check bridge limit on both source AND target
            if existing.len() >= max_per {
                break;
            }
            let target_bridges = BridgeStorage::list_for_thread(conn, candidate_id)?;
            if target_bridges.len() >= max_per {
                continue;
            }

            // Compute weight: overlap_ratio * 0.5 + richness * 0.5
            let shared_count = shared_concepts.len();
            let min_concepts = new_concept_count.min(*candidate_total).max(1);
            let overlap_ratio = shared_count as f64 / min_concepts as f64;
            let richness = (shared_count as f64 / 5.0).min(1.0);
            let weight = overlap_ratio * 0.5 + richness * 0.5;

            if weight < 0.20 {
                continue;
            }

            let bridge = ThinkBridge {
                id: id_gen::bridge_id(),
                source_id: thread_id.to_string(),
                target_id: candidate_id.clone(),
                relation_type: BridgeType::Sibling,
                reason: format!("birth:concept_overlap({},ratio={:.2})", shared_count, overlap_ratio),
                shared_concepts: shared_concepts.clone(),
                weight,
                confidence: weight,
                status: BridgeStatus::Active,
                propagated_from: None,
                propagation_depth: 0,
                created_by: "birth".to_string(),
                use_count: 0,
                created_at: time_utils::now(),
                last_reinforced: None,
            };

            if BridgeStorage::insert(conn, &bridge).is_ok() {
                tracing::info!(
                    source = %&thread_id[..8.min(thread_id.len())],
                    target = %&candidate_id[..8.min(candidate_id.len())],
                    shared = shared_count,
                    weight = format!("{:.3}", weight).as_str(),
                    "Birth bridge created"
                );
                created += 1;
            }
        }

        Ok(created)
    }
}
