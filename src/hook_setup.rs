//! Project auto-setup — installs Claude Code hooks and MCP server config.
//!
//! Called from `project add` (CLI + GUI) to ensure hooks and MCP are wired up
//! at registration time. Both hooks and MCP server are **agent-agnostic**:
//! agent_id is resolved at runtime via the cascade in `mcp/mod.rs`.

use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::Path;

use crate::registry::registry::{AgentRegistry, HierarchyNode};

/// Stale MCP server name patterns from older ai-smartness versions (Python era).
/// These are purged from allowedTools / permissions.allow on every hook install.
pub const LEGACY_WILDCARDS: &[&str] = &["mcp__mcp-smartness__*"];

/// Install Claude Code hooks into `{project_path}/.claude/settings.json`.
///
/// - Creates `.claude/` directory if absent.
/// - Merges into existing `settings.json` (preserves other keys).
/// - Overwrites `hooks.UserPromptSubmit`, `hooks.PostToolUse`, `hooks.PreToolUse`, and `hooks.Stop`.
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
    let stop_cmd = format!("{} hook stop {}", bin_path, project_hash);

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
    let stop_hook = serde_json::json!([{
        "hooks": [{
            "type": "command",
            "command": stop_cmd
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
    hooks_obj.insert("Stop".to_string(), stop_hook);

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
    // Remove stale legacy entries
    tools.retain(|t| t.as_str().map(|s| !LEGACY_WILDCARDS.contains(&s)).unwrap_or(true));

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
    // Remove stale legacy entries
    arr.retain(|v| v.as_str().map(|s| !LEGACY_WILDCARDS.contains(&s)).unwrap_or(true));

    let formatted = serde_json::to_string_pretty(&local)
        .context("Failed to serialize settings.local.json")?;
    std::fs::write(&local_path, formatted)
        .with_context(|| format!("Failed to write {}", local_path.display()))?;

    Ok(())
}

/// Sync full_permissions into a project's `settings.local.json`.
///
/// When `enabled` is true, adds all tool patterns to `permissions.allow` in `.claude/settings.local.json`.
/// When false, removes them.
pub fn sync_full_permissions(project_path: &Path, enabled: bool) -> Result<()> {
    let local_path = project_path.join(".claude/settings.local.json");
    let patterns = [
        "Bash(*)",
        "Edit(*)",
        "Write(*)",
        "MultiEdit(*)",
        "WebFetch(*)",
        "WebSearch(*)",
        "NotebookEdit(*)",
        "Task(*)",
    ];

    let mut local: serde_json::Value = if local_path.exists() {
        let content = std::fs::read_to_string(&local_path)
            .with_context(|| format!("Failed to read {}", local_path.display()))?;
        serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
    } else if enabled {
        serde_json::json!({})
    } else {
        return Ok(());
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

    if enabled {
        for pat in &patterns {
            if !arr.iter().any(|v| v.as_str() == Some(pat)) {
                arr.push(serde_json::json!(pat));
            }
        }
    } else {
        arr.retain(|v| v.as_str().map(|s| !patterns.contains(&s)).unwrap_or(true));
    }

    // Ensure parent dir exists
    if let Some(parent) = local_path.parent() {
        std::fs::create_dir_all(parent).ok();
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

// ─── CLAUDE.md dynamic regeneration ─────────────────────────────────────

const CLAUDE_MD_BEGIN: &str = "<!-- ai-smartness:begin -->";
const CLAUDE_MD_END: &str = "<!-- ai-smartness:end -->";

/// Format hierarchy nodes as a text tree with box-drawing characters.
///
/// Example output:
/// ```text
/// cor (coordinator)
/// ├── dev — developer
/// └── doc — documentation
/// ```
pub fn format_hierarchy_tree(nodes: &[HierarchyNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        out.push_str(&format!("{} ({})\n", node.id, node.role));
        format_subtree(&node.subordinates, "", &mut out);
    }
    out
}

fn format_subtree(children: &[HierarchyNode], prefix: &str, out: &mut String) {
    for (i, child) in children.iter().enumerate() {
        let is_last = i == children.len() - 1;
        let connector = if is_last { "└── " } else { "├── " };
        let child_prefix = if is_last {
            format!("{}    ", prefix)
        } else {
            format!("{}│   ", prefix)
        };
        out.push_str(&format!("{}{}{} — {}\n", prefix, connector, child.id, child.role));
        format_subtree(&child.subordinates, &child_prefix, out);
    }
}

/// Generate the content for the ai-smartness marked section.
fn generate_claude_md_section(tree: &[HierarchyNode]) -> String {
    let mut s = String::new();
    s.push_str("# ai-smartness — Agent Assignment\n\n");
    s.push_str("This project uses [ai-smartness](https://github.com/VzKtS/ai-smartness) for multi-agent memory.\n\n");
    s.push_str("## Assign an agent to this session\n\n");
    s.push_str("At the start of each new session, call the `ai_agent_select` MCP tool:\n\n");
    s.push_str("```\nai_agent_select(agent_id=\"<agent_id>\", session_id=\"<session_id from context>\")\n```\n\n");
    s.push_str("Available agents: call `agent_list` to see them.\n");
    s.push_str("The session_id is injected in your context by the hook (look for session_id in system reminders).\n");
    if !tree.is_empty() {
        s.push_str("\n## Agent hierarchy\n\n```\n");
        s.push_str(&format_hierarchy_tree(tree));
        s.push_str("```\n");
    }
    s
}

/// Split file content at markers, returning (before_begin, after_end).
fn split_at_markers(content: &str) -> Option<(String, String)> {
    let begin_pos = content.find(CLAUDE_MD_BEGIN)?;
    let end_pos = content.find(CLAUDE_MD_END)?;
    if end_pos <= begin_pos {
        return None;
    }
    let before = content[..begin_pos].to_string();
    let after = content[end_pos + CLAUDE_MD_END.len()..].to_string();
    Some((before, after))
}

/// Resolve the project filesystem path from a project_hash using the registry.
fn resolve_project_path(registry_conn: &Connection, project_hash: &str) -> Option<std::path::PathBuf> {
    let path: Option<String> = registry_conn
        .query_row(
            "SELECT path FROM projects WHERE hash = ?1",
            rusqlite::params![project_hash],
            |row| row.get(0),
        )
        .ok();
    path.map(std::path::PathBuf::from)
}

/// Regenerate the ai-smartness section in CLAUDE.md for a project.
///
/// - Reads the hierarchy from the registry database.
/// - Replaces content between `<!-- ai-smartness:begin -->` and `<!-- ai-smartness:end -->` markers.
/// - If markers don't exist, appends the section at the end of the file.
/// - If CLAUDE.md doesn't exist, creates it with just the marked section.
/// - Idempotent: safe to call multiple times.
pub fn regenerate_claude_md(
    project_path: &Path,
    registry_conn: &Connection,
    project_hash: &str,
) -> Result<()> {
    let claude_md_path = project_path.join("CLAUDE.md");

    let tree = AgentRegistry::build_hierarchy_tree(registry_conn, project_hash)
        .map_err(|e| anyhow::anyhow!("Failed to build hierarchy: {}", e))?;

    let section = generate_claude_md_section(&tree);

    let existing = if claude_md_path.exists() {
        std::fs::read_to_string(&claude_md_path)
            .with_context(|| format!("Failed to read {}", claude_md_path.display()))?
    } else {
        String::new()
    };

    let new_content = if let Some((before, after)) = split_at_markers(&existing) {
        format!("{}{}\n{}{}{}", before, CLAUDE_MD_BEGIN, section, CLAUDE_MD_END, after)
    } else if existing.is_empty() {
        format!("{}\n{}{}\n", CLAUDE_MD_BEGIN, section, CLAUDE_MD_END)
    } else {
        format!("{}\n\n{}\n{}{}\n", existing.trim_end(), CLAUDE_MD_BEGIN, section, CLAUDE_MD_END)
    };

    std::fs::write(&claude_md_path, new_content)
        .with_context(|| format!("Failed to write {}", claude_md_path.display()))?;

    Ok(())
}

/// Remove the ai-smartness section from CLAUDE.md.
///
/// Strips content between markers (inclusive). If the file becomes empty, it is deleted.
pub fn strip_claude_md_section(project_path: &Path) -> Result<()> {
    let claude_md_path = project_path.join("CLAUDE.md");
    if !claude_md_path.exists() {
        return Ok(());
    }

    let content = std::fs::read_to_string(&claude_md_path)
        .with_context(|| format!("Failed to read {}", claude_md_path.display()))?;

    if let Some((before, after)) = split_at_markers(&content) {
        let remaining = format!("{}{}", before.trim_end(), after.trim_start());
        let trimmed = remaining.trim();
        if trimmed.is_empty() {
            std::fs::remove_file(&claude_md_path).ok();
        } else {
            std::fs::write(&claude_md_path, format!("{}\n", trimmed))
                .with_context(|| format!("Failed to write {}", claude_md_path.display()))?;
        }
    }

    Ok(())
}

/// Regenerate CLAUDE.md after an agent mutation. Logs errors but never fails.
pub fn refresh_claude_md(registry_conn: &Connection, project_hash: &str) {
    let project_path = match resolve_project_path(registry_conn, project_hash) {
        Some(p) => p,
        None => {
            tracing::debug!(
                project = %&project_hash[..8.min(project_hash.len())],
                "refresh_claude_md: project path not found"
            );
            return;
        }
    };

    match regenerate_claude_md(&project_path, registry_conn, project_hash) {
        Ok(()) => tracing::info!(path = %project_path.display(), "CLAUDE.md regenerated"),
        Err(e) => tracing::warn!(error = %e, "Failed to regenerate CLAUDE.md"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_install_hooks_removes_legacy_wildcard() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let project_path = dir.path();
        let claude_dir = project_path.join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();

        // Pre-populate settings.json with both legacy and current wildcards
        let initial = serde_json::json!({
            "permissions": {
                "allowedTools": [
                    "mcp__mcp-smartness__*",
                    "mcp__ai-smartness__*"
                ]
            }
        });
        let settings_path = claude_dir.join("settings.json");
        std::fs::write(&settings_path, serde_json::to_string_pretty(&initial).unwrap()).unwrap();

        install_claude_hooks(project_path, "testhash").expect("install_claude_hooks failed");

        let result: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&settings_path).unwrap()
        ).unwrap();
        let tools = result["permissions"]["allowedTools"].as_array().unwrap();
        let tool_strs: Vec<&str> = tools.iter().filter_map(|v| v.as_str()).collect();

        assert!(
            !tool_strs.contains(&"mcp__mcp-smartness__*"),
            "legacy wildcard should be removed, got: {:?}", tool_strs
        );
        assert!(
            tool_strs.contains(&"mcp__ai-smartness__*"),
            "current wildcard should remain, got: {:?}", tool_strs
        );
    }

    // ─── CLAUDE.md tests ──────────────────────────────────

    fn make_tree() -> Vec<HierarchyNode> {
        vec![HierarchyNode {
            id: "cor".to_string(),
            name: "cor".to_string(),
            role: "coordinator".to_string(),
            mode: "coordinator".to_string(),
            team: None,
            subordinates: vec![
                HierarchyNode {
                    id: "dev".to_string(),
                    name: "dev".to_string(),
                    role: "developer".to_string(),
                    mode: "supervised".to_string(),
                    team: None,
                    subordinates: vec![],
                    active_tasks: 0,
                },
                HierarchyNode {
                    id: "doc".to_string(),
                    name: "doc".to_string(),
                    role: "documentation".to_string(),
                    mode: "supervised".to_string(),
                    team: None,
                    subordinates: vec![],
                    active_tasks: 0,
                },
            ],
            active_tasks: 0,
        }]
    }

    #[test]
    fn test_format_hierarchy_tree() {
        let output = format_hierarchy_tree(&make_tree());
        assert!(output.contains("cor (coordinator)"));
        assert!(output.contains("├── dev — developer"));
        assert!(output.contains("└── doc — documentation"));
    }

    #[test]
    fn test_format_hierarchy_tree_empty() {
        let output = format_hierarchy_tree(&[]);
        assert!(output.is_empty());
    }

    #[test]
    fn test_split_at_markers_found() {
        let content = "Hello\n<!-- ai-smartness:begin -->\nstuff\n<!-- ai-smartness:end -->\nBye";
        let (before, after) = split_at_markers(content).unwrap();
        assert_eq!(before, "Hello\n");
        assert_eq!(after, "\nBye");
    }

    #[test]
    fn test_split_at_markers_not_found() {
        assert!(split_at_markers("No markers here").is_none());
    }

    #[test]
    fn test_regenerate_creates_file() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::storage::migrations::migrate_registry_db(&conn).unwrap();

        regenerate_claude_md(dir.path(), &conn, "testhash").unwrap();

        let content = std::fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();
        assert!(content.contains(CLAUDE_MD_BEGIN));
        assert!(content.contains(CLAUDE_MD_END));
        assert!(content.contains("ai_agent_select"));
    }

    #[test]
    fn test_regenerate_preserves_user_content() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let claude_md = dir.path().join("CLAUDE.md");
        std::fs::write(&claude_md, "# My Project\n\nCustom stuff here.\n").unwrap();

        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::storage::migrations::migrate_registry_db(&conn).unwrap();

        regenerate_claude_md(dir.path(), &conn, "testhash").unwrap();

        let content = std::fs::read_to_string(&claude_md).unwrap();
        assert!(content.contains("# My Project"));
        assert!(content.contains("Custom stuff here."));
        assert!(content.contains(CLAUDE_MD_BEGIN));
    }

    #[test]
    fn test_regenerate_idempotent() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::storage::migrations::migrate_registry_db(&conn).unwrap();

        regenerate_claude_md(dir.path(), &conn, "testhash").unwrap();
        let first = std::fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();

        regenerate_claude_md(dir.path(), &conn, "testhash").unwrap();
        let second = std::fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();

        assert_eq!(first, second, "Regeneration should be idempotent");
    }

    #[test]
    fn test_strip_removes_section() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let claude_md = dir.path().join("CLAUDE.md");
        let content = "# My Project\n\n<!-- ai-smartness:begin -->\ngenerated stuff\n<!-- ai-smartness:end -->\n\n# Other Section\n";
        std::fs::write(&claude_md, content).unwrap();

        strip_claude_md_section(dir.path()).unwrap();

        let result = std::fs::read_to_string(&claude_md).unwrap();
        assert!(!result.contains("ai-smartness:begin"));
        assert!(result.contains("# My Project"));
        assert!(result.contains("# Other Section"));
    }

    #[test]
    fn test_strip_deletes_empty_file() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let claude_md = dir.path().join("CLAUDE.md");
        let content = "<!-- ai-smartness:begin -->\ngenerated stuff\n<!-- ai-smartness:end -->";
        std::fs::write(&claude_md, content).unwrap();

        strip_claude_md_section(dir.path()).unwrap();

        assert!(!claude_md.exists(), "Empty CLAUDE.md should be deleted");
    }
}
