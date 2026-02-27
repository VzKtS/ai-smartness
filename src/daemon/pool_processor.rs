//! Pool Processor — consumes sealed .pending files and runs extraction.
//!
//! Scans the pool directory for .pending files (sorted by timestamp),
//! reads JSONL entries, processes each via processor::process_capture(),
//! and renames .pending → .done after successful processing.

use std::path::Path;

use ai_smartness::config::GuardianConfig;
use ai_smartness::AiResult;
use rusqlite::Connection;

use super::pool_writer::PoolEntry;
use super::processor::{self, PendingContext};

/// Process all .pending files in the pool directory.
/// Returns the total number of captures processed.
pub fn process_pending_files(
    pool_dir: &Path,
    conn: &Connection,
    pending: &mut Option<PendingContext>,
    thread_quota: usize,
    guardian: &GuardianConfig,
) -> AiResult<usize> {
    if !pool_dir.exists() {
        return Ok(0);
    }

    // Collect and sort .pending files by name (timestamp in filename = chronological order)
    let mut pending_files: Vec<_> = std::fs::read_dir(pool_dir)
        .map_err(|e| ai_smartness::AiError::Storage(e.to_string()))?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|x| x == "pending")
                .unwrap_or(false)
        })
        .collect();

    pending_files.sort_by_key(|e| e.file_name());

    let mut total_processed = 0;

    for entry in &pending_files {
        let path = entry.path();
        match process_single_file(&path, pool_dir, conn, pending, thread_quota, guardian) {
            Ok(count) => {
                // Rename .pending → .done
                let done_path = path.with_extension("done");
                if let Err(e) = std::fs::rename(&path, &done_path) {
                    tracing::warn!(
                        path = %path.display(),
                        error = %e,
                        "Failed to rename .pending → .done"
                    );
                }
                total_processed += count;
                tracing::info!(
                    file = %path.display(),
                    captures = count,
                    "Pool file processed"
                );
            }
            Err(e) => {
                tracing::warn!(
                    file = %path.display(),
                    error = %e,
                    "Pool file processing failed — keeping .pending for retry"
                );
            }
        }
    }

    if total_processed > 0 {
        tracing::info!(
            files = pending_files.len(),
            captures = total_processed,
            "Pool processing complete"
        );
    }

    Ok(total_processed)
}

/// Process a single .pending file (JSONL lines).
fn process_single_file(
    path: &Path,
    pool_dir: &Path,
    conn: &Connection,
    pending: &mut Option<PendingContext>,
    thread_quota: usize,
    guardian: &GuardianConfig,
) -> AiResult<usize> {
    let file_start = std::time::Instant::now();

    // Skip files deleted between directory listing and processing
    if !path.exists() {
        tracing::debug!(file = %path.display(), "Pool file vanished — skipping");
        let _ = std::fs::remove_file(path); // cleanup stale entry
        return Ok(0);
    }

    tracing::info!(
        file = %path.display(),
        "Pool file: reading JSONL entries"
    );

    let content = std::fs::read_to_string(path)
        .map_err(|e| ai_smartness::AiError::Storage(format!("Read pool file: {}", e)))?;

    let total_lines = content.lines().filter(|l| !l.trim().is_empty()).count();
    tracing::info!(
        file = %path.display(),
        total_lines = total_lines,
        file_bytes = content.len(),
        "Pool file: loaded, processing entries"
    );

    let mut processed = 0;

    for (line_idx, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }

        let entry: PoolEntry = match serde_json::from_str(line) {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!(
                    line = line_idx + 1,
                    error = %e,
                    line_preview = %&line[..line.len().min(100)],
                    "Skipping malformed JSONL line"
                );
                continue;
            }
        };

        let max_chars = guardian.extraction.max_content_chars;
        let content_chars: usize = entry.content.chars().count();

        // Split oversized content: process first chunk, re-queue excess
        let (process_content, excess) = if content_chars > max_chars
            && entry.chunk_index < ai_smartness::constants::MAX_CONTENT_CHUNKS
        {
            let first_chunk: String = entry.content.chars().take(max_chars).collect();
            let remainder: String = entry.content.chars().skip(max_chars).collect();
            tracing::info!(
                line = line_idx + 1,
                chunk_index = entry.chunk_index,
                content_chars = content_chars,
                max_chars = max_chars,
                first_chunk_chars = max_chars,
                remainder_chars = content_chars - max_chars,
                "Content oversized — splitting into chunks"
            );
            (first_chunk, Some(remainder))
        } else {
            if content_chars > max_chars {
                tracing::info!(
                    line = line_idx + 1,
                    chunk_index = entry.chunk_index,
                    max_chunks = ai_smartness::constants::MAX_CONTENT_CHUNKS,
                    "Max chunks reached — processing truncated (no further re-queue)"
                );
            }
            (entry.content.clone(), None)
        };

        tracing::info!(
            line = line_idx + 1,
            total = total_lines,
            source_type = %entry.source_type,
            content_len = process_content.len(),
            chunk_index = entry.chunk_index,
            has_excess = excess.is_some(),
            file_path = ?entry.file_path,
            "Pool entry: starting pipeline"
        );

        let entry_start = std::time::Instant::now();
        match processor::process_capture(
            conn,
            pending,
            &entry.source_type,
            &process_content,
            entry.file_path.as_deref(),
            thread_quota,
            guardian,
        ) {
            Ok(thread_id) => {
                processed += 1;
                tracing::info!(
                    line = line_idx + 1,
                    thread_id = ?thread_id,
                    elapsed_ms = entry_start.elapsed().as_millis(),
                    "Pool entry: complete"
                );

                // Re-queue excess content for deferred processing
                if let Some(remainder) = excess {
                    requeue_excess(
                        pool_dir,
                        &entry.source_type,
                        &remainder,
                        entry.file_path.as_deref(),
                        entry.chunk_index + 1,
                    );
                }
            }
            Err(e) => {
                tracing::warn!(
                    line = line_idx + 1,
                    source_type = %entry.source_type,
                    error = %e,
                    elapsed_ms = entry_start.elapsed().as_millis(),
                    "Pool entry: processing failed"
                );
            }
        }
    }

    tracing::info!(
        file = %path.display(),
        processed = processed,
        total_lines = total_lines,
        elapsed_ms = file_start.elapsed().as_millis(),
        "Pool file: done"
    );

    Ok(processed)
}

/// Write excess content back to pool as a new .pending file for deferred processing.
fn requeue_excess(
    pool_dir: &Path,
    source_type: &str,
    content: &str,
    file_path: Option<&str>,
    chunk_index: u8,
) {
    let entry = PoolEntry {
        source_type: source_type.to_string(),
        content: content.to_string(),
        file_path: file_path.map(String::from),
        timestamp: chrono::Utc::now().to_rfc3339(),
        chunk_index,
    };

    let line = match serde_json::to_string(&entry) {
        Ok(l) => l,
        Err(e) => {
            tracing::warn!(error = %e, "Failed to serialize excess chunk — dropping remainder");
            return;
        }
    };

    let ts = chrono::Utc::now().timestamp_millis();
    let filename = format!("{}_chunk{}_{}.pending", source_type, chunk_index, ts);
    let path = pool_dir.join(filename);

    match std::fs::write(&path, format!("{}\n", line)) {
        Ok(_) => {
            tracing::info!(
                path = %path.display(),
                chunk_index = chunk_index,
                excess_chars = content.chars().count(),
                "Excess content re-queued for deferred processing"
            );
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                chunk_index = chunk_index,
                "Failed to write excess chunk to pool — dropping remainder"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    fn write_pending_file(dir: &std::path::Path, name: &str, lines: &[&str]) {
        fs::create_dir_all(dir).unwrap();
        let content = lines.join("\n") + "\n";
        fs::write(dir.join(name), content).unwrap();
    }

    #[test]
    fn test_pending_file_exists_before_processing() {
        let dir = tempfile::tempdir().unwrap();
        let pool_dir = dir.path().join("pool");

        write_pending_file(
            &pool_dir,
            "Read_1000.pending",
            &[r#"{"source_type":"Read","content":"hello world test content for extraction","timestamp":"2026-02-25T12:00:00Z"}"#],
        );

        let pending_before: Vec<_> = fs::read_dir(&pool_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|x| x == "pending")
                    .unwrap_or(false)
            })
            .collect();
        assert_eq!(pending_before.len(), 1);
    }

    #[test]
    fn test_pending_files_sorted_by_timestamp_in_name() {
        let dir = tempfile::tempdir().unwrap();
        let pool_dir = dir.path().join("pool");
        fs::create_dir_all(&pool_dir).unwrap();

        fs::write(pool_dir.join("Read_2000.pending"), "").unwrap();
        fs::write(pool_dir.join("Read_1000.pending"), "").unwrap();
        fs::write(pool_dir.join("Read_3000.pending"), "").unwrap();

        let mut files: Vec<_> = fs::read_dir(&pool_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|x| x == "pending")
                    .unwrap_or(false)
            })
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        files.sort();

        assert_eq!(files[0], "Read_1000.pending", "Oldest first");
        assert_eq!(files[2], "Read_3000.pending", "Newest last");
    }

    #[test]
    fn test_requeue_excess_creates_pending_file() {
        let dir = tempfile::tempdir().unwrap();
        let pool_dir = dir.path().join("pool");
        fs::create_dir_all(&pool_dir).unwrap();

        super::requeue_excess(
            &pool_dir,
            "Read",
            "this is the excess content from a large file",
            Some("/path/to/file.rs"),
            1,
        );

        let pending: Vec<_> = fs::read_dir(&pool_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name.contains("chunk1") && name.ends_with(".pending")
            })
            .collect();
        assert_eq!(pending.len(), 1, "Should create one .pending file for excess");

        // Verify the content is valid JSONL
        let content = fs::read_to_string(pending[0].path()).unwrap();
        let entry: super::PoolEntry = serde_json::from_str(content.trim()).unwrap();
        assert_eq!(entry.source_type, "Read");
        assert_eq!(entry.chunk_index, 1);
        assert_eq!(entry.content, "this is the excess content from a large file");
        assert_eq!(entry.file_path.as_deref(), Some("/path/to/file.rs"));
    }

    #[test]
    fn test_requeue_excess_respects_max_chunks() {
        // chunk_index is checked BEFORE calling requeue_excess,
        // but verify the function works with high indices
        let dir = tempfile::tempdir().unwrap();
        let pool_dir = dir.path().join("pool");
        fs::create_dir_all(&pool_dir).unwrap();

        super::requeue_excess(&pool_dir, "Read", "remainder", None, 4);

        let pending: Vec<_> = fs::read_dir(&pool_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".pending"))
            .collect();
        assert_eq!(pending.len(), 1);

        let content = fs::read_to_string(pending[0].path()).unwrap();
        let entry: super::PoolEntry = serde_json::from_str(content.trim()).unwrap();
        assert_eq!(entry.chunk_index, 4);
    }
}
