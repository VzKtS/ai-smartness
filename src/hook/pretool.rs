//! PreToolUse dispatcher — routes to guard_write and virtual_paths.
//!
//! Single hook entry point for PreToolUse. Reads stdin (JSON),
//! dispatches to appropriate handler based on tool_name.

/// Run the pretool hook.
/// `input` is the raw stdin already read by hook/mod.rs.
pub fn run(project_hash: &str, agent_id: &str, input: &str) {
    tracing::info!(project = project_hash, agent = agent_id, "pretool::run() called");

    if input.is_empty() {
        print!("{{}}");
        return;
    }

    let data: serde_json::Value = match serde_json::from_str(input) {
        Ok(v) => v,
        Err(_) => {
            // Can't parse → passthrough
            print!("{}", input);
            return;
        }
    };

    let tool_name = data
        .get("tool_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    tracing::debug!(tool = tool_name, "PreTool dispatch");

    match tool_name {
        // Guard Write: check plan for Edit/Write (respects hooks.guard_write_enabled toggle)
        "Edit" | "Write" => {
            if is_guard_write_enabled(project_hash) {
                if !super::guard_write::check(project_hash, agent_id, &data) {
                    // Block: exit 2 signals rejection to Claude Code
                    std::process::exit(2);
                }
            }
            // Allow through
        }
        // Virtual Paths: intercept Read for .ai/ paths
        "Read" => {
            if let Some(content) = super::virtual_paths::check(project_hash, agent_id, &data) {
                // Return virtual content as tool response
                let response = serde_json::json!({
                    "tool_response": content,
                });
                print!("{}", serde_json::to_string(&response).unwrap_or_default());
                return;
            }
            // Not a virtual path → passthrough
        }
        _ => {}
    }

    // Default: passthrough unchanged
    print!("{}", input);
}

/// Check if guard_write is enabled in guardian_config.json.
/// Reads JSON value directly to avoid parsing the full GuardianConfig.
fn is_guard_write_enabled(project_hash: &str) -> bool {
    let config_path = ai_smartness::storage::path_utils::project_dir(project_hash)
        .join("guardian_config.json");
    if let Ok(content) = std::fs::read_to_string(&config_path) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
            return v
                .get("hooks")
                .and_then(|h| h.get("guard_write_enabled"))
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
        }
    }
    true // default: enabled
}
