//! Compact hook — generates context compaction synthesis.
//!
//! Triggered when context window approaches capacity (via F16 context tracking).
//! Saves a synthesis report to `{agent_data_dir}/synthesis/` for future injection.

use std::path::Path;

use ai_smartness::storage::database::{open_connection, ConnectionRole};
use ai_smartness::storage::migrations;
use ai_smartness::storage::path_utils;
use ai_smartness::storage::threads::ThreadStorage;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct SynthesisReport {
    pub timestamp: String,
    pub agent_id: String,
    pub active_work: Vec<WorkItem>,
    pub key_insights: Vec<String>,
    pub open_questions: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkItem {
    pub thread_id: String,
    pub title: String,
    pub summary: Option<String>,
}

/// Generate a synthesis of current work context.
/// Called when context window is near capacity.
pub fn generate_synthesis(project_hash: &str, agent_id: &str) -> Option<String> {
    let db_path = path_utils::agent_db_path(project_hash, agent_id);
    if !db_path.exists() {
        return None;
    }

    let conn = open_connection(&db_path, ConnectionRole::Hook).ok()?;
    migrations::migrate_agent_db(&conn).ok()?;

    let threads = ThreadStorage::list_active(&conn).ok()?;
    if threads.is_empty() {
        return None;
    }

    // Build synthesis from active threads
    let active_work: Vec<WorkItem> = threads
        .iter()
        .take(10)
        .map(|t| WorkItem {
            thread_id: t.id.clone(),
            title: t.title.clone(),
            summary: t.summary.clone(),
        })
        .collect();

    let synthesis = SynthesisReport {
        timestamp: chrono::Utc::now().to_rfc3339(),
        agent_id: agent_id.to_string(),
        active_work,
        key_insights: Vec::new(),
        open_questions: Vec::new(),
    };

    // Save to synthesis dir
    let agent_data = path_utils::agent_data_dir(project_hash, agent_id);
    save_synthesis(&agent_data, &synthesis);

    // Return formatted for injection
    Some(format_for_injection(&synthesis))
}

fn save_synthesis(agent_data_dir: &Path, synthesis: &SynthesisReport) {
    let synthesis_dir = agent_data_dir.join("synthesis");
    if std::fs::create_dir_all(&synthesis_dir).is_err() {
        return;
    }
    let filename = format!(
        "synthesis_{}.json",
        chrono::Utc::now().format("%Y%m%d_%H%M%S")
    );
    if let Ok(json) = serde_json::to_string_pretty(synthesis) {
        std::fs::write(synthesis_dir.join(&filename), json).ok();
        tracing::info!(file = %filename, "Synthesis saved");
    }
}

fn format_for_injection(synthesis: &SynthesisReport) -> String {
    let mut out = String::from("Context synthesis (pre-compaction snapshot):\n");

    if !synthesis.active_work.is_empty() {
        out.push_str("Active work:\n");
        for item in &synthesis.active_work {
            out.push_str(&format!("- {}", item.title));
            if let Some(ref s) = item.summary {
                out.push_str(&format!(": {}", &s[..s.len().min(100)]));
            }
            out.push('\n');
        }
    }

    out
}

/// Load the most recent synthesis for injection (if available and fresh).
pub fn load_latest_synthesis(project_hash: &str, agent_id: &str) -> Option<String> {
    let agent_data = path_utils::agent_data_dir(project_hash, agent_id);
    let synthesis_dir = agent_data.join("synthesis");
    if !synthesis_dir.exists() {
        return None;
    }

    // Find most recent synthesis file
    let mut files: Vec<_> = std::fs::read_dir(&synthesis_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_str()
                .map(|n| n.starts_with("synthesis_") && n.ends_with(".json"))
                .unwrap_or(false)
        })
        .collect();

    files.sort_by_key(|e| e.file_name());
    let latest = files.last()?;

    let content = std::fs::read_to_string(latest.path()).ok()?;
    let synthesis: SynthesisReport = serde_json::from_str(&content).ok()?;

    // Only use if less than 1 hour old
    if let Ok(ts) = chrono::DateTime::parse_from_rfc3339(&synthesis.timestamp) {
        let ts_utc: chrono::DateTime<chrono::Utc> = ts.into();
        if chrono::Utc::now() - ts_utc > chrono::Duration::hours(1) {
            return None;
        }
    }

    Some(format_for_injection(&synthesis))
}
