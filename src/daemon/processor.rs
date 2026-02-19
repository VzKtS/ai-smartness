//! Capture processor — clean, extract, and manage threads from tool/prompt output.
//!
//! Called by the IPC server for each tool_capture and prompt_capture request.
//! Maintains a PendingContext for coherence-based child linking.

use std::time::Instant;

use ai_smartness::AiResult;
use ai_smartness::intelligence::thread_manager::ThreadManager;
use ai_smartness::processing::cleaner;
use ai_smartness::processing::extractor::{self, ExtractionSource};
use ai_smartness::storage::threads::ThreadStorage;
use rusqlite::Connection;

/// Pending context for coherence-based child linking.
/// Stores the last capture's content + thread_id.
/// Expires after PENDING_CONTEXT_TTL_SECS (10 minutes).
pub struct PendingContext {
    pub content: String,
    pub thread_id: String,
    pub labels: Vec<String>,
    pub timestamp: Instant,
}

const PENDING_CONTEXT_TTL_SECS: u64 = 600; // 10 minutes

impl PendingContext {
    pub fn is_expired(&self) -> bool {
        self.timestamp.elapsed().as_secs() > PENDING_CONTEXT_TTL_SECS
    }
}

/// Process a tool capture: clean -> extract -> thread management.
/// Returns the thread_id of the created/updated thread, or None if skipped.
pub fn process_capture(
    conn: &Connection,
    pending: &mut Option<PendingContext>,
    source_type: &str,
    content: &str,
    file_path: Option<&str>,
    thread_quota: usize,
) -> AiResult<Option<String>> {
    tracing::info!(source = source_type, content_len = content.len(), "Processing capture");

    // 1. Clean content
    let cleaned = cleaner::clean_tool_output(content);
    if !cleaner::should_capture(&cleaned) {
        tracing::debug!(cleaned_len = cleaned.len(), "Capture filtered out (noise)");
        return Ok(None);
    }

    // 2. Extract metadata (LLM with heuristic fallback)
    let source = match source_type {
        "Read" | "file_read" => ExtractionSource::FileRead,
        "Write" | "file_write" => ExtractionSource::FileWrite,
        "Task" | "task" => ExtractionSource::Task,
        "Fetch" | "fetch" => ExtractionSource::Fetch,
        "Response" | "response" => ExtractionSource::Response,
        "Command" | "command" => ExtractionSource::Command,
        _ => ExtractionSource::Prompt,
    };

    let extraction = extractor::extract(&cleaned, source)?;
    tracing::debug!(
        title = %extraction.title,
        confidence = extraction.confidence,
        topics = ?extraction.subjects,
        "Extraction complete"
    );
    if extraction.confidence == 0.0 {
        tracing::debug!("Extraction confidence=0, skipping");
        return Ok(None);
    }

    // 3. Determine parent hint from pending context
    let parent_hint = pending
        .as_ref()
        .filter(|p| !p.is_expired())
        .map(|p| p.thread_id.clone());

    // 4. Thread management (NewThread / Continue / Fork / Reactivate)
    let thread_id = ThreadManager::process_input(
        conn,
        &extraction,
        &cleaned,
        source_type,
        file_path,
        parent_hint.as_deref(),
        thread_quota,
    )?;

    tracing::info!(thread_id = ?thread_id, "Capture processed");

    // 5. Update pending context for next capture
    if let Some(ref tid) = thread_id {
        let labels = ThreadStorage::get(conn, tid)?
            .map(|t| t.labels)
            .unwrap_or_default();
        *pending = Some(PendingContext {
            content: cleaned[..cleaned.len().min(1500)].to_string(),
            thread_id: tid.clone(),
            labels,
            timestamp: Instant::now(),
        });
    }

    Ok(thread_id)
}

/// Process a prompt capture (user message).
pub fn process_prompt(
    conn: &Connection,
    pending: &mut Option<PendingContext>,
    prompt: &str,
    _session_id: Option<&str>,
    thread_quota: usize,
) -> AiResult<Option<String>> {
    tracing::info!(prompt_len = prompt.len(), "Processing prompt capture");
    process_capture(conn, pending, "prompt", prompt, None, thread_quota)
}
