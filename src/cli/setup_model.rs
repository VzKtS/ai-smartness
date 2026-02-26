//! CLI subcommand: setup-model — download local LLM model for zero-cost inference.
//!
//! Downloads a Qwen2.5-Instruct GGUF model from HuggingFace into {data_dir}/models/
//! so the daemon can use local llama.cpp inference.
//!
//! Models: 3B (default, ~2.1GB) or 7B (~4.7GB).

use ai_smartness::config::LocalModelSize;
use anyhow::{bail, Context, Result};

pub fn run(force: bool, size_7b: bool) -> Result<()> {
    let size = if size_7b { LocalModelSize::SevenB } else { LocalModelSize::ThreeB };

    let data_dir = ai_smartness::storage::path_utils::data_dir();
    let model_dir = data_dir.join("models");
    std::fs::create_dir_all(&model_dir).context("Failed to create models directory")?;

    let model_path = model_dir.join(size.filename());

    if model_path.exists() && !force {
        let file_size = std::fs::metadata(&model_path)
            .map(|m| m.len() / 1_000_000)
            .unwrap_or(0);
        println!("Model already installed at {} ({}MB)", model_path.display(), file_size);
        println!("Use --force to re-download.");
        return Ok(());
    }

    println!("Downloading {}...", size.display_name());
    println!("  URL: {}", size.download_url());
    println!("  Destination: {}", model_path.display());
    println!();

    download_file(size.download_url(), &model_path)?;

    if model_path.exists() {
        let file_size = std::fs::metadata(&model_path)
            .map(|m| m.len() / 1_000_000)
            .unwrap_or(0);
        println!("\nModel installed: {} ({}MB)", model_path.display(), file_size);
        println!("\nRestart the daemon to use local LLM inference:");
        println!("  ai-smartness daemon stop && ai-smartness daemon start");
    } else {
        bail!("Download completed but model file not found");
    }

    Ok(())
}

/// Download a URL to a local file using curl.
fn download_file(url: &str, dest: &std::path::PathBuf) -> Result<()> {
    let status = std::process::Command::new("curl")
        .args(["-fSL", "--progress-bar", "-o"])
        .arg(dest.as_os_str())
        .arg(url)
        .status()
        .context("Failed to run curl. Is curl installed?")?;

    if !status.success() {
        // Clean up partial download
        let _ = std::fs::remove_file(dest);
        bail!("Download failed (curl exit code: {:?})", status.code());
    }
    Ok(())
}
