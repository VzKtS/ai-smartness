//! CLI subcommand: setup-onnx — download ONNX Runtime for neural embeddings.
//!
//! Downloads libonnxruntime from Microsoft GitHub releases into
//! {data_dir}/lib/ so the daemon can use ONNX all-MiniLM-L6-v2 embeddings
//! instead of the TF-IDF hash fallback.

use anyhow::{bail, Context, Result};
use std::path::PathBuf;

/// ONNX Runtime version compatible with ort 2.0.0-rc.11.
const ORT_VERSION: &str = "1.20.1";

/// Base URL for Microsoft ONNX Runtime releases.
const ORT_RELEASE_BASE: &str = "https://github.com/microsoft/onnxruntime/releases/download";

pub fn run(force: bool) -> Result<()> {
    let data_dir = ai_smartness::storage::path_utils::data_dir();
    let lib_dir = data_dir.join("lib");
    std::fs::create_dir_all(&lib_dir).context("Failed to create lib directory")?;

    let (lib_name, archive_name) = platform_names()?;
    let lib_path = lib_dir.join(lib_name);

    if lib_path.exists() && !force {
        println!("ONNX Runtime already installed at {}", lib_path.display());
        println!("Use --force to re-download.");
        return Ok(());
    }

    let url = format!(
        "{}/v{}/{}",
        ORT_RELEASE_BASE, ORT_VERSION, archive_name
    );

    println!("Downloading ONNX Runtime v{} ...", ORT_VERSION);
    println!("  URL: {}", url);

    // Download to temp file
    let tmp_archive = lib_dir.join(&archive_name);
    download_file(&url, &tmp_archive)?;

    println!("Extracting {} ...", lib_name);
    extract_lib(&tmp_archive, &lib_dir, lib_name)?;

    // Cleanup archive
    let _ = std::fs::remove_file(&tmp_archive);

    if lib_path.exists() {
        println!("ONNX Runtime installed: {}", lib_path.display());
        println!("\nRestart the daemon to use ONNX embeddings:");
        println!("  ai-smartness daemon stop && ai-smartness daemon start");
    } else {
        bail!(
            "Extraction succeeded but {} not found at expected path",
            lib_name
        );
    }

    Ok(())
}

/// Returns (lib_filename, archive_filename) for the current platform.
fn platform_names() -> Result<(&'static str, String)> {
    let (lib_name, platform_tag) = if cfg!(target_os = "linux") && cfg!(target_arch = "x86_64") {
        ("libonnxruntime.so", "onnxruntime-linux-x64")
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "aarch64") {
        ("libonnxruntime.so", "onnxruntime-linux-aarch64")
    } else if cfg!(target_os = "macos") && cfg!(target_arch = "x86_64") {
        ("libonnxruntime.dylib", "onnxruntime-osx-x86_64")
    } else if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
        ("libonnxruntime.dylib", "onnxruntime-osx-arm64")
    } else {
        bail!(
            "Unsupported platform: {} / {}",
            std::env::consts::OS,
            std::env::consts::ARCH
        );
    };

    let archive = format!("{}-{}.tgz", platform_tag, ORT_VERSION);
    Ok((lib_name, archive))
}

/// Download a URL to a local file using curl (available on all target platforms).
fn download_file(url: &str, dest: &PathBuf) -> Result<()> {
    let status = std::process::Command::new("curl")
        .args(["-fSL", "--progress-bar", "-o"])
        .arg(dest.as_os_str())
        .arg(url)
        .status()
        .context("Failed to run curl. Is curl installed?")?;

    if !status.success() {
        bail!("Download failed (curl exit code: {:?})", status.code());
    }
    Ok(())
}

/// Extract the shared library from the .tgz archive.
///
/// Strategy: extract to a temp dir, then copy lib files to dest_dir.
/// This avoids GNU tar vs BSD tar argument-ordering issues.
fn extract_lib(archive: &PathBuf, dest_dir: &PathBuf, lib_name: &str) -> Result<()> {
    let tmp_dir = dest_dir.join("_ort_extract");
    std::fs::create_dir_all(&tmp_dir).context("Failed to create temp extraction dir")?;

    // Extract full archive into temp dir
    let status = std::process::Command::new("tar")
        .arg("xzf")
        .arg(archive.as_os_str())
        .arg("-C")
        .arg(tmp_dir.as_os_str())
        .status()
        .context("Failed to run tar")?;

    if !status.success() {
        let _ = std::fs::remove_dir_all(&tmp_dir);
        bail!("tar extraction failed");
    }

    // Find and copy lib files (libonnxruntime.so, .so.1, .so.1.20.1, or .dylib equivalents)
    let mut found = false;
    if let Ok(entries) = std::fs::read_dir(&tmp_dir) {
        for entry in entries.flatten() {
            let lib_subdir = entry.path().join("lib");
            if lib_subdir.is_dir() {
                if let Ok(lib_entries) = std::fs::read_dir(&lib_subdir) {
                    for lib_entry in lib_entries.flatten() {
                        let name = lib_entry.file_name();
                        let name_str = name.to_string_lossy();
                        if name_str.starts_with(lib_name) {
                            let dest_file = dest_dir.join(&*name_str);
                            std::fs::copy(lib_entry.path(), &dest_file)
                                .with_context(|| format!("Failed to copy {}", name_str))?;
                            println!("  Installed: {}", name_str);
                            found = true;
                        }
                    }
                }
            }
        }
    }

    // Cleanup temp dir
    let _ = std::fs::remove_dir_all(&tmp_dir);

    if !found {
        bail!("No {} files found in archive", lib_name);
    }
    Ok(())
}
