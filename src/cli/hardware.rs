use anyhow::{bail, Context, Result};

/// `hardware detect` — display all detected hardware.
pub fn detect() -> Result<()> {
    let hw = ai_smartness::processing::hardware::detect();

    println!("CPU: {} ({} cores / {} threads)", hw.cpu.model, hw.cpu.cores, hw.cpu.threads);
    println!("RAM: {} MB total, {} MB available", hw.ram.total_mb, hw.ram.available_mb);
    println!();

    if hw.gpus.is_empty() {
        println!("GPUs: none detected");
    } else {
        println!("GPUs:");
        for gpu in &hw.gpus {
            if gpu.vram_total_mb > 0 {
                let free = gpu.vram_total_mb.saturating_sub(gpu.vram_used_mb);
                let pct = (gpu.vram_used_mb as f64 / gpu.vram_total_mb as f64 * 100.0) as u32;
                let bar_width = 20;
                let filled = (pct as usize * bar_width / 100).min(bar_width);
                let bar: String = format!(
                    "[{}{}]",
                    "#".repeat(filled),
                    " ".repeat(bar_width - filled),
                );
                println!(
                    "  GPU {}: {} [{}]",
                    gpu.index, gpu.name, gpu.vendor,
                );
                if !gpu.driver_version.is_empty() {
                    println!("         Driver: {}", gpu.driver_version);
                }
                println!(
                    "         VRAM: {} {} / {} MB ({} MB free)",
                    bar, gpu.vram_used_mb, gpu.vram_total_mb, free,
                );
            } else {
                println!(
                    "  GPU {}: {} [{}] (integrated, no dedicated VRAM)",
                    gpu.index, gpu.name, gpu.vendor,
                );
            }
        }
    }

    Ok(())
}

/// `hardware show` — display current device assignments from config.
pub fn show() -> Result<()> {
    let config_path = ai_smartness::storage::path_utils::data_dir().join("config.json");

    let config: ai_smartness::config::GuardianConfig = if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read {}", config_path.display()))?;
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        ai_smartness::config::GuardianConfig::default()
    };

    println!("Device assignments:");
    println!("  Runtime  (ONNX/engram): {}", config.hardware.runtime_device);
    println!("  Provider (llama.cpp):   {}", config.hardware.provider_device);

    Ok(())
}

/// `hardware set <tier> <device>` — assign a device to a computation tier.
///
/// tier: "runtime" or "provider"
/// device: "auto", "cpu", "gpu:0", "gpu:1", etc.
pub fn set(tier: &str, device: &str) -> Result<()> {
    let selection: ai_smartness::config::DeviceSelection = device.parse()
        .map_err(|e: String| anyhow::anyhow!(e))?;

    let config_path = ai_smartness::storage::path_utils::data_dir().join("config.json");

    let mut config: serde_json::Value = if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)?;
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    // Ensure hardware section exists
    if !config.get("hardware").is_some() {
        config.as_object_mut().unwrap().insert(
            "hardware".to_string(),
            serde_json::json!({"runtime_device": "auto", "provider_device": "auto"}),
        );
    }

    let field = match tier {
        "runtime" => "runtime_device",
        "provider" => "provider_device",
        _ => bail!("Invalid tier '{}'. Expected: runtime, provider", tier),
    };

    config["hardware"][field] = serde_json::to_value(&selection)?;

    // Write back
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&config_path, serde_json::to_string_pretty(&config)?)?;

    println!("{} device set to: {}", tier, selection);
    Ok(())
}
