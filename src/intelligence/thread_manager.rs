//! Thread Manager -- thread lifecycle management.
//!
//! Handles: NewThread / Continue / Fork / Reactivate decisions.
//! Called by the daemon processor after extraction + coherence.

use crate::{id_gen, time_utils};
use crate::bridge::{BridgeStatus, BridgeType, ThinkBridge};
use crate::constants::*;
use crate::thread::{Thread, ThreadMessage, ThreadStatus, OriginType, WorkContext};
use crate::AiResult;
use crate::processing::embeddings::EmbeddingManager;
use crate::processing::extractor::Extraction;
use crate::storage::bridges::BridgeStorage;
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

const CONTINUE_THRESHOLD: f64 = 0.25;
const REACTIVATE_THRESHOLD: f64 = 0.50;
const AUTO_MERGE_THRESHOLD: f64 = 0.85;

/// Labels that are too generic to carry semantic value — filtered out before storage.
const LABEL_BLOCKLIST: &[&str] = &[
    "action", "decision", "metadata", "empty", "search result",
    "no matches", "empty result", "file-listing", "directory-listing",
    "grep-output", "search-config", "build-output", "code-snippet",
];

fn filter_blocked_labels(labels: &[String]) -> Vec<String> {
    labels
        .iter()
        .filter(|l| {
            !LABEL_BLOCKLIST
                .iter()
                .any(|blocked| l.to_lowercase() == *blocked)
        })
        .cloned()
        .collect()
}

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

        let action = if let Some(parent_id) = parent_hint {
            ThreadAction::Fork {
                parent_id: parent_id.to_string(),
            }
        } else {
            Self::decide_action(conn, extraction, embeddings)?
        };

        match action {
            ThreadAction::NewThread => {
                tracing::info!(action = "NewThread", "Action decided");
                Self::ensure_capacity(conn, thread_quota, embeddings)?;
                let id = Self::create_thread(
                    conn, extraction, content, source_type, None, file_path,
                )?;
                Ok(Some(id))
            }
            ThreadAction::Continue { thread_id } => {
                tracing::info!(action = "Continue", thread_id = %thread_id, "Action decided");
                Self::update_thread(conn, &thread_id, extraction, content, file_path)?;
                Ok(Some(thread_id))
            }
            ThreadAction::Fork { parent_id } => {
                tracing::info!(action = "Fork", parent_id = %parent_id, "Action decided");
                Self::ensure_capacity(conn, thread_quota, embeddings)?;
                let id = Self::create_thread(
                    conn,
                    extraction,
                    content,
                    source_type,
                    Some(&parent_id),
                    file_path,
                )?;
                Ok(Some(id))
            }
            ThreadAction::Reactivate { thread_id } => {
                tracing::info!(action = "Reactivate", thread_id = %thread_id, "Action decided");
                Self::reactivate_thread(conn, &thread_id)?;
                Self::update_thread(conn, &thread_id, extraction, content, file_path)?;
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

        let embed_text = format!("{} {}", extraction.title, extraction.subjects.join(" "));
        let embedding = embeddings.embed(&embed_text);

        let origin_type = source_type.parse().unwrap_or(OriginType::Prompt);

        let thread = Thread {
            id: thread_id.clone(),
            title: extraction.title.clone(),
            status: ThreadStatus::Active,
            weight: 1.0,
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
            topics: extraction.subjects.clone(),
            tags: vec![],
            labels: filter_blocked_labels(&extraction.labels),
            concepts: extraction.concepts.clone(),
            embedding: Some(embedding),
            relevance_score,
            ratings: vec![],
            work_context,
            injection_stats: None,
        };

        ThreadStorage::insert(conn, &thread)?;

        tracing::info!(thread_id = %thread_id, title = %extraction.title, topics = ?extraction.subjects, "Thread created");

        // Add initial message (truncate to 2000 chars)
        let msg_content = if content.len() > 2000 {
            content[..2000].to_string()
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
    ) -> AiResult<()> {
        let mut thread = match ThreadStorage::get(conn, thread_id)? {
            Some(t) => t,
            None => return Ok(()),
        };

        let msg_content = if content.len() > 2000 {
            content[..2000].to_string()
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
        };
        ThreadStorage::add_message(conn, &msg)?;

        // Boost weight
        thread.weight = (thread.weight + THREAD_USE_BOOST).min(1.0);

        // Merge topics (dedup)
        for topic in &extraction.subjects {
            if !thread.topics.iter().any(|t| t == topic) {
                thread.topics.push(topic.clone());
            }
        }

        // Merge labels (preserve existing, filter blocked)
        for label in &filter_blocked_labels(&extraction.labels) {
            if !thread.labels.iter().any(|l| l == label) {
                thread.labels.push(label.clone());
            }
        }

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

        // Update embedding
        let embeddings = EmbeddingManager::global();
        let embed_text = format!("{} {}", thread.title, thread.topics.join(" "));
        thread.embedding = Some(embeddings.embed(&embed_text));

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
    ) -> AiResult<ThreadAction> {
        let embed_text = format!("{} {}", extraction.title, extraction.subjects.join(" "));
        let query_emb = embeddings.embed(&embed_text);

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

        tracing::debug!(best_sim = best_sim, threshold = CONTINUE_THRESHOLD, "Active similarity search");

        if best_sim >= CONTINUE_THRESHOLD {
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

        tracing::debug!(best_susp_sim = best_susp_sim, threshold = REACTIVATE_THRESHOLD, "Suspended similarity search");

        if best_susp_sim >= REACTIVATE_THRESHOLD {
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

        if best_pair_sim >= AUTO_MERGE_THRESHOLD {
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
}
