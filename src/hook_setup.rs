//! Project auto-setup — installs Claude Code hooks and MCP server config.
//!
//! Called from `project add` (CLI + GUI) to ensure hooks and MCP are wired up
//! at registration time. Both hooks and MCP server are **agent-agnostic**:
//! agent_id is resolved at runtime via the cascade in `mcp/mod.rs`.

use anyhow::{Context, Result};
use std::path::Path;

/// Install Claude Code hooks into `{project_path}/.claude/settings.json`.
///
/// - Creates `.claude/` directory if absent.
/// - Merges into existing `settings.json` (preserves other keys).
/// - Overwrites only `hooks.UserPromptSubmit` and `hooks.PostToolUse`.
pub fn install_claude_hooks(project_path: &Path, project_hash: &str) -> Result<()> {
    let claude_dir = project_path.join(".claude");
    if !claude_dir.exists() {
        std::fs::create_dir_all(&claude_dir)
            .with_context(|| format!("Failed to create {}", claude_dir.display()))?;
    }

    let settings_path = claude_dir.join("settings.json");
    let bin_path = resolve_bin_path();

    // Read existing settings or start fresh
    let mut settings: serde_json::Value = if settings_path.exists() {
        let content = std::fs::read_to_string(&settings_path)
            .with_context(|| format!("Failed to read {}", settings_path.display()))?;
        serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    // Ensure settings is an object
    if !settings.is_object() {
        settings = serde_json::json!({});
    }

    // Build hook commands (no agent_id — resolved at runtime via $AI_SMARTNESS_AGENT)
    let inject_cmd = format!("{} hook inject {}", bin_path, project_hash);
    let capture_cmd = format!("{} hook capture {}", bin_path, project_hash);
    let pretool_cmd = format!("{} hook pretool {}", bin_path, project_hash);

    // Build hook entries — Claude Code expects nested { hooks: [...] } format
    let inject_hook = serde_json::json!([{
        "hooks": [{
            "type": "command",
            "command": inject_cmd
        }]
    }]);
    let capture_hook = serde_json::json!([{
        "matcher": "*",
        "hooks": [{
            "type": "command",
            "command": capture_cmd
        }]
    }]);
    let pretool_hook = serde_json::json!([{
        "hooks": [{
            "type": "command",
            "command": pretool_cmd
        }]
    }]);

    // Merge into settings.hooks
    let hooks = settings
        .as_object_mut()
        .unwrap()
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}));

    if !hooks.is_object() {
        *hooks = serde_json::json!({});
    }

    let hooks_obj = hooks.as_object_mut().unwrap();
    hooks_obj.insert("UserPromptSubmit".to_string(), inject_hook);
    hooks_obj.insert("PostToolUse".to_string(), capture_hook);
    hooks_obj.insert("PreToolUse".to_string(), pretool_hook);

    // Merge allowedTools — wildcards so agents don't need manual approval for MCP tools
    let permissions = settings
        .as_object_mut()
        .unwrap()
        .entry("permissions")
        .or_insert_with(|| serde_json::json!({}));
    if !permissions.is_object() {
        *permissions = serde_json::json!({});
    }
    let allowed = permissions
        .as_object_mut()
        .unwrap()
        .entry("allowedTools")
        .or_insert_with(|| serde_json::json!([]));
    if !allowed.is_array() {
        *allowed = serde_json::json!([]);
    }
    let tools = allowed.as_array_mut().unwrap();
    let wildcard = "mcp__ai-smartness__*";
    if !tools.iter().any(|t| t.as_str() == Some(wildcard)) {
        tools.push(serde_json::json!(wildcard));
    }

    // Write back
    let formatted = serde_json::to_string_pretty(&settings)
        .context("Failed to serialize settings")?;
    std::fs::write(&settings_path, formatted)
        .with_context(|| format!("Failed to write {}", settings_path.display()))?;

    // Also write wildcards into settings.local.json (permissions.allow)
    // This is the user-level permission file that Claude Code checks for tool approval.
    install_local_permissions(&claude_dir)?;

    Ok(())
}

/// Install MCP wildcards into `settings.local.json` (`permissions.allow`).
///
/// This file controls which tools are auto-approved without user prompts.
/// Claude Code uses `permissions.allow` (not `allowedTools`) in this file.
fn install_local_permissions(claude_dir: &Path) -> Result<()> {
    let local_path = claude_dir.join("settings.local.json");
    let wildcards = ["mcp__ai-smartness__*"];

    let mut local: serde_json::Value = if local_path.exists() {
        let content = std::fs::read_to_string(&local_path)
            .with_context(|| format!("Failed to read {}", local_path.display()))?;
        serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    if !local.is_object() {
        local = serde_json::json!({});
    }

    let permissions = local
        .as_object_mut()
        .unwrap()
        .entry("permissions")
        .or_insert_with(|| serde_json::json!({}));
    if !permissions.is_object() {
        *permissions = serde_json::json!({});
    }
    let allow = permissions
        .as_object_mut()
        .unwrap()
        .entry("allow")
        .or_insert_with(|| serde_json::json!([]));
    if !allow.is_array() {
        *allow = serde_json::json!([]);
    }
    let arr = allow.as_array_mut().unwrap();
    for wc in &wildcards {
        if !arr.iter().any(|v| v.as_str() == Some(wc)) {
            arr.push(serde_json::json!(wc));
        }
    }

    let formatted = serde_json::to_string_pretty(&local)
        .context("Failed to serialize settings.local.json")?;
    std::fs::write(&local_path, formatted)
        .with_context(|| format!("Failed to write {}", local_path.display()))?;

    Ok(())
}

/// Install MCP server config into `{project_path}/.mcp.json`.
///
/// - Creates `.mcp.json` if absent.
/// - Merges into existing `.mcp.json` (preserves other server entries).
/// - Overwrites only `mcpServers.ai-smartness`.
/// - Passes `project_hash` as explicit arg to avoid CWD-dependent hash resolution.
pub fn install_mcp_config(project_path: &Path, project_hash: &str) -> Result<()> {
    let mcp_path = project_path.join(".mcp.json");
    let bin_path = resolve_bin_path();

    // Read existing config or start fresh
    let mut config: serde_json::Value = if mcp_path.exists() {
        let content = std::fs::read_to_string(&mcp_path)
            .with_context(|| format!("Failed to read {}", mcp_path.display()))?;
        serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    if !config.is_object() {
        config = serde_json::json!({});
    }

    // Ensure mcpServers object exists
    let servers = config
        .as_object_mut()
        .unwrap()
        .entry("mcpServers")
        .or_insert_with(|| serde_json::json!({}));
    if !servers.is_object() {
        *servers = serde_json::json!({});
    }

    // Add/update ai-smartness entry with explicit project_hash
    servers.as_object_mut().unwrap().insert(
        "ai-smartness".to_string(),
        serde_json::json!({
            "command": bin_path,
            "args": ["mcp", project_hash]
        }),
    );

    let formatted = serde_json::to_string_pretty(&config)
        .context("Failed to serialize .mcp.json")?;
    std::fs::write(&mcp_path, formatted)
        .with_context(|| format!("Failed to write {}", mcp_path.display()))?;

    Ok(())
}

/// Resolve the path to the `ai-smartness` binary.
///
/// Priority:
/// 1. `which ai-smartness` (works when installed in PATH)
/// 2. `std::env::current_exe()` (works during development)
fn resolve_bin_path() -> String {
    // Try `which` first
    if let Ok(output) = std::process::Command::new("which")
        .arg("ai-smartness")
        .output()
    {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return path;
            }
        }
    }

    // Fallback to current exe
    std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "ai-smartness".to_string())
}
