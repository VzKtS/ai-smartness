//! Model download management — aria2c (primary) with curl fallback.
//!
//! Handles download, checksum verification, disk space check, and concurrent download prevention.
//! Consolidates download logic previously split between local_llm.rs and cli/setup_model.rs.

use crate::config::LocalModelSize;
use crate::storage::path_utils;
use crate::{AiError, AiResult};
use std::path::{Path, PathBuf};

/// Models directory path.
pub fn model_dir_path() -> PathBuf {
    path_utils::data_dir().join("models")
}

/// Full path to a model's GGUF file.
pub fn model_file_path(size: &LocalModelSize) -> PathBuf {
    model_dir_path().join(size.filename())
}

/// Check if a model is already downloaded.
pub fn is_downloaded(size: &LocalModelSize) -> bool {
    model_file_path(size).exists()
}

/// Download a model GGUF file with aria2c (16 segments, resume) or curl fallback.
/// Returns the path to the downloaded file.
pub fn download_model(size: &LocalModelSize, force: bool) -> AiResult<PathBuf> {
    let dir = model_dir_path();
    let dest = dir.join(size.filename());

    // Skip if already downloaded (unless force)
    if dest.exists() && !force {
        tracing::info!(path = %dest.display(), "Model already downloaded");
        return Ok(dest);
    }

    // Create models directory
    std::fs::create_dir_all(&dir).map_err(|e| {
        AiError::Storage(format!("Failed to create models dir: {}", e))
    })?;

    // Check disk space
    check_disk_space(size.file_size_bytes(), &dir)?;

    // Acquire download lock
    let _lock = acquire_lock(size)?;

    // Remove existing file if force re-download
    if dest.exists() && force {
        let _ = std::fs::remove_file(&dest);
    }

    let url = size.download_url();
    tracing::info!(url = url, model = size.cli_name(), "Downloading model...");

    // Try aria2c first, fall back to curl
    let result = download_via_aria2c(url, &dir, size.filename())
        .or_else(|e| {
            tracing::warn!("aria2c failed ({}), falling back to curl", e);
            eprintln!("Warning: aria2c not available — falling back to curl (slower, no resume).");
            eprintln!("Install aria2 for faster downloads: sudo apt install aria2");
            download_via_curl(url, &dest)
        });

    match result {
        Ok(()) => {
            // Verify file exists and has reasonable size
            if dest.exists() {
                let actual_size = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
                let expected = size.file_size_bytes();
                // Allow 20% tolerance on size (estimates may not be exact)
                if actual_size < expected / 2 {
                    let _ = std::fs::remove_file(&dest);
                    return Err(AiError::Provider(format!(
                        "Downloaded file too small ({} MB vs expected ~{} MB) — likely corrupted",
                        actual_size / 1_000_000,
                        expected / 1_000_000
                    )));
                }
                tracing::info!(
                    path = %dest.display(),
                    size_mb = actual_size / 1_000_000,
                    "Model downloaded successfully"
                );
                Ok(dest)
            } else {
                Err(AiError::Provider("Download completed but file not found".into()))
            }
        }
        Err(e) => {
            let _ = std::fs::remove_file(&dest);
            Err(e)
        }
    }
}

/// Verify SHA256 checksum of a downloaded model file.
/// Returns true if checksum matches, false if mismatch.
pub fn verify_checksum(path: &Path, expected_hex: &str) -> AiResult<bool> {
    use sha2::{Digest, Sha256};

    if expected_hex.is_empty() {
        tracing::debug!("No expected checksum — skipping verification");
        return Ok(true);
    }

    if !path.exists() {
        return Err(AiError::Storage(format!("File not found: {}", path.display())));
    }

    let file = std::fs::File::open(path).map_err(|e| {
        AiError::Storage(format!("Failed to open file for checksum: {}", e))
    })?;

    let mut reader = std::io::BufReader::with_capacity(1024 * 1024, file); // 1 MB buffer
    let mut hasher = Sha256::new();
    std::io::copy(&mut reader, &mut hasher).map_err(|e| {
        AiError::Storage(format!("Failed to hash file: {}", e))
    })?;

    let result = format!("{:x}", hasher.finalize());
    Ok(result == expected_hex.to_lowercase())
}

/// Check available disk space before download.
pub fn check_disk_space(needed_bytes: u64, target_dir: &Path) -> AiResult<()> {
    let needed_with_margin = (needed_bytes as f64 * 1.1) as u64; // 10% margin

    #[cfg(unix)]
    {
        let available = get_available_space_unix(target_dir);
        if let Some(avail) = available {
            if avail < needed_with_margin {
                return Err(AiError::Storage(format!(
                    "Insufficient disk space: {:.1} GB available, {:.1} GB needed",
                    avail as f64 / 1_000_000_000.0,
                    needed_with_margin as f64 / 1_000_000_000.0
                )));
            }
            tracing::info!(
                available_gb = format!("{:.1}", avail as f64 / 1_000_000_000.0),
                needed_gb = format!("{:.1}", needed_with_margin as f64 / 1_000_000_000.0),
                "Disk space check passed"
            );
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (needed_with_margin, target_dir);
    }

    Ok(())
}

/// Delete a downloaded model file.
pub fn delete_model(size: &LocalModelSize) -> AiResult<()> {
    let path = model_file_path(size);
    if !path.exists() {
        return Err(AiError::Storage(format!(
            "Model '{}' is not downloaded",
            size.cli_name()
        )));
    }
    std::fs::remove_file(&path).map_err(|e| {
        AiError::Storage(format!("Failed to delete model file: {}", e))
    })?;
    // Also remove any aria2 control file
    let aria2_ctrl = path.with_extension("gguf.aria2");
    let _ = std::fs::remove_file(aria2_ctrl);
    tracing::info!(model = size.cli_name(), "Model file deleted");
    Ok(())
}

// ============================================================================
// Download backends
// ============================================================================

/// Download via aria2c with 16 parallel segments and resume support.
fn download_via_aria2c(url: &str, dir: &Path, filename: &str) -> AiResult<()> {
    let status = std::process::Command::new("aria2c")
        .args([
            "-x16",                        // 16 connections
            "-s16",                        // 16 segments
            "--continue=true",             // resume support
            "--auto-file-renaming=false",  // don't rename on conflict
            "--summary-interval=10",       // progress every 10s
            "-d",
        ])
        .arg(dir.as_os_str())
        .args(["-o", filename])
        .arg(url)
        .stdin(std::process::Stdio::null())
        .status()
        .map_err(|e| AiError::Provider(format!("aria2c not found or failed to start: {}", e)))?;

    if !status.success() {
        return Err(AiError::Provider(format!(
            "aria2c download failed (exit {})",
            status.code().unwrap_or(-1)
        )));
    }
    Ok(())
}

/// Fallback download via curl (single connection, progress bar).
fn download_via_curl(url: &str, dest: &Path) -> AiResult<()> {
    let status = std::process::Command::new("curl")
        .args(["-fSL", "--progress-bar", "-o"])
        .arg(dest.as_os_str())
        .arg(url)
        .status()
        .map_err(|e| AiError::Provider(format!("curl failed: {}", e)))?;

    if !status.success() {
        return Err(AiError::Provider(format!(
            "curl download failed (exit {})",
            status.code().unwrap_or(-1)
        )));
    }
    Ok(())
}

// ============================================================================
// Disk space (Unix)
// ============================================================================

#[cfg(unix)]
fn get_available_space_unix(path: &Path) -> Option<u64> {
    use std::ffi::CString;
    use std::mem::MaybeUninit;

    // Use parent if target doesn't exist yet
    let check_path = if path.exists() { path.to_path_buf() } else {
        path.parent().unwrap_or(Path::new("/")).to_path_buf()
    };

    let c_path = CString::new(check_path.to_string_lossy().as_bytes()).ok()?;
    let mut stat = MaybeUninit::<libc::statvfs>::uninit();

    let result = unsafe { libc::statvfs(c_path.as_ptr(), stat.as_mut_ptr()) };
    if result != 0 {
        return None;
    }

    let stat = unsafe { stat.assume_init() };
    Some(stat.f_bavail as u64 * stat.f_bsize as u64)
}

// ============================================================================
// Download lock (prevent concurrent downloads of same model)
// ============================================================================

struct DownloadLock {
    path: PathBuf,
}

impl Drop for DownloadLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn acquire_lock(size: &LocalModelSize) -> AiResult<DownloadLock> {
    let lock_path = model_dir_path().join(format!("{}.lock", size.filename()));

    // Check if another download is in progress
    if lock_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&lock_path) {
            let pid_str = content.trim();
            if is_pid_alive(pid_str) {
                return Err(AiError::Storage(format!(
                    "Another download of '{}' is already in progress (PID {})",
                    size.cli_name(),
                    pid_str
                )));
            }
            // Stale lock — remove it
            let _ = std::fs::remove_file(&lock_path);
        }
    }

    // Write our PID
    std::fs::write(&lock_path, std::process::id().to_string()).map_err(|e| {
        AiError::Storage(format!("Failed to create download lock: {}", e))
    })?;

    Ok(DownloadLock { path: lock_path })
}

fn is_pid_alive(pid_str: &str) -> bool {
    #[cfg(unix)]
    {
        if let Ok(pid) = pid_str.parse::<i32>() {
            extern "C" {
                fn kill(pid: i32, sig: i32) -> i32;
            }
            unsafe { kill(pid, 0) == 0 }
        } else {
            false
        }
    }
    #[cfg(not(unix))]
    {
        let _ = pid_str;
        false
    }
}
