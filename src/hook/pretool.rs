//! PreToolUse dispatcher — routes to virtual_paths.
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
