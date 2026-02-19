use anyhow::{bail, Result};

pub fn run(mode: &str) -> Result<()> {
    let valid_modes = ["standard", "minimal", "aggressive", "custom"];
    if !valid_modes.contains(&mode) {
        bail!(
            "Unknown mode: {}. Valid modes: {}",
            mode,
            valid_modes.join(", ")
        );
    }

    // Read or create config file
    let config_path = ai_smartness::storage::path_utils::data_dir().join("config.json");

    let mut config: serde_json::Value = if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)?;
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    config["memory_mode"] = serde_json::Value::String(mode.to_string());

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&config_path, serde_json::to_string_pretty(&config)?)?;

    println!("Memory mode set to: {}", mode);

    Ok(())
}
