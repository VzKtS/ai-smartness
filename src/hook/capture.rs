//! Capture hook — PostToolUse handler.
//!
//! Reads tool output from stdin, filters noise, sends to daemon for processing.
//! The daemon handles extraction, coherence, and thread management.
//!
//! IMPORTANT: capture does NOT do LLM calls itself — it only filters and forwards.
//!
//! Engram-on-Thinking (T14): After capture, reads the JSONL transcript to extract
//! the latest thinking block, queries the engram retriever via daemon IPC, and
//! injects matching threads as `<engram>` hints in stdout (fed back to Claude).

use ai_smartness::processing::cleaner;
use ai_smartness::processing::daemon_ipc_client;
use ai_smartness::storage::path_utils;
use ai_smartness::storage::transcript;

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Run the capture hook.
/// `input` is the raw stdin already read by hook/mod.rs.
pub fn run(project_hash: &str, agent_id: &str, input: &str, session_id: Option<&str>) {
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

    // 2. Skip pure interaction/navigation tools (no useful content for memory)
    let skip_tools = ["AskUserQuestion", "Glob", "Grep"];
    if skip_tools.iter().any(|t| tool_name == *t) {
        tracing::info!(tool = tool_name, "Capture: skipping (excluded tool)");
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
        // Even if capture is disabled, still try engram injection
        try_print_engram_hint(session_id, project_hash, agent_id);
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

    // 4.5 Bash preprocessing — skip short successful commands (not worth GPU time)
    if tool_name == "Bash" {
        let exit_code = data
            .get("tool_response")
            .and_then(|r| r.get("exitCode"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        if exit_code == 0 && output.len() < 200 {
            tracing::debug!(
                exit_code = exit_code,
                output_len = output.len(),
                "Capture: Bash skipped (success + short output)"
            );
            try_print_engram_hint(session_id, project_hash, agent_id);
            print_continue();
            return;
        }
    }

    // 5. Noise filtering (heuristic, not LLM)
    let cleaned = cleaner::clean_tool_output(&output);
    if !cleaner::should_capture(&cleaned) {
        tracing::info!(tool = %tool_name, cleaned_len = cleaned.len(), "Capture: filtered out by should_capture");
        try_print_engram_hint(session_id, project_hash, agent_id);
        print_continue();
        return;
    }

    // 6. Extract file_path / URL / query from tool_input (stored as thread reference)
    //    - Read/Edit/Write: file_path
    //    - WebFetch: url
    //    - WebSearch: query
    let file_path = data
        .get("tool_input")
        .and_then(|i| {
            i.get("file_path").and_then(|v| v.as_str())
                .or_else(|| i.get("url").and_then(|v| v.as_str()))
                .or_else(|| i.get("query").and_then(|v| v.as_str()))
        });

    // 7. Send to daemon via IPC (fire-and-forget, non-blocking)
    tracing::info!(tool = %tool_name, content_len = cleaned.len(), file_path = ?file_path, "Capture sending to daemon");
    let _ = daemon_ipc_client::send_capture(project_hash, agent_id, tool_name, &cleaned, file_path);

    // 8. Engram-on-Thinking injection (T14)
    try_print_engram_hint(session_id, project_hash, agent_id);

    // 9. Always continue (hook must never block)
    print_continue();
}

/// Try to inject engram hints based on the agent's latest thinking block.
///
/// Reads the JSONL transcript, extracts thinking, queries the engram retriever
/// via daemon IPC, and prints `<engram>` hints to stdout if matches found.
/// Deduplicates by thinking hash to avoid querying the same thinking block
/// on multiple tool calls within the same turn.
///
/// This function NEVER fails — all errors are silently logged.
fn try_print_engram_hint(session_id: Option<&str>, project_hash: &str, agent_id: &str) {
    let sid = match session_id {
        Some(s) if !s.is_empty() => s,
        _ => return,
    };

    // Check if engram injection is enabled in config
    if !is_engram_injection_enabled(project_hash) {
        return;
    }

    // Extract thinking block from JSONL transcript
    let thinking = match transcript::extract_last_thinking(sid) {
        Some(t) if t.len() > 50 => t, // Skip very short thinking blocks
        _ => return,
    };

    // Deduplicate: hash check against last processed thinking
    let hash = compute_hash(&thinking);
    let state_path = path_utils::agent_data_dir(project_hash, agent_id)
        .join("engram_thinking.hash");

    if let Ok(stored_hash) = std::fs::read_to_string(&state_path) {
        if stored_hash.trim() == hash.to_string() {
            tracing::debug!("Engram: same thinking hash, skipping");
            return;
        }
    }

    // Update hash state file
    if let Some(parent) = state_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(&state_path, hash.to_string()).ok();

    // Truncate thinking for query (engram doesn't need the full block)
    let query_text = if thinking.len() > 2000 {
        &thinking[..2000]
    } else {
        &thinking
    };

    // Query engram via daemon IPC (timeout handled by daemon_ipc_client: 2s)
    let results = match daemon_ipc_client::engram_query(project_hash, agent_id, query_text, 5) {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!(error = %e, "Engram query failed (daemon not running?)");
            return;
        }
    };

    if results.is_empty() {
        tracing::debug!("Engram: no matches");
        return;
    }

    // Apply convergence threshold
    if !should_inject(&results) {
        tracing::debug!(
            count = results.len(),
            max_pass = results.iter().map(|r| r.pass_count).max().unwrap_or(0),
            "Engram: below convergence threshold"
        );
        return;
    }

    // Format and print engram hint
    let mut hint = String::from("<engram>\n");
    for r in results.iter().filter(|r| r.pass_count >= 2).take(3) {
        let summary = r.summary.as_deref().unwrap_or("");
        let summary_short = if summary.len() > 80 {
            format!("{}...", &summary[..summary.chars().take(77).map(|c| c.len_utf8()).sum()])
        } else {
            summary.to_string()
        };
        hint.push_str(&format!("- \"{}\": {}\n", r.title, summary_short));
    }
    hint.push_str("</engram>");

    tracing::info!(
        matches = results.len(),
        injected = results.iter().filter(|r| r.pass_count >= 2).take(3).count(),
        "Engram hint injected into PostToolUse stdout"
    );

    println!("{}", hint);
}

/// Convergence threshold — dynamic injection decision across multiple threads.
///
/// Instead of a static per-thread threshold, uses convergence:
/// - 1 thread with 5+ validators → inject (one strong match)
/// - 2 threads with 3+ validators → inject (two moderate matches converge)
/// - 4 threads with 2+ validators → inject (cluster of weak matches = signal)
fn should_inject(threads: &[daemon_ipc_client::EngramResult]) -> bool {
    let strong = threads.iter().filter(|t| t.pass_count >= 5).count();
    let moderate = threads.iter().filter(|t| t.pass_count >= 3).count();
    let weak = threads.iter().filter(|t| t.pass_count >= 2).count();
    strong >= 1 || moderate >= 2 || weak >= 4
}

/// Compute a simple hash of a string for deduplication.
fn compute_hash(s: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

/// Check if engram injection is enabled in config.
/// Default: false (opt-in feature during R&D).
fn is_engram_injection_enabled(_project_hash: &str) -> bool {
    let config_path = path_utils::data_dir().join("config.json");
    if let Ok(content) = std::fs::read_to_string(&config_path) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
            return v
                .get("engram")
                .and_then(|e| e.get("thinking_injection"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
        }
    }
    false
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

/// Check if a tool is enabled for capture in config.json.
/// Reads the global config (~/.config/ai-smartness/config.json).
/// Default: false (all tool captures off unless explicitly enabled).
fn is_tool_capture_enabled(_project_hash: &str, tool_name: &str) -> bool {
    let config_path = path_utils::data_dir().join("config.json");
    if let Ok(content) = std::fs::read_to_string(&config_path) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(tools) = v.get("capture").and_then(|c| c.get("tools")) {
                let key = match tool_name {
                    "Read" => "read",
                    "Edit" => "edit",
                    "Write" => "write",
                    "Bash" => "bash",
                    "WebFetch" => "web_fetch",
                    "WebSearch" => "web_search",
                    "Task" => "task",
                    "Agent" => "task",
                    "NotebookEdit" => "notebook_edit",
                    _ => return false,
                };
                return tools.get(key).and_then(|v| v.as_bool()).unwrap_or(false);
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_inject_strong() {
        // 1 thread with 5 validators → inject
        let threads = vec![daemon_ipc_client::EngramResult {
            id: "a".into(), title: "T".into(), summary: None, pass_count: 5, weighted_score: 0.8,
        }];
        assert!(should_inject(&threads));
    }

    #[test]
    fn test_should_inject_moderate_pair() {
        // 2 threads with 3 validators each → inject
        let threads = vec![
            daemon_ipc_client::EngramResult {
                id: "a".into(), title: "T1".into(), summary: None, pass_count: 3, weighted_score: 0.5,
            },
            daemon_ipc_client::EngramResult {
                id: "b".into(), title: "T2".into(), summary: None, pass_count: 3, weighted_score: 0.4,
            },
        ];
        assert!(should_inject(&threads));
    }

    #[test]
    fn test_should_inject_weak_cluster() {
        // 4 threads with 2 validators each → inject
        let threads: Vec<_> = (0..4).map(|i| daemon_ipc_client::EngramResult {
            id: format!("{}", i), title: format!("T{}", i), summary: None, pass_count: 2, weighted_score: 0.3,
        }).collect();
        assert!(should_inject(&threads));
    }

    #[test]
    fn test_should_inject_insufficient() {
        // 1 thread with 3 validators → NOT inject
        let threads = vec![daemon_ipc_client::EngramResult {
            id: "a".into(), title: "T".into(), summary: None, pass_count: 3, weighted_score: 0.5,
        }];
        assert!(!should_inject(&threads));
    }

    #[test]
    fn test_should_inject_noise() {
        // 3 threads with 1 validator → NOT inject
        let threads: Vec<_> = (0..3).map(|i| daemon_ipc_client::EngramResult {
            id: format!("{}", i), title: format!("T{}", i), summary: None, pass_count: 1, weighted_score: 0.1,
        }).collect();
        assert!(!should_inject(&threads));
    }

    #[test]
    fn test_compute_hash_deterministic() {
        let h1 = compute_hash("hello world");
        let h2 = compute_hash("hello world");
        let h3 = compute_hash("different");
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
    }
}
