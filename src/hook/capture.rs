//! Capture hook — PostToolUse handler.
//!
//! Reads tool output from stdin, filters noise, sends to daemon for processing.
//! The daemon handles extraction, coherence, and thread management.
//!
//! IMPORTANT: capture does NOT do LLM calls itself — it only filters and forwards.

use ai_smartness::processing::cleaner;
use ai_smartness::processing::daemon_ipc_client;
use ai_smartness::session::SessionState;
use ai_smartness::storage::path_utils;

/// Run the capture hook.
/// `input` is the raw stdin already read by hook/mod.rs.
pub fn run(project_hash: &str, agent_id: &str, input: &str) {
    tracing::info!(project = project_hash, agent = agent_id, "capture::run() called");

    tracing::info!(input_len = input.len(), "Capture: stdin read");

    if input.is_empty() {
        tracing::info!("Capture: stdin was EMPTY, skipping");
        print_continue();
        return;
    }

    let data: serde_json::Value = match serde_json::from_str(input) {
        Ok(v) => v,
        Err(e) => {
            tracing::info!(error = %e, input_preview = &input[..input.len().min(200)], "Capture: invalid JSON, skipping");
            print_continue();
            return;
        }
    };

    let tool_name = data
        .get("tool_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    tracing::info!(tool = tool_name, keys = ?data.as_object().map(|o| o.keys().collect::<Vec<_>>()), "Capture: processing tool output");

    // 2. Skip pure interaction tools (no useful content)
    if tool_name == "AskUserQuestion" {
        tracing::info!("Capture: skipping AskUserQuestion");
        print_continue();
        return;
    }

    // 3. Skip AI Smartness system tools (prevent cycle)
    if tool_name.starts_with("mcp__ai-smartness__") {
        tracing::info!(tool = tool_name, "Capture: skipping AI Smartness tool");
        print_continue();
        return;
    }

    // 3.5 Check if this tool is enabled for capture (per-tool toggle)
    if !is_tool_capture_enabled(project_hash, tool_name) {
        tracing::info!(tool = tool_name, "Capture: tool disabled in config, skipping");
        print_continue();
        return;
    }

    // 4. Extract tool output
    let tool_response = data.get("tool_response");
    if let Some(resp) = tool_response {
        tracing::info!(
            resp_type = match resp {
                serde_json::Value::String(_) => "string",
                serde_json::Value::Object(_) => "object",
                serde_json::Value::Array(_) => "array",
                serde_json::Value::Number(_) => "number",
                serde_json::Value::Bool(_) => "bool",
                serde_json::Value::Null => "null",
            },
            resp_preview = &resp.to_string()[..resp.to_string().len().min(300)],
            "Capture: tool_response shape"
        );
    }
    let output = extract_tool_output(tool_response);
    tracing::info!(output_len = output.len(), output_preview = &output[..output.len().min(100)], "Capture: extracted tool output");

    // 5. Noise filtering (heuristic, not LLM)
    let cleaned = cleaner::clean_tool_output(&output);
    if !cleaner::should_capture(&cleaned) {
        tracing::info!(tool = %tool_name, cleaned_len = cleaned.len(), "Capture: filtered out by should_capture");
        print_continue();
        return;
    }

    // 6. Send to daemon via IPC (fire-and-forget, non-blocking)
    tracing::info!(tool = %tool_name, content_len = cleaned.len(), "Capture sending to daemon");
    let _ = daemon_ipc_client::send_capture(project_hash, agent_id, tool_name, &cleaned);

    // 7. Update session state (tool history + file modifications)
    update_session_state(project_hash, agent_id, tool_name, &data);

    // 8. Always continue (hook must never block)
    print_continue();
}

/// Print continue response for Claude Code.
fn print_continue() {
    println!("{{\"continue\":true}}");
}

/// Extract text content from tool_response.
///
/// Claude Code sends tool_response in various formats depending on the tool:
/// - Read:  `{"file": {"content": "file text..."}}`
/// - Bash:  `{"stdout": "...", "stderr": "...", "exitCode": 0}` or just a string
/// - Glob:  `{"files": ["path1", "path2"]}` or a string
/// - Grep:  string with matches
/// - Edit:  `{"diff": "..."}`
/// - String: direct text
/// - Object with "content" as string or array of content blocks
/// - Anything else: recursively extract strings
fn extract_tool_output(response: Option<&serde_json::Value>) -> String {
    match response {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Array(arr)) => extract_from_content_blocks(arr),
        Some(serde_json::Value::Object(obj)) => {
            // Read tool: {"file": {"content": "..."}}
            if let Some(file) = obj.get("file") {
                if let Some(content) = file.get("content").and_then(|v| v.as_str()) {
                    return content.to_string();
                }
            }
            // Bash tool: {"stdout": "...", "stderr": "..."}
            if let Some(stdout) = obj.get("stdout").and_then(|v| v.as_str()) {
                let stderr = obj.get("stderr").and_then(|v| v.as_str()).unwrap_or("");
                if stderr.is_empty() {
                    return stdout.to_string();
                }
                return format!("{}\n{}", stdout, stderr);
            }
            // Edit tool: {"diff": "..."}
            if let Some(diff) = obj.get("diff").and_then(|v| v.as_str()) {
                return diff.to_string();
            }
            // Glob tool: {"files": [...]}
            if let Some(files) = obj.get("files").and_then(|v| v.as_array()) {
                return files.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
            }
            // Generic: try "content" or "output"
            if let Some(content) = obj.get("content").or_else(|| obj.get("output")) {
                match content {
                    serde_json::Value::String(s) => return s.clone(),
                    serde_json::Value::Array(arr) => return extract_from_content_blocks(arr),
                    other => return other.to_string(),
                }
            }
            // Last resort: stringify
            serde_json::to_string(obj).unwrap_or_default()
        }
        Some(v) => v.to_string(),
        None => String::new(),
    }
}

/// Extract text from Claude content block arrays: [{"type":"text","text":"..."},...]
fn extract_from_content_blocks(blocks: &[serde_json::Value]) -> String {
    let mut parts = Vec::new();
    for block in blocks {
        if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
            parts.push(text);
        } else if let Some(s) = block.as_str() {
            parts.push(s);
        }
    }
    parts.join("\n")
}

/// Check if a tool is enabled for capture in guardian_config.json.
/// Reads JSON value directly to avoid parsing the full GuardianConfig.
fn is_tool_capture_enabled(project_hash: &str, tool_name: &str) -> bool {
    let config_path =
        path_utils::project_dir(project_hash).join("guardian_config.json");
    if let Ok(content) = std::fs::read_to_string(&config_path) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(tools) = v.get("capture").and_then(|c| c.get("tools")) {
                let key = match tool_name {
                    "Read" => "read",
                    "Edit" => "edit",
                    "Write" => "write",
                    "Bash" => "bash",
                    "Grep" => "grep",
                    "Glob" => "glob",
                    "WebFetch" => "web_fetch",
                    "WebSearch" => "web_search",
                    "Task" => "task",
                    "NotebookEdit" => "notebook_edit",
                    _ => return true,
                };
                return tools.get(key).and_then(|v| v.as_bool()).unwrap_or(true);
            }
        }
    }
    true
}

/// Update session state with tool call info and file modifications.
fn update_session_state(
    project_hash: &str,
    agent_id: &str,
    tool_name: &str,
    data: &serde_json::Value,
) {
    let agent_data = path_utils::agent_data_dir(project_hash, agent_id);
    let mut session = SessionState::load(&agent_data, agent_id, project_hash);

    // Extract target (file_path for file tools, command for Bash, etc.)
    let target = data
        .get("tool_input")
        .and_then(|i| {
            i.get("file_path")
                .or_else(|| i.get("command"))
                .or_else(|| i.get("pattern"))
                .and_then(|v| v.as_str())
        })
        .unwrap_or("");

    // Record tool call
    session.record_tool_call(tool_name, target);

    // Track file modifications for Edit/Write/Read
    if let Some(file_path) = data
        .get("tool_input")
        .and_then(|i| i.get("file_path"))
        .and_then(|v| v.as_str())
    {
        let action = match tool_name {
            "Edit" => "edit",
            "Write" => "write",
            "Read" => "read",
            _ => "",
        };
        if !action.is_empty() {
            session.record_file_modification(file_path, action, "");
        }
    }

    session.save(&agent_data);
}
