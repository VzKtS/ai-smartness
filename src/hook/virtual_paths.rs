//! Virtual Path System — intercepts Read(".ai/...") for virtual paths.
//!
//! Called from PreToolUse hook (via pretool dispatcher) when tool is Read.
//! Provides virtual filesystem endpoints for memory operations:
//!   .ai/help           → show available virtual paths
//!   .ai/recall/<query> → search memory threads
//!   .ai/threads        → list active threads
//!   .ai/status         → system status

use ai_smartness::storage::database::{open_connection, ConnectionRole};
use ai_smartness::storage::migrations;
use ai_smartness::storage::path_utils;
use ai_smartness::storage::threads::ThreadStorage;
use ai_smartness::thread::ThreadStatus;

/// Check if a Read tool call targets a virtual path.
/// Returns Some(content) if it's a virtual path, None otherwise.
pub fn check(project_hash: &str, agent_id: &str, data: &serde_json::Value) -> Option<String> {
    let tool_name = data.get("tool_name").and_then(|v| v.as_str())?;
    if tool_name != "Read" {
        return None;
    }

    let file_path = data
        .get("tool_input")
        .and_then(|i| i.get("file_path"))
        .and_then(|f| f.as_str())?;

    // Check if it's a virtual path
    let virtual_part = if file_path.starts_with(".ai/") {
        &file_path[4..]
    } else if let Some(pos) = file_path.find("/.ai/") {
        &file_path[pos + 5..]
    } else {
        return None;
    };

    tracing::info!(virtual_path = virtual_part, "Virtual path intercepted");

    match parse_virtual_path(virtual_part) {
        VirtualPath::Help => Some(help_content()),
        VirtualPath::Recall(query) => Some(recall_content(project_hash, agent_id, &query)),
        VirtualPath::Threads => Some(threads_content(project_hash, agent_id)),
        VirtualPath::Status => Some(status_content(project_hash, agent_id)),
        VirtualPath::Unknown => None,
    }
}

enum VirtualPath {
    Help,
    Recall(String),
    Threads,
    Status,
    Unknown,
}

fn parse_virtual_path(path: &str) -> VirtualPath {
    let path = path.trim_end_matches('/');
    if path == "help" || path.is_empty() {
        VirtualPath::Help
    } else if let Some(query) = path.strip_prefix("recall/") {
        VirtualPath::Recall(query.to_string())
    } else if path == "threads" {
        VirtualPath::Threads
    } else if path == "status" {
        VirtualPath::Status
    } else {
        VirtualPath::Unknown
    }
}

fn help_content() -> String {
    "\
AI Smartness Virtual Filesystem
================================
Read these virtual paths to interact with your memory system:

  .ai/help              This help text
  .ai/recall/<query>    Search memory for <query> (e.g., .ai/recall/authentication)
  .ai/threads           List all active memory threads
  .ai/status            System status (thread counts, daemon health)

Examples:
  Read(\".ai/recall/database migration\")  → Find threads about database migrations
  Read(\".ai/threads\")                    → See all active threads
  Read(\".ai/status\")                     → Check system health
"
    .to_string()
}

fn recall_content(project_hash: &str, agent_id: &str, query: &str) -> String {
    let db_path = path_utils::agent_db_path(project_hash, agent_id);
    let conn = match open_connection(&db_path, ConnectionRole::Hook) {
        Ok(c) => c,
        Err(e) => return format!("Error opening DB: {}", e),
    };
    if let Err(e) = migrations::migrate_agent_db(&conn) {
        return format!("Error migrating DB: {}", e);
    }

    match ai_smartness::intelligence::memory_retriever::MemoryRetriever::recall(&conn, query) {
        Ok(threads) => {
            if threads.is_empty() {
                return format!("No threads found matching: {}", query);
            }
            let mut out = format!("Memory recall for: \"{}\"\n\n", query);
            for (i, t) in threads.iter().enumerate().take(5) {
                out.push_str(&format!(
                    "{}. [{}] {} (weight: {:.2})\n",
                    i + 1,
                    t.id[..8.min(t.id.len())].to_string(),
                    t.title,
                    t.weight,
                ));
                if let Some(ref summary) = t.summary {
                    out.push_str(&format!("   {}\n", &summary[..summary.len().min(200)]));
                }
                if !t.topics.is_empty() {
                    out.push_str(&format!(
                        "   Topics: {}\n",
                        t.topics.iter().take(5).cloned().collect::<Vec<_>>().join(", ")
                    ));
                }
                out.push('\n');
            }
            out
        }
        Err(e) => format!("Recall error: {}", e),
    }
}

fn threads_content(project_hash: &str, agent_id: &str) -> String {
    let db_path = path_utils::agent_db_path(project_hash, agent_id);
    let conn = match open_connection(&db_path, ConnectionRole::Hook) {
        Ok(c) => c,
        Err(e) => return format!("Error opening DB: {}", e),
    };
    if let Err(e) = migrations::migrate_agent_db(&conn) {
        return format!("Error migrating DB: {}", e);
    }

    match ThreadStorage::list_active(&conn) {
        Ok(threads) => {
            if threads.is_empty() {
                return "No active threads.".to_string();
            }
            let mut out = format!("Active threads ({}):\n\n", threads.len());
            for t in threads.iter().take(20) {
                out.push_str(&format!(
                    "  [{}] {} (w:{:.2}, act:{})\n",
                    &t.id[..8.min(t.id.len())],
                    t.title,
                    t.weight,
                    t.activation_count,
                ));
            }
            if threads.len() > 20 {
                out.push_str(&format!("  ... and {} more\n", threads.len() - 20));
            }
            out
        }
        Err(e) => format!("Error listing threads: {}", e),
    }
}

fn status_content(project_hash: &str, agent_id: &str) -> String {
    let db_path = path_utils::agent_db_path(project_hash, agent_id);
    let conn = match open_connection(&db_path, ConnectionRole::Hook) {
        Ok(c) => c,
        Err(e) => return format!("Error opening DB: {}", e),
    };
    if let Err(e) = migrations::migrate_agent_db(&conn) {
        return format!("Error migrating DB: {}", e);
    }

    let active = ThreadStorage::count_by_status(&conn, &ThreadStatus::Active).unwrap_or(0);
    let suspended = ThreadStorage::count_by_status(&conn, &ThreadStatus::Suspended).unwrap_or(0);
    let archived = ThreadStorage::count_by_status(&conn, &ThreadStatus::Archived).unwrap_or(0);

    let beat = ai_smartness::storage::beat::BeatState::load(
        &path_utils::agent_data_dir(project_hash, agent_id),
    );

    format!(
        "\
AI Smartness Status
====================
Agent: {}
Project: {}

Threads:
  Active:    {}
  Suspended: {}
  Archived:  {}
  Total:     {}

Beat System:
  Current beat:     {}
  Since last:       {} beats
  Last interaction: {}
",
        agent_id,
        &project_hash[..8.min(project_hash.len())],
        active,
        suspended,
        archived,
        active + suspended + archived,
        beat.beat,
        beat.since_last(),
        beat.last_interaction_at,
    )
}
