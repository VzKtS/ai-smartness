//! PreToolUse dispatcher — routes to virtual_paths + engram injection.
//!
//! Single hook entry point for PreToolUse. Reads stdin (JSON),
//! dispatches to appropriate handler based on tool_name.
//!
//! Engram-on-Thinking (T14): Extracts the latest thinking block from the
//! JSONL transcript, queries the engram retriever via daemon IPC, and
//! injects matching threads as `additionalContext` in the PreToolUse
//! hookSpecificOutput (visible to Claude, unlike PostToolUse stdout).

use ai_smartness::processing::daemon_ipc_client;
use ai_smartness::storage::path_utils;
use ai_smartness::storage::transcript;

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Run the pretool hook.
/// `input` is the raw stdin already read by hook/mod.rs.
pub fn run(project_hash: &str, agent_id: &str, input: &str, session_id: Option<&str>) {
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
            // Not a virtual path → fall through to engram + passthrough
        }
        _ => {}
    }

    // Skip engram injection for ai-smartness MCP tools (prevent cycle)
    if tool_name.starts_with("mcp__ai-smartness__") {
        print!("{}", input);
        return;
    }

    // Engram-on-Thinking injection (T14)
    if let Some(hint) = try_engram_injection(session_id, project_hash, agent_id) {
        let response = serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "additionalContext": hint,
            }
        });
        print!("{}", serde_json::to_string(&response).unwrap_or_default());
        return;
    }

    // Default: passthrough unchanged
    print!("{}", input);
}

/// Try to build an engram hint based on the agent's latest thinking block.
///
/// Reads the JSONL transcript, extracts thinking, queries the engram retriever
/// via daemon IPC, and returns the formatted `<engram>` hint if matches found.
/// Deduplicates by thinking hash to avoid querying the same thinking block
/// on multiple tool calls within the same turn.
///
/// Returns `Some(hint_string)` if injection should happen, `None` otherwise.
/// This function NEVER fails — all errors are silently logged.
fn try_engram_injection(session_id: Option<&str>, project_hash: &str, agent_id: &str) -> Option<String> {
    let sid = match session_id {
        Some(s) if !s.is_empty() => s,
        _ => return None,
    };

    // Check if engram injection is enabled in config
    if !is_engram_injection_enabled(project_hash) {
        return None;
    }

    // Extract thinking block from JSONL transcript
    let thinking = match transcript::extract_last_thinking(sid) {
        Some(t) if t.len() > 50 => t, // Skip very short thinking blocks
        _ => return None,
    };

    // Deduplicate: hash check against last processed thinking
    let hash = compute_hash(&thinking);
    let state_path = path_utils::agent_data_dir(project_hash, agent_id)
        .join("engram_thinking.hash");

    if let Ok(stored_hash) = std::fs::read_to_string(&state_path) {
        if stored_hash.trim() == hash.to_string() {
            tracing::debug!("Engram: same thinking hash, skipping");
            return None;
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
            return None;
        }
    };

    if results.is_empty() {
        tracing::debug!("Engram: no matches");
        return None;
    }

    // Apply convergence threshold
    if !should_inject(&results) {
        tracing::debug!(
            count = results.len(),
            max_pass = results.iter().map(|r| r.pass_count).max().unwrap_or(0),
            "Engram: below convergence threshold"
        );
        return None;
    }

    // Format engram hint — degressive summary length: most relevant thread gets more context
    let max_lengths = [250, 150, 80];
    let mut hint = String::from("<engram>\n");
    for (i, r) in results.iter().filter(|r| r.pass_count >= 2).take(3).enumerate() {
        let summary = r.summary.as_deref().unwrap_or("");
        let max_len = max_lengths[i];
        let summary_short = if summary.len() > max_len {
            let byte_end: usize = summary.chars().take(max_len - 3).map(|c| c.len_utf8()).sum();
            format!("{}...", &summary[..byte_end])
        } else {
            summary.to_string()
        };
        hint.push_str(&format!("- \"{}\": {}\n", r.title, summary_short));
    }
    // Collect top thread ID for the directive
    let top_id = results
        .iter()
        .filter(|r| r.pass_count >= 2)
        .next()
        .map(|r| r.id.as_str())
        .unwrap_or("");
    hint.push_str(&format!(
        "BIFURCATE: save current reasoning as __mind__ thread, then explore the threads above \
         (use ai_recall or read thread {}) before continuing. Do NOT resume your original \
         thought path without integrating this context.\n",
        top_id
    ));
    hint.push_str("</engram>");

    tracing::info!(
        matches = results.len(),
        injected = results.iter().filter(|r| r.pass_count >= 2).take(3).count(),
        "Engram hint injected via PreToolUse additionalContext"
    );

    Some(hint)
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
