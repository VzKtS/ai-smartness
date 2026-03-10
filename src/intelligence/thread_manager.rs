//! Thread Manager -- thread lifecycle management.
//!
//! All captures create new threads (NewThread only).
//! Thread grouping is handled upstream by changelog shortcut (processor.rs stage 1.5).
//! Continuity is tracked via continuity_parent_id, not thread merging.

use crate::{id_gen, time_utils};
use crate::bridge::{BridgeStatus, BridgeType, ThinkBridge};
use crate::config::{EmbeddingMode, GossipConfig, GuardianConfig, ThreadMatchingConfig};
use crate::constants::*;
use crate::thread::{Thread, ThreadMessage, ThreadStatus, OriginType, WorkContext};
use crate::AiResult;
use crate::intelligence::gossip::Gossip;
use crate::intelligence::metadata_utils::{self, MAX_TOPICS, MAX_LABELS};
use crate::processing::embeddings::EmbeddingManager;
use crate::processing::extractor::{Extraction, ExtractionMode};
use crate::storage::bridges::BridgeStorage;
use crate::storage::concept_index::find_threads_sharing_concepts_db;
use crate::storage::threads::ThreadStorage;
use chrono::Utc;
use rusqlite::Connection;

/// Action decided by the thread manager.
/// All captures create new threads — grouping is handled by changelog shortcut upstream.
#[derive(Debug)]
pub enum ThreadAction {
    NewThread,
}

// LABEL_BLOCKLIST and filter_blocked_labels imported via `use crate::constants::*`

/// Compute short content hash (16 hex chars) for file versioning.
/// Uses SipHash (std DefaultHasher) — CPU-only, ~1μs, no GPU.
fn content_hash(content: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

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
pub fn build_enriched_embed_text_from_thread(thread: &Thread) -> String {
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
        continuity_previous_id: Option<&str>,
        coherence_score: Option<f64>,
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
            continuity_prev = ?continuity_previous_id,
            "ThreadManager: processing input"
        );

        // All captures that reach process_input create their own thread.
        // Thread grouping is handled upstream by changelog shortcut (processor.rs stage 1.5).
        // Continuity is tracked via continuity_parent_id, not thread merging.
        let embeddings = EmbeddingManager::global();
        let embed_mode = &guardian.thread_matching.embedding.mode;

        tracing::info!(action = "NewThread", "Action decided");
        Self::ensure_capacity(conn, thread_quota, embeddings, &guardian.thread_matching)?;
        let id = Self::create_thread(
            conn, extraction, content, source_type, None, file_path, embed_mode,
            continuity_previous_id, coherence_score,
        )?;
        // Backfill continuity_to on previous thread's last message
        if let Some(prev_id) = continuity_previous_id {
            let _ = ThreadStorage::update_last_message_continuity_to(conn, prev_id, &id);
        }
        // Thinkbridges: immediate concept connections
        // Skip for tool threads (file/command/task/fetch) — continuity-only, no gossip
        let normalized = normalize_concepts(&extraction.concepts);
        let is_tool_thread = matches!(source_type,
            "command" | "Command" | "Bash"
            | "Read" | "file_read" | "Write" | "file_write" | "Edit"
            | "Task" | "task" | "WebFetch" | "fetch" | "WebSearch"
            | "NotebookEdit"
        );
        let thinkbridges = if is_tool_thread {
            0
        } else {
            Self::create_thinkbridges(conn, &id, &normalized, &guardian.gossip)?
        };
        if thinkbridges > 0 {
            tracing::info!(thread_id = %id, bridges = thinkbridges, "Thinkbridges");
        }
        Ok(Some(id))
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
        continuity_previous_id: Option<&str>,
        coherence_score: Option<f64>,
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

        // File sources: use file path as title (deterministic, no LLM interpretation)
        let title = match (file_path, source_type) {
            (Some(fp), "Read" | "file_read" | "Write" | "file_write" | "Edit" | "NotebookEdit") => fp.to_string(),
            _ => extraction.title.clone(),
        };

        let thread = Thread {
            id: thread_id.clone(),
            title,
            status: ThreadStatus::Active,
            weight: (extraction.importance + extraction.confidence) / 10.0,
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
            concepts: normalize_concepts(&extraction.concepts),
            embedding,
            relevance_score,
            ratings: vec![],
            work_context,
            injection_stats: None,
            extraction_mode: extraction.extraction_mode.clone(),
            has_truncated_origin: false,
            continuity_parent_id: continuity_previous_id.map(|s| s.to_string()),
            subject_coherence: coherence_score,
            confidence: extraction.confidence,
        };

        ThreadStorage::insert(conn, &thread)?;

        tracing::info!(
            thread_id = %thread_id,
            title = %extraction.title,
            topics = ?extraction.subjects,
            continuity_parent = ?continuity_previous_id,
            subject_coherence = ?coherence_score,
            "Thread created"
        );

        // Add initial message
        // Verbatim sources (prompt/response/web): ALWAYS store original content
        // Tool Summary mode: store file_path reference (summary already in thread.summary)
        // Tool Extract mode: store the original content
        let force_verbatim = matches!(source_type,
            "prompt" | "response" | "WebFetch" | "fetch" | "WebSearch"
        );
        let (msg_source, msg_content, truncated) = if extraction.extraction_mode == ExtractionMode::Summary && !force_verbatim {
            if let Some(fp) = file_path {
                ("reference".to_string(), fp.to_string(), false)
            } else {
                ("summary".to_string(), extraction.summary.clone(), false)
            }
        } else {
            let limit = match source_type {
                "prompt" | "response" => CONTENT_LIMIT_CONVERSATION,
                "WebFetch" | "fetch" | "WebSearch" => CONTENT_LIMIT_WEB,
                _ => CONTENT_LIMIT_DEFAULT,
            };
            let trunc = content.len() > limit;
            let c = if trunc {
                tracing::warn!(thread_id = %thread_id, len = content.len(), limit = limit, "Message truncated");
                truncate_safe(content, limit).to_string()
            } else {
                content.to_string()
            };
            ("capture".to_string(), c, trunc)
        };

        let msg = ThreadMessage {
            thread_id: thread_id.clone(),
            msg_id: id_gen::message_id(),
            content: msg_content,
            source: msg_source,
            source_type: source_type.to_string(),
            timestamp: now,
            metadata: {
                let mut meta = serde_json::Map::new();
                if let Some(fp) = file_path {
                    meta.insert("file_path".to_string(), serde_json::Value::String(fp.to_string()));
                    meta.insert("content_hash".to_string(), serde_json::Value::String(content_hash(content)));
                }
                serde_json::Value::Object(meta)
            },
            is_truncated: truncated,
            continuity_from: None,
            continuity_to: None,
        };
        ThreadStorage::add_message(conn, &msg)?;

        // Propagate truncation flag to thread (sticky — once true, stays true)
        if truncated {
            if let Some(mut t) = ThreadStorage::get(conn, &thread_id)? {
                t.has_truncated_origin = true;
                ThreadStorage::update(conn, &t)?;
            }
        }

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
                shard_concept: None,
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
        source_type: &str,
        file_path: Option<&str>,
        embed_mode: &EmbeddingMode,
    ) -> AiResult<()> {
        let mut thread = match ThreadStorage::get(conn, thread_id)? {
            Some(t) => t,
            None => return Ok(()),
        };

        // Verbatim sources (prompt/response/web): ALWAYS store original content
        // Tool Summary mode: store file_path reference (summary already in thread.summary)
        let force_verbatim = matches!(source_type,
            "prompt" | "response" | "WebFetch" | "fetch" | "WebSearch"
        );
        let (msg_source, msg_content, truncated) = if extraction.extraction_mode == ExtractionMode::Summary && !force_verbatim {
            if let Some(fp) = file_path {
                ("reference".to_string(), fp.to_string(), false)
            } else {
                ("summary".to_string(), extraction.summary.clone(), false)
            }
        } else {
            let limit = match source_type {
                "prompt" | "response" => CONTENT_LIMIT_CONVERSATION,
                "WebFetch" | "fetch" | "WebSearch" => CONTENT_LIMIT_WEB,
                _ => CONTENT_LIMIT_DEFAULT,
            };
            let trunc = content.len() > limit;
            let c = if trunc {
                tracing::warn!(thread_id = %thread_id, len = content.len(), limit = limit, "Message truncated");
                truncate_safe(content, limit).to_string()
            } else {
                content.to_string()
            };
            ("capture".to_string(), c, trunc)
        };

        let msg = ThreadMessage {
            thread_id: thread_id.to_string(),
            msg_id: id_gen::message_id(),
            content: msg_content,
            source: msg_source,
            source_type: source_type.to_string(),
            timestamp: time_utils::now(),
            metadata: serde_json::Value::Object(Default::default()),
            is_truncated: truncated,
            continuity_from: None,
            continuity_to: None,
        };
        ThreadStorage::add_message(conn, &msg)?;

        // Propagate truncation flag (sticky)
        if truncated {
            thread.has_truncated_origin = true;
        }

        // Boost weight
        thread.weight = (thread.weight + THREAD_USE_BOOST).min(1.0);

        // Merge topics (case-insensitive dedup + cap)
        for topic in &extraction.subjects {
            thread.topics.push(topic.clone());
        }
        thread.topics = metadata_utils::dedup_case_insensitive(thread.topics.clone());
        thread.topics.truncate(MAX_TOPICS);

        // Merge labels (case-insensitive dedup + cap, filter blocked)
        for label in &filter_blocked_labels(&extraction.labels) {
            thread.labels.push(label.clone());
        }
        thread.labels = metadata_utils::dedup_case_insensitive(thread.labels.clone());
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

    /// Add a lightweight changelog message to a thread — skips LLM entirely.
    /// Used when a file is already tracked by an existing thread (Read/Write/Edit shortcut).
    /// Returns Some(thread_id) on success, None if thread not found.
    pub fn add_changelog(
        conn: &Connection,
        thread_id: &str,
        file_path: &str,
        source_type: &str,
        content: &str,
        continuity_from: Option<&str>,
        guardian: &GuardianConfig,
    ) -> AiResult<Option<String>> {
        let mut thread = match ThreadStorage::get(conn, thread_id)? {
            Some(t) => t,
            None => return Ok(None),
        };

        // Compute content hash BEFORE any side-effects
        let current_hash = content_hash(content);

        // Find previous hash from last changelog/reference message
        let messages = ThreadStorage::get_messages(conn, thread_id)?;
        let previous_hash = messages
            .iter()
            .rev()
            .find(|m| m.source == "changelog" || m.source == "reference")
            .and_then(|m| m.metadata.get("content_hash").and_then(|v| v.as_str()))
            .map(String::from);

        let changed = previous_hash
            .as_ref()
            .map(|prev| prev != &current_hash)
            .unwrap_or(true); // First changelog = always "changed"

        // Case 2: Same hash → skip total (no changelog, no reactivation, no GPU)
        if !changed {
            tracing::debug!(
                thread_id = %thread_id,
                file_path = %file_path,
                hash = %current_hash,
                "Changelog skip: content unchanged"
            );
            return Ok(Some(thread_id.to_string()));
        }

        // Case 3: Hash differs → append changelog
        // Reactivate if suspended/archived — a modified file is back in focus
        if thread.status != ThreadStatus::Active {
            tracing::info!(
                thread_id = %thread_id,
                old_status = %thread.status,
                "Changelog: reactivating thread"
            );
            Self::reactivate_thread(conn, thread_id)?;
            // Re-fetch after reactivation
            thread = match ThreadStorage::get(conn, thread_id)? {
                Some(t) => t,
                None => return Ok(None),
            };
        }

        let line_count = content.lines().count();

        // --- File Chronicle: LLM extraction on changed content ---
        // Call toolextractor to get a semantic summary of the change.
        // Graceful degradation: if LLM fails, fall back to bare changelog.
        let extraction = if guardian.extraction.llm.enabled {
            use crate::processing::toolextractor;
            match toolextractor::summarize_tool_output(
                content,
                source_type,
                Some(file_path),
                None, // no agent context for changelog (keep it unbiased)
                &guardian.extraction,
                &guardian.local_model_size,
            ) {
                Ok(Some(ext)) => {
                    tracing::info!(
                        thread_id = %thread_id,
                        title = %ext.title,
                        "Changelog: LLM extraction succeeded"
                    );
                    Some(ext)
                }
                Ok(None) => {
                    tracing::debug!(thread_id = %thread_id, "Changelog: LLM returned skip");
                    None
                }
                Err(e) => {
                    tracing::warn!(
                        thread_id = %thread_id,
                        error = %e,
                        "Changelog: LLM extraction failed, falling back to bare changelog"
                    );
                    None
                }
            }
        } else {
            None
        };

        // Build changelog text — rich if extraction available, bare otherwise
        let action = if let Some(ref ext) = extraction {
            format!("[changelog] {} {} — {}", source_type, file_path, ext.summary)
        } else {
            match source_type {
                "Read" | "file_read" => {
                    format!("[changelog] Read {} (modified, {} lines)", file_path, line_count)
                }
                "Write" | "file_write" => {
                    format!("[changelog] Write {} ({} lines)", file_path, line_count)
                }
                "Edit" | "NotebookEdit" => {
                    format!("[changelog] {} {}", source_type, file_path)
                }
                "WebFetch" | "fetch" => {
                    format!("[changelog] WebFetch '{}' (content updated)", file_path)
                }
                "WebSearch" => {
                    format!("[changelog] WebSearch '{}' (results updated)", file_path)
                }
                _ => {
                    format!("[changelog] {} {}", source_type, file_path)
                }
            }
        };

        // Build metadata — include extraction fields when available
        let mut metadata = serde_json::json!({
            "file_path": file_path,
            "content_hash": current_hash,
            "line_count": line_count,
            "action": source_type,
            "changed": changed,
            "previous_hash": previous_hash,
        });
        if let Some(ref ext) = extraction {
            metadata["extracted_title"] = serde_json::json!(ext.title);
            metadata["extracted_topics"] = serde_json::json!(ext.subjects);
            metadata["extracted_labels"] = serde_json::json!(ext.labels);
            metadata["extracted_concepts"] = serde_json::json!(ext.concepts);
            metadata["extracted_summary"] = serde_json::json!(ext.summary);
        }

        // Insert changelog message
        let msg = ThreadMessage {
            thread_id: thread_id.to_string(),
            msg_id: id_gen::message_id(),
            content: action,
            source: "changelog".to_string(),
            source_type: source_type.to_string(),
            timestamp: time_utils::now(),
            metadata,
            is_truncated: false,
            continuity_from: continuity_from.map(|s| s.to_string()),
            continuity_to: None,
        };
        ThreadStorage::add_message(conn, &msg)?;

        // --- Evolve thread metadata from extraction ---
        if let Some(ref ext) = extraction {
            // Summary: always latest (most relevant for recall)
            thread.summary = Some(ext.summary.clone());

            // Topics: union + dedup, capped
            for topic in &ext.subjects {
                if !thread.topics.iter().any(|t| t.eq_ignore_ascii_case(topic)) {
                    thread.topics.push(topic.clone());
                }
            }
            thread.topics.truncate(MAX_TOPICS);

            // Labels: union + dedup, capped
            let new_labels = filter_blocked_labels(&ext.labels);
            for label in &new_labels {
                if !thread.labels.iter().any(|l| l.eq_ignore_ascii_case(label)) {
                    thread.labels.push(label.clone());
                }
            }
            thread.labels.truncate(MAX_LABELS);

            // Concepts: union + normalize → new thinkbridges
            let old_concept_count = thread.concepts.len();
            let mut all_concepts = thread.concepts.clone();
            all_concepts.extend(ext.concepts.clone());
            thread.concepts = normalize_concepts(&all_concepts);

            // Re-embed thread with enriched metadata
            let embed_text = build_enriched_embed_text_from_thread(&thread);
            let embeddings = EmbeddingManager::global();
            thread.embedding = Some(embeddings.embed(&embed_text));

            // Create thinkbridges from NEW concepts only
            let new_concepts: Vec<String> = thread.concepts.iter()
                .skip(old_concept_count)
                .cloned()
                .collect();
            if !new_concepts.is_empty() {
                let bridges = Self::create_thinkbridges(
                    conn, thread_id, &new_concepts, &guardian.gossip,
                )?;
                if bridges > 0 {
                    tracing::info!(
                        thread_id = %thread_id,
                        new_concepts = new_concepts.len(),
                        bridges = bridges,
                        "Changelog: new thinkbridges from evolved concepts"
                    );
                }
            }
        }

        // Half weight boost — gradual growth, weight > 0.6 signals frequent access
        thread.weight = (thread.weight + THREAD_USE_BOOST * 0.5).min(1.0);

        // Update work_context
        let wc = thread.work_context.get_or_insert_with(|| WorkContext {
            files: vec![],
            actions: vec![],
            goal: None,
            updated_at: Utc::now(),
        });
        if !wc.files.contains(&file_path.to_string()) {
            wc.files.push(file_path.to_string());
        }
        if !wc.actions.contains(&source_type.to_string()) {
            wc.actions.push(source_type.to_string());
        }
        wc.updated_at = Utc::now();

        ThreadStorage::update(conn, &thread)?;

        tracing::info!(
            thread_id = %thread_id,
            file_path = %file_path,
            action = %source_type,
            changed = changed,
            hash = %current_hash,
            enriched = extraction.is_some(),
            "Changelog added"
        );

        Ok(Some(thread_id.to_string()))
    }

    /// Create thinkbridges: per-shard concept bridges at thread creation.
    /// Each shared concept between the new thread and an existing thread creates
    /// one bridge. The number of bridges between a pair = connection strength.
    /// Returns the number of bridges created.
    pub fn create_thinkbridges(
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

        // Dynamic limits = max distinct connected threads (not raw bridge count)
        let thread_count = ThreadStorage::count_by_status(conn, &ThreadStatus::Active)?;
        let (max_connected, _) = Gossip::dynamic_limits(thread_count, gossip_config);

        let mut created = 0u32;
        let concept_count = concepts.len().max(1);
        // Weight per shard: equal share so that all shards sum to ~1.0
        let shard_weight = 1.0 / concept_count as f64;

        // Pre-load existing bridges for the new thread (avoid repeated queries)
        let existing = BridgeStorage::list_for_thread(conn, thread_id)?;

        for (candidate_id, shared_concepts, _candidate_total) in &candidates {
            // Min bridges: skip pairs without enough shared concepts to validate the link
            if shared_concepts.len() < gossip_config.min_bridges {
                continue;
            }

            // Check distinct-thread limit on source
            let source_connections = BridgeStorage::count_connected_threads(conn, thread_id)?;
            if source_connections >= max_connected {
                break;
            }

            // Check distinct-thread limit on target
            let target_connections = BridgeStorage::count_connected_threads(conn, candidate_id)?;
            if target_connections >= max_connected {
                continue;
            }

            // Create one bridge per shared concept (shard)
            for concept in shared_concepts {
                // Dedup: skip if shard-bridge already exists for this concept + pair
                let already_exists = existing.iter().any(|b| {
                    (b.source_id == *candidate_id || b.target_id == *candidate_id)
                        && b.shard_concept.as_deref() == Some(concept.as_str())
                        && b.status != BridgeStatus::Invalid
                });
                if already_exists {
                    continue;
                }

                let bridge = ThinkBridge {
                    id: id_gen::bridge_id(),
                    source_id: thread_id.to_string(),
                    target_id: candidate_id.clone(),
                    relation_type: BridgeType::Sibling,
                    reason: format!("thinkbridge:shard({})", concept),
                    shared_concepts: vec![concept.clone()],
                    weight: shard_weight,
                    confidence: shard_weight,
                    status: BridgeStatus::Active,
                    propagated_from: None,
                    propagation_depth: 0,
                    created_by: "thinkbridge".to_string(),
                    use_count: 0,
                    created_at: time_utils::now(),
                    last_reinforced: None,
                    shard_concept: Some(concept.clone()),
                };

                if BridgeStorage::insert(conn, &bridge).is_ok() {
                    created += 1;
                }
            }

            if created > 0 {
                tracing::info!(
                    source = %&thread_id[..8.min(thread_id.len())],
                    target = %&candidate_id[..8.min(candidate_id.len())],
                    shards = shared_concepts.len(),
                    "Thinkbridges created (per-shard)"
                );
            }
        }

        Ok(created)
    }

    /// Invalidate thinkbridges for a thread, protecting high-use bridges.
    /// Then re-create with current concepts. Returns (deleted, created).
    pub fn refresh_thinkbridges(
        conn: &Connection,
        thread_id: &str,
        concepts: &[String],
        gossip_config: &GossipConfig,
    ) -> AiResult<(usize, u32)> {
        let (deleted, protected) =
            BridgeStorage::delete_thinkbridges_for_thread(conn, thread_id, 5)?;

        if deleted > 0 || protected > 0 {
            tracing::info!(
                thread = %&thread_id[..8.min(thread_id.len())],
                deleted = deleted,
                protected = protected,
                "Thinkbridges invalidated"
            );
        }

        let created = Self::create_thinkbridges(conn, thread_id, concepts, gossip_config)?;
        Ok((deleted, created))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::{setup_agent_db, ThreadBuilder};

    #[test]
    fn test_content_hash_deterministic() {
        let h1 = content_hash("hello world");
        let h2 = content_hash("hello world");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 16, "Hash should be 16 hex chars");
    }

    #[test]
    fn test_content_hash_different_for_different_content() {
        let h1 = content_hash("hello world");
        let h2 = content_hash("hello world!");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_add_changelog_to_active_thread() {
        let conn = setup_agent_db();
        let thread = ThreadBuilder::new()
            .id("cl-active")
            .title("Config module")
            .weight(0.5)
            .work_context(vec!["src/config.rs"], vec!["Read"])
            .build();
        ThreadStorage::insert(&conn, &thread).unwrap();

        let result = ThreadManager::add_changelog(
            &conn, "cl-active", "src/config.rs", "Read", "fn main() {}\n", None,
            &GuardianConfig::default(),
        ).unwrap();

        assert_eq!(result, Some("cl-active".to_string()));

        // Check message was inserted
        let messages = ThreadStorage::get_messages(&conn, "cl-active").unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].source, "changelog");
        assert!(messages[0].content.contains("[changelog] Read src/config.rs"));
        assert!(messages[0].metadata.get("content_hash").is_some());
        assert_eq!(messages[0].metadata["changed"], true);

        // Check weight was boosted
        let updated = ThreadStorage::get(&conn, "cl-active").unwrap().unwrap();
        assert!(updated.weight > 0.5, "Weight should be boosted");
    }

    #[test]
    fn test_add_changelog_reactivates_suspended_thread() {
        let conn = setup_agent_db();
        let thread = ThreadBuilder::new()
            .id("cl-suspended")
            .title("Suspended file")
            .status(ThreadStatus::Suspended)
            .weight(0.1)
            .work_context(vec!["src/lib.rs"], vec!["Write"])
            .build();
        ThreadStorage::insert(&conn, &thread).unwrap();

        let result = ThreadManager::add_changelog(
            &conn, "cl-suspended", "src/lib.rs", "Write", "pub mod config;\n", None,
            &GuardianConfig::default(),
        ).unwrap();

        assert_eq!(result, Some("cl-suspended".to_string()));

        let updated = ThreadStorage::get(&conn, "cl-suspended").unwrap().unwrap();
        assert_eq!(updated.status, ThreadStatus::Active, "Thread should be reactivated");
        assert!(updated.weight >= 0.3, "Reactivated thread should have min weight 0.3");
    }

    #[test]
    fn test_add_changelog_reactivates_archived_thread() {
        let conn = setup_agent_db();
        let thread = ThreadBuilder::new()
            .id("cl-archived")
            .title("Archived file")
            .status(ThreadStatus::Archived)
            .weight(0.05)
            .work_context(vec!["src/old.rs"], vec!["Read"])
            .build();
        ThreadStorage::insert(&conn, &thread).unwrap();

        let result = ThreadManager::add_changelog(
            &conn, "cl-archived", "src/old.rs", "Read", "// old code\n", None,
            &GuardianConfig::default(),
        ).unwrap();

        assert_eq!(result, Some("cl-archived".to_string()));

        let updated = ThreadStorage::get(&conn, "cl-archived").unwrap().unwrap();
        assert_eq!(updated.status, ThreadStatus::Active);
    }

    #[test]
    fn test_add_changelog_unchanged_detection() {
        let conn = setup_agent_db();
        let thread = ThreadBuilder::new()
            .id("cl-unchanged")
            .title("Stable file")
            .work_context(vec!["src/stable.rs"], vec!["Read"])
            .build();
        ThreadStorage::insert(&conn, &thread).unwrap();

        let content = "fn stable() { 42 }\n";

        // First read — always "changed" (no previous hash)
        ThreadManager::add_changelog(&conn, "cl-unchanged", "src/stable.rs", "Read", content, None, &GuardianConfig::default()).unwrap();
        let msgs = ThreadStorage::get_messages(&conn, "cl-unchanged").unwrap();
        assert_eq!(msgs[0].metadata["changed"], true);

        // Second read with same content — skip total (no new message)
        let result = ThreadManager::add_changelog(&conn, "cl-unchanged", "src/stable.rs", "Read", content, None, &GuardianConfig::default()).unwrap();
        assert_eq!(result, Some("cl-unchanged".to_string())); // still returns thread_id
        let msgs = ThreadStorage::get_messages(&conn, "cl-unchanged").unwrap();
        assert_eq!(msgs.len(), 1); // no second message — content unchanged
    }

    #[test]
    fn test_add_changelog_changed_detection() {
        let conn = setup_agent_db();
        let thread = ThreadBuilder::new()
            .id("cl-changed")
            .title("Changing file")
            .work_context(vec!["src/evolve.rs"], vec!["Write"])
            .build();
        ThreadStorage::insert(&conn, &thread).unwrap();

        // First write
        ThreadManager::add_changelog(&conn, "cl-changed", "src/evolve.rs", "Write", "v1\n", None, &GuardianConfig::default()).unwrap();

        // Second write with different content
        ThreadManager::add_changelog(&conn, "cl-changed", "src/evolve.rs", "Write", "v2\n", None, &GuardianConfig::default()).unwrap();
        let msgs = ThreadStorage::get_messages(&conn, "cl-changed").unwrap();
        assert_eq!(msgs[1].metadata["changed"], true);
        assert!(msgs[1].content.contains("[changelog] Write"));
    }

    #[test]
    fn test_add_changelog_nonexistent_thread() {
        let conn = setup_agent_db();
        let result = ThreadManager::add_changelog(
            &conn, "nonexistent", "src/main.rs", "Read", "content", None,
            &GuardianConfig::default(),
        ).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_add_changelog_updates_work_context_actions() {
        let conn = setup_agent_db();
        let thread = ThreadBuilder::new()
            .id("cl-actions")
            .title("Action tracking")
            .work_context(vec!["src/main.rs"], vec!["Read"])
            .build();
        ThreadStorage::insert(&conn, &thread).unwrap();

        // Edit action should be added to work_context.actions
        ThreadManager::add_changelog(&conn, "cl-actions", "src/main.rs", "Edit", "diff here", None, &GuardianConfig::default()).unwrap();

        let updated = ThreadStorage::get(&conn, "cl-actions").unwrap().unwrap();
        let wc = updated.work_context.unwrap();
        assert!(wc.actions.contains(&"Read".to_string()));
        assert!(wc.actions.contains(&"Edit".to_string()));
    }
}
