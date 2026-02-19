//! Guard Write — blocks Edit/Write without a validated plan.
//!
//! Called from PreToolUse hook (via pretool dispatcher).
//! Checks `{agent_data_dir}/plan_state.json` for a validated plan
//! with non-expired timestamp and matching file paths.

use ai_smartness::storage::path_utils;

/// Check if an Edit/Write tool call should be allowed.
/// Returns true if allowed, false if blocked.
/// On block, prints error to stderr and exits with code 2.
pub fn check(project_hash: &str, agent_id: &str, data: &serde_json::Value) -> bool {
    let tool_name = data
        .get("tool_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Only guard Edit and Write tools
    if tool_name != "Edit" && tool_name != "Write" {
        return true;
    }

    let data_dir = path_utils::agent_data_dir(project_hash, agent_id);
    let plan_path = data_dir.join("plan_state.json");

    // No plan file → block
    if !plan_path.exists() {
        tracing::info!(tool = tool_name, "Guard-write: no plan_state.json found, blocking");
        eprintln!(
            "[guard-write] No validated plan found for {}. Create a plan first (ai_plan tool).",
            tool_name
        );
        return false;
    }

    // Read plan
    let plan: serde_json::Value = match std::fs::read_to_string(&plan_path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or(serde_json::json!({})),
        Err(_) => {
            tracing::warn!("Guard-write: failed to read plan_state.json");
            return true; // Don't block on read errors
        }
    };

    // Check expiration
    if let Some(expires) = plan.get("expires_at").and_then(|e| e.as_str()) {
        if let Ok(exp) = chrono::DateTime::parse_from_rfc3339(expires) {
            if exp < chrono::Utc::now() {
                tracing::info!("Guard-write: plan expired");
                eprintln!("[guard-write] Plan expired. Create a new plan.");
                return false;
            }
        }
    }

    // Check validated_files (optional — if present, enforce file allowlist)
    let file_path = data
        .get("tool_input")
        .and_then(|i| i.get("file_path"))
        .and_then(|f| f.as_str());

    if let (Some(fp), Some(files)) = (file_path, plan.get("validated_files").and_then(|v| v.as_array())) {
        let allowed = files.iter().any(|f| {
            let pattern = f.as_str().unwrap_or("");
            matches_wildcard(pattern, fp)
        });
        if !allowed {
            tracing::info!(file = fp, "Guard-write: file not in validated plan");
            eprintln!("[guard-write] File not in validated plan: {}", fp);
            return false;
        }
    }

    tracing::debug!(tool = tool_name, "Guard-write: allowed");
    true
}

/// Simple wildcard matching: supports trailing `*` and `**`.
fn matches_wildcard(pattern: &str, path: &str) -> bool {
    if pattern == "*" || pattern == "**" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix("**") {
        return path.starts_with(prefix);
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        if !path.starts_with(prefix) {
            return false;
        }
        // Single * doesn't match across /
        let remainder = &path[prefix.len()..];
        return !remainder.contains('/');
    }
    pattern == path
}
