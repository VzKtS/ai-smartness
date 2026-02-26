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
        match process_single_file(&path, conn, pending, thread_quota, guardian) {
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

        tracing::info!(
            line = line_idx + 1,
            total = total_lines,
            source_type = %entry.source_type,
            content_len = entry.content.len(),
            file_path = ?entry.file_path,
            "Pool entry: starting pipeline"
        );

        let entry_start = std::time::Instant::now();
        match processor::process_capture(
            conn,
            pending,
            &entry.source_type,
            &entry.content,
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
}
