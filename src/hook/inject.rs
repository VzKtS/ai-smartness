//! Injection hook — UserPromptSubmit handler.
//!
//! Pass-through with beat tracking: records session_id, prompt_count,
//! and context tokens from Claude Code transcript JSONL.

use ai_smartness::storage::{beat::BeatState, path_utils, transcript};

/// Run the inject hook.
/// `input` is the raw stdin already read by hook/mod.rs.
/// `session_id` is extracted from the hook JSON (per-session isolation).
pub fn run(project_hash: &str, agent_id: &str, input: &str, session_id: Option<&str>) {
    let message = extract_message(input);

    // Record interaction in BeatState (session_id, prompt_count, context_tokens)
    let agent_data = path_utils::agent_data_dir(project_hash, agent_id);
    let mut beat = BeatState::load(&agent_data);
    beat.record_interaction(session_id, None);

    if let Some(sid) = session_id {
        update_context_from_transcript(&mut beat, sid);
    }

    beat.save(&agent_data);

    // Pass-through: output the user message unchanged
    if message.trim().is_empty() {
        print!("{}", input);
    } else {
        print!("{}", message);
    }
}

/// Update context tokens from Claude Code transcript JSONL.
fn update_context_from_transcript(beat: &mut BeatState, session_id: &str) {
    let transcript_path = match transcript::find_transcript(session_id) {
        Some(p) => p,
        None => return,
    };

    let info = match transcript::read_last_usage(&transcript_path) {
        Some(i) => i,
        None => return,
    };

    if !beat.should_update_context(info.percent) {
        return;
    }

    beat.update_context(info.total_tokens, info.percent, "transcript", info.model);
}

fn extract_message(input: &str) -> String {
    if let Ok(data) = serde_json::from_str::<serde_json::Value>(input) {
        if let Some(msg) = data
            .get("prompt")
            .or_else(|| data.get("message"))
            .and_then(|v| v.as_str())
        {
            return msg.to_string();
        }
    }
    input.to_string()
}
