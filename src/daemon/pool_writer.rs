//! Pool Writer — non-blocking capture batching via JSONL files.
//!
//! Captures are appended to per-source-type JSONL files.
//! When a file exceeds size/line/age limits, it is sealed (.pending).
//! The PoolProcessor then consumes .pending files at its own pace.
//!
//! Lifecycle: `.jsonl` (active) → `.pending` (sealed) → `.done` (processed) → deleted

use std::collections::HashMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use ai_smartness::config::PoolConfig;

/// Entry in a pool file — one per JSONL line.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct PoolEntry {
    pub source_type: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    pub timestamp: String,
}

/// Tracks an active (open) JSONL file being written to.
struct ActiveFile {
    path: PathBuf,
    lines: usize,
    bytes: usize,
    created_at: Instant,
}

/// Pool writer — manages per-source-type JSONL files with auto-sealing.
pub struct PoolWriter {
    base_dir: PathBuf,
    active_files: HashMap<String, ActiveFile>,
    config: PoolConfig,
}

impl PoolWriter {
    pub fn new(pool_dir: &Path, config: PoolConfig) -> Self {
        Self {
            base_dir: pool_dir.to_path_buf(),
            active_files: HashMap::new(),
            config,
        }
    }

    /// Append a capture to the pool. Non-blocking (filesystem write only).
    /// Auto-seals if the active file exceeds limits.
    pub fn append(
        &mut self,
        source_type: &str,
        content: &str,
        file_path: Option<&str>,
    ) -> io::Result<()> {
        std::fs::create_dir_all(&self.base_dir)?;

        let entry = PoolEntry {
            source_type: source_type.to_string(),
            content: content.to_string(),
            file_path: file_path.map(String::from),
            timestamp: chrono::Utc::now().to_rfc3339(),
        };

        let line = serde_json::to_string(&entry)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        let line_bytes = line.len() + 1; // +1 for newline

        // Check if we need to seal the current file first
        if let Some(active) = self.active_files.get(source_type) {
            let should_seal = active.lines >= self.config.max_lines_per_file
                || active.bytes + line_bytes > self.config.max_bytes_per_file;
            if should_seal {
                // Remove from map, seal, then create new
                if let Some(active) = self.active_files.remove(source_type) {
                    Self::seal_file(&active.path)?;
                }
            }
        }

        // Get or create active file
        let active = self
            .active_files
            .entry(source_type.to_string())
            .or_insert_with(|| {
                let ts = chrono::Utc::now().timestamp_millis();
                let filename = format!("{}_{}.jsonl", source_type, ts);
                ActiveFile {
                    path: self.base_dir.join(filename),
                    lines: 0,
                    bytes: 0,
                    created_at: Instant::now(),
                }
            });

        // Append line
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&active.path)?;
        writeln!(file, "{}", line)?;

        active.lines += 1;
        active.bytes += line_bytes;

        // Check if we should seal after this write
        if active.lines >= self.config.max_lines_per_file
            || active.bytes >= self.config.max_bytes_per_file
        {
            if let Some(active) = self.active_files.remove(source_type) {
                Self::seal_file(&active.path)?;
            }
        }

        Ok(())
    }

    /// Seal all active files (flush). Called at shutdown.
    pub fn seal_all(&mut self) -> io::Result<()> {
        let files: Vec<(String, ActiveFile)> = self.active_files.drain().collect();
        for (_, active) in files {
            if active.lines > 0 {
                Self::seal_file(&active.path)?;
            }
        }
        Ok(())
    }

    /// Seal files that have exceeded max_age_secs.
    pub fn seal_expired(&mut self) -> io::Result<()> {
        let max_age = std::time::Duration::from_secs(self.config.max_age_secs);
        let expired_keys: Vec<String> = self
            .active_files
            .iter()
            .filter(|(_, f)| f.created_at.elapsed() >= max_age && f.lines > 0)
            .map(|(k, _)| k.clone())
            .collect();

        for key in expired_keys {
            if let Some(active) = self.active_files.remove(&key) {
                Self::seal_file(&active.path)?;
            }
        }
        Ok(())
    }

    /// Rename .jsonl → .pending (seal).
    fn seal_file(path: &Path) -> io::Result<()> {
        let pending_path = path.with_extension("pending");
        std::fs::rename(path, &pending_path)?;
        tracing::debug!(
            from = %path.display(),
            to = %pending_path.display(),
            "Pool file sealed"
        );
        Ok(())
    }
}

/// Clean up .done files older than the configured interval.
pub fn cleanup_done_files(pool_dir: &Path, max_age_secs: u64) -> io::Result<usize> {
    if !pool_dir.exists() {
        return Ok(0);
    }
    let mut cleaned = 0;
    for entry in std::fs::read_dir(pool_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map(|e| e == "done").unwrap_or(false) {
            if let Ok(meta) = path.metadata() {
                if let Ok(modified) = meta.modified() {
                    if modified.elapsed().unwrap_or_default().as_secs() > max_age_secs {
                        std::fs::remove_file(&path)?;
                        cleaned += 1;
                    }
                }
            }
        }
    }
    Ok(cleaned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn test_config() -> PoolConfig {
        PoolConfig {
            max_lines_per_file: 3,
            max_bytes_per_file: 10_000,
            max_age_secs: 60,
            cleanup_interval_secs: 300,
        }
    }

    #[test]
    fn test_pool_writer_creates_jsonl_file() {
        let dir = tempfile::tempdir().unwrap();
        let pool_dir = dir.path().join("pool");
        let mut writer = PoolWriter::new(&pool_dir, test_config());

        writer
            .append("Read", "file content here", Some("/src/main.rs"))
            .unwrap();

        let files: Vec<_> = fs::read_dir(&pool_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|x| x == "jsonl")
                    .unwrap_or(false)
            })
            .collect();
        assert_eq!(files.len(), 1, "Should create one .jsonl file");

        let content = fs::read_to_string(files[0].path()).unwrap();
        assert!(content.contains("Read"));
        assert!(content.contains("file content here"));
    }

    #[test]
    fn test_pool_writer_seals_at_max_lines() {
        let dir = tempfile::tempdir().unwrap();
        let pool_dir = dir.path().join("pool");
        let mut writer = PoolWriter::new(&pool_dir, test_config());

        writer.append("Read", "line1", None).unwrap();
        writer.append("Read", "line2", None).unwrap();
        writer.append("Read", "line3", None).unwrap(); // triggers seal

        let pending: Vec<_> = fs::read_dir(&pool_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|x| x == "pending")
                    .unwrap_or(false)
            })
            .collect();
        assert_eq!(pending.len(), 1, "Should seal into .pending after 3 lines");
    }

    #[test]
    fn test_pool_writer_seal_all_flushes_active() {
        let dir = tempfile::tempdir().unwrap();
        let pool_dir = dir.path().join("pool");
        let mut writer = PoolWriter::new(&pool_dir, test_config());

        writer.append("Read", "partial", None).unwrap();
        writer.seal_all().unwrap();

        let pending: Vec<_> = fs::read_dir(&pool_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|x| x == "pending")
                    .unwrap_or(false)
            })
            .collect();
        assert_eq!(
            pending.len(),
            1,
            "seal_all should convert active .jsonl to .pending"
        );
    }

    #[test]
    fn test_pool_writer_groups_by_source_type() {
        let dir = tempfile::tempdir().unwrap();
        let pool_dir = dir.path().join("pool");
        let mut writer = PoolWriter::new(&pool_dir, test_config());

        writer.append("Read", "content1", None).unwrap();
        writer.append("Write", "content2", None).unwrap();

        let files: Vec<_> = fs::read_dir(&pool_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(files.len(), 2, "Different source_types = different files");
    }

    #[test]
    fn test_cleanup_done_files() {
        let dir = tempfile::tempdir().unwrap();
        let pool_dir = dir.path().join("pool");
        fs::create_dir_all(&pool_dir).unwrap();

        // Create a .done file
        fs::write(pool_dir.join("Read_1000.done"), "processed").unwrap();
        // Create a .pending file (should not be cleaned)
        fs::write(pool_dir.join("Read_2000.pending"), "waiting").unwrap();

        // Cleanup with 0 max_age (clean everything)
        let cleaned = cleanup_done_files(&pool_dir, 0).unwrap();
        assert_eq!(cleaned, 1);

        // .pending should still exist
        assert!(pool_dir.join("Read_2000.pending").exists());
    }

    #[test]
    fn test_done_files_sorted_by_timestamp() {
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
