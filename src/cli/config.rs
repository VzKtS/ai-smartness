use anyhow::{bail, Context, Result};

/// `config show` — display the full config.
pub fn run_show() -> Result<()> {
    let config_path = ai_smartness::storage::path_utils::data_dir().join("config.json");

    let config = if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read {}", config_path.display()))?;
        let val: serde_json::Value = serde_json::from_str(&content)
            .with_context(|| "Invalid JSON in config.json")?;
        serde_json::to_string_pretty(&val)?
    } else {
        let default = ai_smartness::config::GuardianConfig::default();
        serde_json::to_string_pretty(&default)?
    };

    println!("{}", config);
    Ok(())
}

/// `config get <key>` — display a single config value.
///
/// Key uses dot notation: `hooks.guard_write_enabled`, `capture.tools.bash`
pub fn run_get(key: &str) -> Result<()> {
    let config_path = ai_smartness::storage::path_utils::data_dir().join("config.json");

    let config: serde_json::Value = if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)?;
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::to_value(&ai_smartness::config::GuardianConfig::default())?
    };

    // Navigate dot-separated path
    let value = resolve_path(&config, key);
    match value {
        Some(v) => println!("{}", serde_json::to_string_pretty(v)?),
        None => bail!("Key not found: {}", key),
    }
    Ok(())
}

/// `config set <key> <value>` — set a config value and propagate.
///
/// Key uses dot notation: `hooks.guard_write_enabled`, `capture.tools.bash`
/// Value is parsed as JSON (bool, number, string).
pub fn run_set(key: &str, value: &str) -> Result<()> {
    let config_path = ai_smartness::storage::path_utils::data_dir().join("config.json");

    let mut config: serde_json::Value = if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)?;
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    // Parse the value as JSON, falling back to string
    let parsed: serde_json::Value = serde_json::from_str(value)
        .unwrap_or(serde_json::Value::String(value.to_string()));

    // Set the value at the dot-separated path
    set_path(&mut config, key, parsed.clone())?;

    // Write config.json
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&config_path, serde_json::to_string_pretty(&config)?)?;

    // Propagate hooks/capture to per-project guardian_config.json
    ai_smartness::config_sync::sync_guardian_configs(&config);

    println!("{} = {}", key, serde_json::to_string(&parsed)?);
    Ok(())
}

/// Resolve a dot-separated path in a JSON value.
fn resolve_path<'a>(value: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    let mut current = value;
    for segment in path.split('.') {
        current = current.get(segment)?;
    }
    Some(current)
}

/// Set a value at a dot-separated path, creating intermediate objects as needed.
fn set_path(root: &mut serde_json::Value, path: &str, value: serde_json::Value) -> Result<()> {
    let segments: Vec<&str> = path.split('.').collect();
    if segments.is_empty() {
        bail!("Empty key path");
    }

    let mut current = root;
    for segment in &segments[..segments.len() - 1] {
        if !current.is_object() {
            *current = serde_json::json!({});
        }
        current = current
            .as_object_mut()
            .unwrap()
            .entry(segment.to_string())
            .or_insert_with(|| serde_json::json!({}));
    }

    if !current.is_object() {
        *current = serde_json::json!({});
    }
    current
        .as_object_mut()
        .unwrap()
        .insert(segments.last().unwrap().to_string(), value);

    Ok(())
}
