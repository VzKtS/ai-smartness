//! Capture processor — clean, extract, and manage threads from tool/prompt output.
//!
//! Called by the IPC server for each tool_capture and prompt_capture request.
//! Maintains a PendingContext for coherence-based child linking.
//!
//! Pipeline: clean → extract (LLM) → coherence gate → thread management.

use std::time::Instant;

use ai_smartness::AiResult;
use ai_smartness::config::GuardianConfig;
use ai_smartness::constants::truncate_safe;
use ai_smartness::intelligence::thread_manager::ThreadManager;
use ai_smartness::processing::cleaner;
use ai_smartness::processing::coherence::{self, CoherenceAction};
use ai_smartness::processing::extractor::{self, ExtractionSource};
use ai_smartness::processing::toolextractor;
use ai_smartness::storage::threads::ThreadStorage;
use rusqlite::Connection;

/// Pending context for coherence-based child linking.
/// Stores the last capture's content + thread_id.
/// Expires after config.extraction.pending_context_ttl_secs (default: 10 minutes).
pub struct PendingContext {
    pub content: String,
    pub thread_id: String,
    pub labels: Vec<String>,
    pub coherence_score: Option<f64>,
    pub timestamp: Instant,
    /// Extraction metadata for subject-aware coherence
    pub title: String,
    pub subjects: Vec<String>,
    pub concepts: Vec<String>,
    pub summary: Option<String>,
}

impl PendingContext {
    pub fn is_expired(&self, ttl: u64) -> bool {
        self.timestamp.elapsed().as_secs() > ttl
    }
}

/// Process a tool capture: clean -> extract -> coherence gate -> thread management.
/// Returns the thread_id of the created/updated thread, or None if skipped.
pub fn process_capture(
    conn: &Connection,
    pending: &mut Option<PendingContext>,
    source_type: &str,
    content: &str,
    file_path: Option<&str>,
    thread_quota: usize,
    guardian: &GuardianConfig,
) -> AiResult<Option<String>> {
    let pipeline_start = Instant::now();
    tracing::info!(
        source = source_type,
        content_len = content.len(),
        file_path = ?file_path,
        thread_quota = thread_quota,
        "Pipeline START — processing capture"
    );

    // 1. Clean content
    let cleaned = cleaner::clean_tool_output(content);
    tracing::debug!(
        original_len = content.len(),
        cleaned_len = cleaned.len(),
        min_capture = guardian.extraction.min_capture_length,
        "Stage 1/6: Content cleaned"
    );
    if !cleaner::should_capture_with_config(&cleaned, guardian.extraction.min_capture_length) {
        tracing::info!(
            cleaned_len = cleaned.len(),
            min_required = guardian.extraction.min_capture_length,
            elapsed_ms = pipeline_start.elapsed().as_millis(),
            "Pipeline DROP at stage 1 — noise filter"
        );
        return Ok(None);
    }

    // Stage 1.5: Changelog shortcut for known files (Read/Write/Edit)
    // 3-case logic:  1) no file_path → full LLM   2) same hash → skip total   3) diff hash → changelog
    if let Some(fp) = file_path {
        if is_file_tool_source(source_type) {
            match try_changelog_shortcut(conn, pending, source_type, fp, &cleaned, guardian) {
                Ok(Some(thread_id)) => {
                    tracing::info!(
                        thread_id = %thread_id,
                        file_path = %fp,
                        total_ms = pipeline_start.elapsed().as_millis(),
                        "Pipeline SHORTCUT — file tracked (skipped LLM)"
                    );
                    return Ok(Some(thread_id));
                }
                Ok(None) => {
                    // No existing thread for this file → fall through to full pipeline
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        file_path = %fp,
                        "Changelog shortcut failed, falling through to full pipeline"
                    );
                }
            }
        }
    }

    // Build agent context from PendingContext (recent activity) for importance scoring
    let ttl = guardian.extraction.pending_context_ttl_secs;
    let agent_context = pending
        .as_ref()
        .filter(|p| !p.is_expired(ttl))
        .map(|ctx| ctx.content.as_str());

    // 2. Extract metadata — branch: tool pipeline vs human exchange pipeline
    let extraction = if is_tool_source(source_type) {
        // Tool pipeline: single-pass summary via toolextractor (faster, shorter prompt)
        tracing::info!(
            source = source_type,
            cleaned_len = cleaned.len(),
            has_agent_context = agent_context.is_some(),
            elapsed_ms = pipeline_start.elapsed().as_millis(),
            "Stage 2/6: Starting TOOL extraction (single-pass summary)"
        );
        match toolextractor::summarize_tool_output(
            &cleaned,
            source_type,
            file_path,
            agent_context,
            &guardian.extraction,
        )? {
            Some(e) => e,
            None => {
                tracing::info!(
                    elapsed_ms = pipeline_start.elapsed().as_millis(),
                    "Pipeline DROP at stage 2 — tool extraction skip or failure"
                );
                return Ok(None);
            }
        }
    } else {
        // Human exchange pipeline: full extractor.rs (prompt/response)
        let source = match source_type {
            "Read" | "file_read" => ExtractionSource::FileRead,
            "Write" | "file_write" => ExtractionSource::FileWrite,
            "Task" | "task" => ExtractionSource::Task,
            "Fetch" | "fetch" => ExtractionSource::Fetch,
            "Response" | "response" => ExtractionSource::Response,
            "Command" | "command" => ExtractionSource::Command,
            _ => ExtractionSource::Prompt,
        };

        // Phase B: Gate LLM relevance for short prompts (configurable, default OFF)
        if source_type == "prompt" && guardian.extraction.enable_prompt_relevance_gate {
            let char_count = cleaned.chars().count();
            if char_count <= guardian.extraction.prompt_relevance_gate_max_chars {
                match check_prompt_relevance(&cleaned, agent_context, guardian) {
                    Ok(true) => {
                        tracing::info!(chars = char_count, "Short prompt judged RELEVANT by gate LLM");
                    }
                    Ok(false) => {
                        tracing::info!(chars = char_count, "Short prompt judged NOT relevant, dropping");
                        return Ok(None);
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Relevance gate LLM failed, proceeding normally");
                    }
                }
            }
        }

        tracing::info!(
            source = source.as_str(),
            cleaned_len = cleaned.len(),
            has_agent_context = agent_context.is_some(),
            elapsed_ms = pipeline_start.elapsed().as_millis(),
            "Stage 2/6: Starting LLM extraction (human exchange)"
        );
        match extractor::extract(
            &cleaned,
            source,
            &guardian.extraction,
            &guardian.label_suggestion,
            &guardian.importance_rating,
            agent_context,
        )? {
            Some(e) => e,
            None => {
                tracing::info!(
                    elapsed_ms = pipeline_start.elapsed().as_millis(),
                    "Pipeline DROP at stage 2 — LLM skip or failure"
                );
                return Ok(None);
            }
        }
    };
    tracing::info!(
        title = %extraction.title,
        confidence = extraction.confidence,
        importance = extraction.importance,
        topics = ?extraction.subjects,
        labels = ?extraction.labels,
        n_concepts = extraction.concepts.len(),
        elapsed_ms = pipeline_start.elapsed().as_millis(),
        "Stage 2/6: Extraction complete"
    );

    // 3. Confidence quality gate
    if extraction.confidence == 0.0 {
        tracing::info!(
            elapsed_ms = pipeline_start.elapsed().as_millis(),
            "Pipeline DROP at stage 3 — confidence=0"
        );
        return Ok(None);
    }
    tracing::debug!(confidence = extraction.confidence, "Stage 3/6: Confidence gate passed");

    // 4. Coherence gate — subject-aware comparison with pending context
    //    Uses extraction metadata (title, subjects, concepts) instead of raw text.
    //    Returns: continuity_previous_id, coherence_score.
    let coherence_cfg = &guardian.coherence;
    let (continuity_previous_id, coherence_score): (Option<String>, Option<f64>) =
        if !coherence_cfg.enabled {
            tracing::debug!("Coherence gate disabled, skipping coherence check");
            (None, None)
        } else if let Some(ctx) = pending.as_ref().filter(|p| !p.is_expired(ttl)) {
            let coherence_input = coherence::CoherenceInput {
                prev_title: &ctx.title,
                prev_subjects: &ctx.subjects,
                prev_concepts: &ctx.concepts,
                prev_labels: &ctx.labels,
                prev_content: &ctx.content,
                new_title: &extraction.title,
                new_subjects: &extraction.subjects,
                new_concepts: &extraction.concepts,
                new_content: &cleaned,
            };

            let coherence_result = coherence::check_coherence(
                &coherence_input,
                coherence_cfg,
            )?;

            let action = coherence::determine_action(
                coherence_result.score,
                coherence_cfg.child_threshold,
            );

            match action {
                CoherenceAction::Child => {
                    tracing::debug!(
                        score = coherence_result.score,
                        parent = %ctx.thread_id,
                        "Coherence: Child — continuity edge created"
                    );
                    // parent_hint NEVER set — coherence drives continuity edges, not thread grouping.
                    // Thread grouping is handled by changelog shortcut (stage 1.5) only.
                    (Some(ctx.thread_id.clone()), Some(coherence_result.score))
                }
                CoherenceAction::Orphan => {
                    tracing::debug!(
                        score = coherence_result.score,
                        "Coherence: Orphan — subject changed, chain breaks"
                    );
                    (None, None)
                }
            }
        } else {
            (None, None)
        };

    // 5. Thread management (always NewThread — grouping handled by changelog shortcut)
    tracing::info!(
        continuity = ?continuity_previous_id,
        elapsed_ms = pipeline_start.elapsed().as_millis(),
        "Stage 5/6: Thread management"
    );
    let thread_id = ThreadManager::process_input(
        conn,
        &extraction,
        &cleaned,
        source_type,
        file_path,
        continuity_previous_id.as_deref(),
        coherence_score,
        thread_quota,
        guardian,
    )?;

    tracing::info!(
        thread_id = ?thread_id,
        elapsed_ms = pipeline_start.elapsed().as_millis(),
        "Stage 5/6: Thread management complete"
    );

    // 6. Update pending context for next capture
    if let Some(ref tid) = thread_id {
        let labels = ThreadStorage::get(conn, tid)?
            .map(|t| t.labels)
            .unwrap_or_default();
        *pending = Some(PendingContext {
            content: truncate_safe(&cleaned, 4000).to_string(),
            thread_id: tid.clone(),
            labels: labels.clone(),
            coherence_score,
            timestamp: Instant::now(),
            title: extraction.title.clone(),
            subjects: extraction.subjects.clone(),
            concepts: extraction.concepts.clone(),
            summary: Some(extraction.summary.clone()),
        });
        tracing::debug!(
            thread_id = %tid,
            labels = ?labels,
            "Stage 6/6: Pending context updated"
        );
    }

    tracing::info!(
        source = source_type,
        thread_id = ?thread_id,
        total_ms = pipeline_start.elapsed().as_millis(),
        "Pipeline COMPLETE"
    );

    Ok(thread_id)
}

/// Process a prompt capture (user message).
pub fn process_prompt(
    conn: &Connection,
    pending: &mut Option<PendingContext>,
    prompt: &str,
    _session_id: Option<&str>,
    thread_quota: usize,
    guardian: &GuardianConfig,
) -> AiResult<Option<String>> {
    tracing::info!(prompt_len = prompt.len(), "Processing prompt capture");

    // Phase A safety net: reject prompts below configured min length (defence in depth)
    if prompt.chars().count() < guardian.capture.min_prompt_length {
        tracing::debug!(chars = prompt.chars().count(), min = guardian.capture.min_prompt_length, "Prompt too short, skipping");
        return Ok(None);
    }

    process_capture(conn, pending, "prompt", prompt, None, thread_quota, guardian)
}

/// Check if a source_type corresponds to a tool capture (vs human exchange).
/// Tool captures use the toolextractor pipeline (single-pass summary).
/// Human exchanges (prompt/response) use the full extractor pipeline.
fn is_tool_source(source_type: &str) -> bool {
    matches!(
        source_type,
        "Read" | "file_read" | "Write" | "file_write" | "Edit"
            | "Bash" | "Command" | "command"
            | "WebFetch" | "fetch" | "WebSearch"
            | "Task" | "task"
            | "NotebookEdit"
    )
}

/// Gate LLM for short prompts (51-150 chars).
/// Judges whether the prompt has extractive value relative to current context.
///
/// Procedural order: raw content FIRST, context at END.
/// Consistent with the Thinkbridge pipeline — the LLM judges content
/// before being influenced by existing context.
fn check_prompt_relevance(
    prompt: &str,
    agent_context: Option<&str>,
    _guardian: &GuardianConfig,
) -> ai_smartness::AiResult<bool> {
    let context_block = match agent_context {
        Some(ctx) if !ctx.is_empty() => format!(
            "\n\nRecent agent context:\n---\n{}\n---",
            truncate_safe(ctx, 500)
        ),
        _ => String::new(),
    };

    let gate_prompt = format!(
        r#"Evaluate whether this user message contains extractable information (concepts, decisions, facts, intentions).

Message:
"{}"
{}

Reply ONLY with JSON: {{"relevant": true}} or {{"relevant": false}}
- true = contains at least one concept, fact, decision or actionable intention
- false = purely procedural message, confirmation, acknowledgment, or conversational noise

Examples false: "yes that's good", "ok perfect", "go", "dispatch", "thanks", "I just wanted to make sure"
Examples true: "use Redis instead of Memcached", "the bug is in UTF-8 parsing", "add a 30s timeout""#,
        prompt, context_block
    );

    let response = ai_smartness::processing::llm_subprocess::call_llm(&gate_prompt)?;

    // Parse JSON response
    if let Some(start) = response.find('{') {
        if let Some(end) = response.rfind('}') {
            let json_str = &response[start..=end];
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(json_str) {
                return Ok(v.get("relevant").and_then(|r| r.as_bool()).unwrap_or(true));
            }
        }
    }

    // Parse failure → fail-open (treat as relevant)
    Ok(true)
}

/// Check if a source_type is a file/URL tool eligible for changelog shortcut.
/// WebFetch included: same URL re-fetched → changelog update, skip LLM.
/// WebSearch excluded: results change per query, hash-based dedup won't match.
fn is_file_tool_source(source_type: &str) -> bool {
    matches!(
        source_type,
        "Read" | "file_read" | "Write" | "file_write" | "Edit"
            | "WebFetch" | "fetch"
            | "WebSearch"
            | "NotebookEdit"
    )
}

/// Try the changelog shortcut: if a thread already tracks this file_path,
/// append a changelog message and skip LLM extraction entirely.
/// Returns Some(thread_id) if shortcut applied, None if no matching thread found.
fn try_changelog_shortcut(
    conn: &Connection,
    pending: &mut Option<PendingContext>,
    source_type: &str,
    file_path: &str,
    content: &str,
    _guardian: &GuardianConfig,
) -> AiResult<Option<String>> {
    let matches = ThreadStorage::find_by_file_path(conn, file_path)?;
    if matches.is_empty() {
        return Ok(None);
    }

    // Take the most recently active thread that tracks this file
    let target = &matches[0];
    tracing::info!(
        thread_id = %target.id,
        thread_title = %target.title,
        thread_status = %target.status,
        file_path = %file_path,
        "Changelog shortcut: found existing thread for file"
    );

    // Extract continuity_from from pending context (the previous thread in the chain)
    let continuity_from = pending.as_ref()
        .filter(|p| !p.is_expired(_guardian.extraction.pending_context_ttl_secs))
        .map(|p| p.thread_id.clone());

    // add_changelog handles reactivation internally
    let result = ThreadManager::add_changelog(
        conn, &target.id, file_path, source_type, content,
        continuity_from.as_deref(),
    )?;

    // Update PendingContext to preserve coherence chain
    if let Some(ref tid) = result {
        let thread = ThreadStorage::get(conn, tid)?;
        let (labels, title, subjects, concepts, summary) = match thread {
            Some(t) => (t.labels, t.title, t.topics, t.concepts, t.summary),
            None => (vec![], String::new(), vec![], vec![], None),
        };
        *pending = Some(PendingContext {
            content: truncate_safe(content, 4000).to_string(),
            thread_id: tid.clone(),
            labels,
            coherence_score: None,
            timestamp: Instant::now(),
            title,
            subjects,
            concepts,
            summary,
        });
    }

    Ok(result)
}

/// Enrich an existing thread with LLM-generated metadata (summary, labels, concepts, topics).
/// Skips thread creation/merging — only updates the existing thread in place.
/// Called by capture queue workers for `enrich_thread` jobs.
pub fn enrich_existing_thread(
    conn: &Connection,
    thread_id: &str,
    _hint_content: &str,
    guardian: &GuardianConfig,
) -> AiResult<()> {
    let start = Instant::now();

    // 1. Load existing thread
    let mut thread = ThreadStorage::get(conn, thread_id)?
        .ok_or_else(|| ai_smartness::AiError::ThreadNotFound(
            format!("{} (enrichment)", thread_id),
        ))?;

    tracing::info!(
        thread_id = %thread_id,
        title = %thread.title,
        "Enrichment: starting LLM extraction"
    );

    // 2. Build enrichment content from thread metadata
    let content = format!(
        "Title: {}\nTopics: {}\nLabels: {}\nSummary: {}",
        thread.title,
        thread.topics.join(", "),
        thread.labels.join(", "),
        thread.summary.as_deref().unwrap_or("(none)"),
    );

    // 3. LLM extraction (via call_llm → goes through circuit breaker + Auto fallback)
    let extraction = match extractor::extract(
        &content,
        ExtractionSource::Response,
        &guardian.extraction,
        &guardian.label_suggestion,
        &guardian.importance_rating,
        None,
    )? {
        Some(e) => e,
        None => {
            tracing::debug!(thread_id = %thread_id, "Enrichment: extraction returned None");
            return Ok(());
        }
    };

    tracing::info!(
        thread_id = %thread_id,
        title = %extraction.title,
        labels = ?extraction.labels,
        concepts = extraction.concepts.len(),
        elapsed_ms = start.elapsed().as_millis(),
        "Enrichment: extraction complete"
    );

    // 4. Overwrite ALL fields — explicit enrichment always replaces previous values.
    // This enables full re-enrichment when upgrading LLM or reprocessing with better hardware.
    thread.summary = Some(extraction.summary.clone());
    thread.labels = extraction.labels.clone();
    thread.concepts = ai_smartness::constants::normalize_concepts(&extraction.concepts);
    thread.topics = extraction.subjects.clone();

    // Exception: respect importance_manually_set — user/agent intentionally set this value
    if !thread.importance_manually_set {
        thread.importance = extraction.importance;
    }

    // 5. Recompute embedding with enriched text
    let embed_text =
        ai_smartness::intelligence::thread_manager::build_enriched_embed_text_from_thread(&thread);
    let mgr = ai_smartness::processing::embeddings::EmbeddingManager::global();
    let embedding = mgr.embed(&embed_text);
    thread.embedding = Some(embedding);

    // 6. Persist
    ThreadStorage::update(conn, &thread)?;

    // 7. Create thinkbridges from new concepts
    if !thread.concepts.is_empty() {
        let bridge_count = ThreadManager::create_thinkbridges(
            conn,
            thread_id,
            &thread.concepts,
            &guardian.gossip,
        )
        .unwrap_or(0);
        tracing::info!(
            thread_id = %thread_id,
            bridges = bridge_count,
            total_ms = start.elapsed().as_millis(),
            "Enrichment: complete"
        );
    }

    Ok(())
}
