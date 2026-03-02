//! Reminder assembler — builds the `<ai-smartness>` core context block.
//!
//! Lightweight: reads beat.json + user_profile.json + 3 DB queries.
//! No EngramRetriever, no ONNX. ~15ms overhead.
//!
//! All sections are **core** — inamovible, non-removable by agents.
//! Conditional sections are omitted only when their data is empty.

use ai_smartness::registry::registry::AgentRegistry;
use ai_smartness::storage::cognitive_inbox::CognitiveInbox;
use ai_smartness::storage::mcp_messages::McpMessages;
use ai_smartness::storage::threads::ThreadStorage;
use ai_smartness::storage::{database, path_utils};
use ai_smartness::storage::beat::BeatState;
use ai_smartness::thread::ThreadStatus;
use ai_smartness::user_profile::UserProfile;

/// Build the full `<ai-smartness>` reminder block.
///
/// Returns the block with trailing `\n\n`, or empty string on fatal error.
pub fn build(
    project_hash: &str,
    agent_id: &str,
    session_id: Option<&str>,
    beat: &BeatState,
) -> String {
    match build_inner(project_hash, agent_id, session_id, beat) {
        Some(block) => format!("<ai-smartness>\n{}</ai-smartness>\n\n", block),
        None => String::new(),
    }
}

fn build_inner(
    project_hash: &str,
    agent_id: &str,
    session_id: Option<&str>,
    beat: &BeatState,
) -> Option<String> {
    let agent_data = path_utils::agent_data_dir(project_hash, agent_id);
    let profile = UserProfile::load(&agent_data);

    let mut lines = Vec::new();

    // ── Line 1: Runtime rule (INAMOVIBLE) ──
    lines.push("Be proactive on your memory. Don't wait to be requested about it! -> ai_help to show all MCP tools. User's language Be proactive on your memory. Don't wait to be requested about it! -> ai_help to show all MCP tools. Always do LITERALLY what the user asks. When unsure or when reasoning isn't short and clear,ASK — and keep asking until fully certain. Never assume, never paraphrase when verbatim is requested, never decide on behalf of the user.".to_string());
    // ── Line 2: Header ──
    let version = env!("CARGO_PKG_VERSION");
    let sid = session_id.unwrap_or("unknown");
    let ctx_part = match (beat.context_tokens, beat.context_percent) {
        (Some(t), Some(p)) => format!(" | ctx:{} ({:.0}%)", t, p),
        _ => String::new(),
    };
    lines.push(format!("v{} | sid:{}{}", version, sid, ctx_part));

    // ── Line 3: Agent identity ──
    let role = get_agent_role(project_hash, agent_id);
    lines.push(format!(
        "agent: {} ({}) | ai_agent_select(agent_id=\"{}\", session_id=\"{}\")",
        agent_id, role, agent_id, sid
    ));

    // ── Rules (conditional + golden rule) ──
    lines.push(String::new());
    lines.push("rules:".to_string());
    lines.push("- NEVER use bash/sqlite to access or modify ai-smartness databases directly. All memory operations MUST go through MCP tools.".to_string());
    for rule in &profile.context_rules {
        lines.push(format!("- {}", rule));
    }

    // ── Threads, pins, focus (from agent DB) ──
    if let Ok(agent_conn) = database::open_connection(
        &path_utils::agent_db_path(project_hash, agent_id),
        database::ConnectionRole::Hook,
    ) {
        append_threads_pins_focus(&mut lines, &agent_conn);
        append_pending_messages(&mut lines, &agent_conn, project_hash, agent_id, beat);
    }

    // ── Profile ──
    let name_part = profile
        .identity
        .name
        .as_deref()
        .unwrap_or_default();
    if !name_part.is_empty() {
        lines.push(format!(
            "\nprofile: {} | lang:{}",
            name_part, profile.preferences.language
        ));
    } else {
        lines.push(format!("\nprofile: lang:{}", profile.preferences.language));
    }

    Some(lines.join("\n") + "\n")
}

/// Append threads (top 3), pins, and focus sections from a single list_all pass.
fn append_threads_pins_focus(lines: &mut Vec<String>, conn: &rusqlite::Connection) {
    let all = match ThreadStorage::list_all(conn) {
        Ok(t) => t,
        Err(_) => return,
    };

    // Threads: top 3 active, excluding __pin__ and __focus__
    let threads: Vec<_> = all
        .iter()
        .filter(|t| {
            t.status == ThreadStatus::Active
                && !t.tags.contains(&"__pin__".to_string())
                && !t.tags.contains(&"__focus__".to_string())
        })
        .take(3)
        .collect();

    if !threads.is_empty() {
        lines.push(String::new());
        lines.push("threads (ai_recall for deep search):".to_string());
        for t in &threads {
            let id8 = if t.id.len() > 8 { &t.id[..8] } else { &t.id };
            lines.push(format!(
                "- {} \"{}\" w={:.2} i={:.2}",
                id8, t.title, t.weight, t.importance
            ));
        }
    }

    // Pins: __pin__ tagged, active
    let pins: Vec<_> = all
        .iter()
        .filter(|t| t.tags.contains(&"__pin__".to_string()) && t.status == ThreadStatus::Active)
        .take(10)
        .collect();

    if !pins.is_empty() {
        lines.push(String::new());
        lines.push("pins:".to_string());
        for t in &pins {
            let id8 = if t.id.len() > 8 { &t.id[..8] } else { &t.id };
            lines.push(format!("- {} \"{}\" w={:.2}", id8, t.title, t.weight));
        }
    }

    // Focus: __focus__ tagged, active
    let focus: Vec<_> = all
        .iter()
        .filter(|t| t.tags.contains(&"__focus__".to_string()) && t.status == ThreadStatus::Active)
        .collect();

    if !focus.is_empty() {
        lines.push(String::new());
        lines.push("focus:".to_string());
        for t in &focus {
            let topic = t.topics.first().map(|s| s.as_str()).unwrap_or(&t.title);
            lines.push(format!("- {} w={:.2}", topic, t.weight));
        }
    }
}

/// Append alerts line (tasks + messages pending). Omitted if both are 0.
fn append_pending_messages(
    lines: &mut Vec<String>,
    agent_conn: &rusqlite::Connection,
    project_hash: &str,
    agent_id: &str,
    beat: &BeatState,
) {
    let tasks = beat.pending_tasks.len();

    // Messages: cognitive inbox (agent DB) + mcp messages (shared DB)
    let mut msgs = CognitiveInbox::count_pending(agent_conn, agent_id).unwrap_or(0);
    if let Ok(shared_conn) = database::open_connection(
        &path_utils::shared_db_path(project_hash),
        database::ConnectionRole::Hook,
    ) {
        msgs += McpMessages::count_pending(&shared_conn, agent_id).unwrap_or(0);
    }

    if tasks > 0 || msgs > 0 {
        let mut parts = Vec::new();
        if tasks > 0 {
            parts.push(format!("{} tasks pending", tasks));
        }
        if msgs > 0 {
            parts.push(format!("{} messages pending", msgs));
        }
        lines.push(format!("\nalerts: {}", parts.join(" | ")));
    }
}

/// Lookup agent role from registry. Returns "agent" as fallback.
fn get_agent_role(project_hash: &str, agent_id: &str) -> String {
    let reg_conn = match database::open_connection(
        &path_utils::registry_db_path(),
        database::ConnectionRole::Hook,
    ) {
        Ok(c) => c,
        Err(_) => return "agent".to_string(),
    };

    match AgentRegistry::get(&reg_conn, agent_id, project_hash) {
        Ok(Some(agent)) => {
            if let Some(ref custom) = agent.custom_role {
                if !custom.is_empty() {
                    return custom.clone();
                }
            }
            agent.role.clone()
        }
        _ => "agent".to_string(),
    }
}
