//! CLI subcommand: setup-model — download local LLM model for zero-cost inference.
//!
//! Downloads a GGUF model from HuggingFace into {data_dir}/models/
//! so the daemon can use local llama.cpp inference instead of Claude CLI.

use anyhow::{bail, Context, Result};
use std::path::PathBuf;

/// Default model filename.
const MODEL_FILENAME: &str = "qwen2.5-0.5b-instruct-q5_k_m.gguf";

/// HuggingFace download URL.
const MODEL_URL: &str = "https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF/resolve/main/qwen2.5-0.5b-instruct-q5_k_m.gguf";

pub fn run(force: bool) -> Result<()> {
    let data_dir = ai_smartness::storage::path_utils::data_dir();
    let model_dir = data_dir.join("models");
    std::fs::create_dir_all(&model_dir).context("Failed to create models directory")?;

    let model_path = model_dir.join(MODEL_FILENAME);

    if model_path.exists() && !force {
        let size = std::fs::metadata(&model_path)
            .map(|m| m.len() / 1_000_000)
            .unwrap_or(0);
        println!("Model already installed at {} ({}MB)", model_path.display(), size);
        println!("Use --force to re-download.");
        return Ok(());
    }

    println!("Downloading Qwen2.5-0.5B-Instruct (Q5_K_M quantization)...");
    println!("  URL: {}", MODEL_URL);
    println!("  Destination: {}", model_path.display());
    println!("  Size: ~400MB");
    println!();

    download_file(MODEL_URL, &model_path)?;

    if model_path.exists() {
        let size = std::fs::metadata(&model_path)
            .map(|m| m.len() / 1_000_000)
            .unwrap_or(0);
        println!("\nModel installed: {} ({}MB)", model_path.display(), size);
        println!("\nRestart the daemon to use local LLM inference:");
        println!("  ai-smartness daemon stop && ai-smartness daemon start");
    } else {
        bail!("Download completed but model file not found");
    }

    Ok(())
}

/// Download a URL to a local file using curl.
fn download_file(url: &str, dest: &PathBuf) -> Result<()> {
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
