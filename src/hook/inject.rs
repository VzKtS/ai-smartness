//! Injection hook — UserPromptSubmit handler.
//!
//! Beat tracking + reminder assembly: records session_id, prompt_count,
//! context tokens from transcript JSONL, then prepends the `<ai-smartness>`
//! core reminder block to the user message.

use ai_smartness::processing::daemon_ipc_client;
use ai_smartness::storage::{beat::BeatState, path_utils, transcript};

use super::reminder;

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

    // Always strip IDE/system-injected tags to prevent feedback loops
    // (capturing our own engram reminders) and reduce noise.
    let clean_message = strip_system_tags(&message);

    // Send prompt to daemon for capture (non-blocking)
    tracing::info!(
        clean_len = clean_message.len(),
        clean_preview = &clean_message[..clean_message.len().min(100)],
        "Inject: sending prompt to daemon"
    );
    match daemon_ipc_client::send_prompt_capture(project_hash, agent_id, &clean_message, session_id) {
        Ok(_) => tracing::info!("Inject: prompt sent to daemon OK"),
        Err(e) => tracing::warn!(error = %e, "Inject: prompt send FAILED"),
    }

    // Build and prepend reminder block
    let reminder_block = reminder::build(project_hash, agent_id, session_id, &beat);
    print!("{}", reminder_block);

    // Output the user message
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

/// Strip IDE/system-injected XML tags from prompt text.
///
/// VSCode extension injects `<ide_selection>...</ide_selection>` into prompts,
/// and context compaction may leave `<system-reminder>...</system-reminder>` blocks.
/// These pollute captured content and must be removed before sending to memory.
fn strip_system_tags(text: &str) -> String {
    const TAGS: &[&str] = &["ide_selection", "system-reminder"];
    let mut result = text.to_string();
    for tag in TAGS {
        loop {
            let open = format!("<{}", tag);
            let close = format!("</{}>", tag);
            let start = match result.find(&open) {
                Some(i) => i,
                None => break,
            };
            let end = match result[start..].find(&close) {
                Some(i) => start + i + close.len(),
                None => break, // unclosed tag — leave as-is
            };
            result.replace_range(start..end, "");
        }
    }
    // Collapse multiple blank lines left by removed blocks
    while result.contains("\n\n\n") {
        result = result.replace("\n\n\n", "\n\n");
    }
    result.trim().to_string()
}


