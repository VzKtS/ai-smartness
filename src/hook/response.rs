//! Response hook — Stop handler.
//!
//! Captures Claude's response text when the agent finishes responding.
//! Uses the `Stop` hook event which provides `last_assistant_message`.
//!
//! IMPORTANT: This hook never blocks — it only captures and lets Claude stop normally.

use ai_smartness::constants::MIN_RESPONSE_LENGTH;
use ai_smartness::processing::daemon_ipc_client;
use ai_smartness::storage::path_utils;

/// Run the response capture hook.
/// `input` is the raw stdin already read by hook/mod.rs.
pub fn run(project_hash: &str, agent_id: &str, input: &str) {
    tracing::info!(project = project_hash, agent = agent_id, "response::run() called");

    // Check config toggle
    if !is_response_capture_enabled() {
        tracing::info!("Response capture disabled in config, skipping");
        return;
    }

    if input.is_empty() {
        tracing::info!("Response: stdin was EMPTY, skipping");
        return;
    }

    let data: serde_json::Value = match serde_json::from_str(input) {
        Ok(v) => v,
        Err(e) => {
            tracing::info!(error = %e, "Response: invalid JSON, skipping");
            return;
        }
    };

    // Extract the response text
    let response_text = match data.get("last_assistant_message").and_then(|v| v.as_str()) {
        Some(text) if !text.is_empty() => text,
        _ => {
            tracing::info!("Response: no last_assistant_message, skipping");
            return;
        }
    };

    // Filter short responses (noise protection)
    if response_text.len() < MIN_RESPONSE_LENGTH {
        tracing::info!(
            len = response_text.len(),
            min = MIN_RESPONSE_LENGTH,
            "Response: too short, skipping"
        );
        return;
    }

    // Send to daemon via IPC (source_type = "Response")
    tracing::info!(
        content_len = response_text.len(),
        "Response: sending to daemon"
    );
    let _ = daemon_ipc_client::send_capture(project_hash, agent_id, "Response", response_text);
}

/// Check if agent response capture is enabled in global config.
fn is_response_capture_enabled() -> bool {
    let config_path = path_utils::data_dir().join("config.json");
    if let Ok(content) = std::fs::read_to_string(&config_path) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(tools) = v.get("capture").and_then(|c| c.get("tools")) {
                return tools.get("agent_response").and_then(|v| v.as_bool()).unwrap_or(true);
            }
        }
    }
    true // default: enabled
}
