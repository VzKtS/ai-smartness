//! CLI `model` subcommand — manage local LLM models.

use ai_smartness::config::{GuardianConfig, LocalModelSize};
use ai_smartness::processing::{model_download, daemon_ipc_client};
use ai_smartness::storage::path_utils;
use anyhow::{Context, Result};

/// List all available models with download and active status.
pub fn list() -> Result<()> {
    let active = load_active_model();
    let variants = LocalModelSize::all_variants();

    println!("Available models:\n");
    for size in &variants {
        let downloaded = model_download::is_downloaded(size);
        let is_active = *size == active;
        let marker = if is_active { " *" } else { "" };
        let status = if downloaded { "downloaded" } else { "not downloaded" };
        let file_gb = size.file_size_bytes() as f64 / 1_000_000_000.0;

        println!(
            "  {:<12} {:<45} {:>6.1} GB  [{}]{}",
            size.cli_name(),
            size.display_name(),
            file_gb,
            status,
            marker
        );
    }
    println!("\n  * = active model");
    Ok(())
}

/// Show detailed info for a specific model.
pub fn info(name: &str) -> Result<()> {
    let size = parse_model_name(name)?;
    let active = load_active_model();
    let downloaded = model_download::is_downloaded(&size);
    let path = model_download::model_file_path(&size);

    println!("Model: {}", size.display_name());
    println!("  CLI name:     {}", size.cli_name());
    println!("  Template:     {}", size.template_name());
    println!("  VRAM:         {} MB", size.model_vram_mb());
    println!("  Native ctx:   {} tokens", size.native_ctx_size());
    println!("  File size:    {:.1} GB", size.file_size_bytes() as f64 / 1_000_000_000.0);
    println!("  KV/token:     {} KB", size.kv_bytes_per_token() / 1024);
    println!("  Ctx cascade:  {:?}", size.ctx_cascade());
    println!("  Status:       {}", if downloaded { "downloaded" } else { "not downloaded" });
    println!("  Active:       {}", if size == active { "yes" } else { "no" });
    if downloaded {
        if let Ok(meta) = std::fs::metadata(&path) {
            println!("  File path:    {}", path.display());
            println!("  Actual size:  {:.1} GB", meta.len() as f64 / 1_000_000_000.0);
        }
    }
    Ok(())
}

/// Show currently active model.
pub fn active() -> Result<()> {
    let active = load_active_model();
    let downloaded = model_download::is_downloaded(&active);
    println!("Active model: {} — {}", active.cli_name(), active.display_name());
    if !downloaded {
        println!("  Warning: model is not downloaded. Run: ai-smartness model download {}", active.cli_name());
    }
    Ok(())
}

/// Download a model.
pub fn download(name: &str, force: bool) -> Result<()> {
    let size = parse_model_name(name)?;

    if model_download::is_downloaded(&size) && !force {
        println!("Model '{}' is already downloaded.", size.cli_name());
        println!("Use --force to re-download.");
        return Ok(());
    }

    println!("Downloading {} ({:.1} GB)...",
        size.display_name(),
        size.file_size_bytes() as f64 / 1_000_000_000.0
    );

    match model_download::download_model(&size, force) {
        Ok(path) => {
            println!("Downloaded: {}", path.display());
            Ok(())
        }
        Err(e) => {
            eprintln!("Download failed: {}", e);
            Err(anyhow::anyhow!("{}", e))
        }
    }
}

/// Set active model (updates config, restarts daemon).
pub fn set(name: &str) -> Result<()> {
    let size = parse_model_name(name)?;

    // Auto-download if not present
    if !model_download::is_downloaded(&size) {
        println!("Model '{}' not downloaded. Downloading first...", size.cli_name());
        model_download::download_model(&size, false)
            .map_err(|e| anyhow::anyhow!("Download failed: {}", e))?;
    }

    // Load and update config
    let config_path = path_utils::data_dir().join("config.json");
    let mut config_val = if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)
            .context("Failed to read config.json")?;
        serde_json::from_str::<serde_json::Value>(&content)
            .unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    config_val["local_model_size"] = serde_json::json!(size);
    let content = serde_json::to_string_pretty(&config_val)
        .context("Failed to serialize config")?;
    std::fs::write(&config_path, content)
        .context("Failed to write config.json")?;

    println!("Active model set to: {} — {}", size.cli_name(), size.display_name());

    // Restart daemon if running
    if daemon_ipc_client::ping().is_ok() {
        println!("Restarting daemon to apply model change...");
        match daemon_ipc_client::restart() {
            Ok(_) => println!("Daemon restarting."),
            Err(e) => eprintln!("Warning: daemon restart failed ({}). Restart manually.", e),
        }
    }

    Ok(())
}

/// Verify SHA256 checksum of a downloaded model.
pub fn verify(name: &str) -> Result<()> {
    let size = parse_model_name(name)?;
    let path = model_download::model_file_path(&size);

    if !path.exists() {
        anyhow::bail!("Model '{}' is not downloaded", size.cli_name());
    }

    println!("Verifying checksum for {}...", size.display_name());
    println!("  File: {}", path.display());

    // Compute SHA256
    use sha2::{Digest, Sha256};
    let file = std::fs::File::open(&path).context("Failed to open model file")?;
    let mut reader = std::io::BufReader::with_capacity(1024 * 1024, file);
    let mut hasher = Sha256::new();
    std::io::copy(&mut reader, &mut hasher).context("Failed to hash file")?;
    let hash = format!("{:x}", hasher.finalize());

    println!("  SHA256: {}", hash);
    println!("  Status: computed (no reference checksum embedded yet)");
    Ok(())
}

/// Delete a downloaded model.
pub fn rm(name: &str) -> Result<()> {
    let size = parse_model_name(name)?;
    let active = load_active_model();

    if size == active {
        anyhow::bail!(
            "Cannot delete active model '{}'. Set a different model first: ai-smartness model set <name>",
            size.cli_name()
        );
    }

    if !model_download::is_downloaded(&size) {
        anyhow::bail!("Model '{}' is not downloaded", size.cli_name());
    }

    let path = model_download::model_file_path(&size);
    let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);

    model_download::delete_model(&size)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    println!(
        "Deleted '{}' ({:.1} GB freed)",
        size.cli_name(),
        file_size as f64 / 1_000_000_000.0
    );
    Ok(())
}

// ============================================================================
// Helpers
// ============================================================================

fn parse_model_name(name: &str) -> Result<LocalModelSize> {
    LocalModelSize::from_cli_name(name).ok_or_else(|| {
        let valid: Vec<&str> = LocalModelSize::all_variants()
            .iter()
            .map(|s| s.cli_name())
            .collect();
        anyhow::anyhow!(
            "Unknown model '{}'. Available: {}",
            name,
            valid.join(", ")
        )
    })
}

fn load_active_model() -> LocalModelSize {
    let config_path = path_utils::data_dir().join("config.json");
    if let Ok(content) = std::fs::read_to_string(&config_path) {
        if let Ok(config) = serde_json::from_str::<GuardianConfig>(&content) {
            return config.local_model_size;
        }
    }
    LocalModelSize::default()
}
