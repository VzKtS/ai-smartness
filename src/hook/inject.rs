//! Injection hook — UserPromptSubmit handler.
//!
//! Pure pass-through: extracts the user message and outputs it unchanged.

/// Run the inject hook.
/// `input` is the raw stdin already read by hook/mod.rs.
/// `session_id` is extracted from the hook JSON (per-session isolation).
pub fn run(_project_hash: &str, _agent_id: &str, input: &str, _session_id: Option<&str>) {
    let message = extract_message(input);

    if message.trim().is_empty() {
        print!("{}", input);
    } else {
        print!("{}", message);
    }
}

fn extract_message(input: &str) -> String {
    // Try parsing as JSON first
    if let Ok(data) = serde_json::from_str::<serde_json::Value>(input) {
        if let Some(msg) = data
            .get("prompt")
            .or_else(|| data.get("message"))
            .and_then(|v| v.as_str())
        {
            return msg.to_string();
        }
    }
    // Fallback: treat entire input as the message
    input.to_string()
}
