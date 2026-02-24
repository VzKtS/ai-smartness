//! Compact hook — generates context compaction synthesis.
//!
//! Triggered when context window approaches capacity (via F16 context tracking).
//! Saves a synthesis report to `{agent_data_dir}/synthesis/` for future injection.

use std::path::Path;

use ai_smartness::storage::path_utils;
use ai_smartness::storage::threads::ThreadStorage;
use rusqlite::Connection;
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
/// Called when context compaction is detected.
pub fn generate_synthesis(conn: &Connection, project_hash: &str, agent_id: &str) -> Option<String> {
    let threads = ThreadStorage::list_active(conn).ok()?;
    if threads.is_empty() {
        return None;
    }

    // Sort by importance (most critical threads first)
    let mut sorted = threads;
    sorted.sort_by(|a, b| b.importance.partial_cmp(&a.importance).unwrap_or(std::cmp::Ordering::Equal));

    // Build synthesis from active threads
    let active_work: Vec<WorkItem> = sorted
        .iter()
        .take(10)
        .map(|t| WorkItem {
            thread_id: t.id.clone(),
            title: t.title.clone(),
            summary: t.summary.clone(),
        })
        .collect();

    // Key insights: threads with high importance
    let mut key_insights: Vec<String> = sorted.iter()
        .filter(|t| t.importance >= 0.7)
        .take(5)
        .map(|t| {
            let summary_preview = t.summary.as_deref()
                .unwrap_or("")
                .chars().take(100).collect::<String>();
            format!("[importance={:.1}] {}: {}", t.importance, t.title, summary_preview)
        })
        .collect();

    // Include active pins as insights
    let agent_data = path_utils::agent_data_dir(project_hash, agent_id);
    let pins_path = agent_data.join("pins.json");
    if let Ok(pins_content) = std::fs::read_to_string(&pins_path) {
        if let Ok(pins) = serde_json::from_str::<serde_json::Value>(&pins_content) {
            if let Some(pin_array) = pins.get("pins").and_then(|p| p.as_array()) {
                for pin in pin_array.iter().take(3) {
                    if let Some(content) = pin.get("content").and_then(|c| c.as_str()) {
                        let preview: String = content.chars().take(100).collect();
                        key_insights.push(format!("[pin] {}", preview));
                    }
                }
            }
        }
    }

    // Open questions: threads tagged __focus__ (active investigation topics)
    let open_questions: Vec<String> = sorted.iter()
        .filter(|t| t.tags.iter().any(|tag| tag.contains("__focus__")))
        .take(5)
        .map(|t| {
            let summary_preview = t.summary.as_deref()
                .unwrap_or("(no summary)")
                .chars().take(100).collect::<String>();
            format!("{}: {}", t.title, summary_preview)
        })
        .collect();

    let synthesis = SynthesisReport {
        timestamp: chrono::Utc::now().to_rfc3339(),
        agent_id: agent_id.to_string(),
        active_work,
        key_insights,
        open_questions,
    };

    // Save to synthesis dir (reuse agent_data from pins read above)
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
                out.push_str(&format!(": {}", ai_smartness::constants::truncate_safe(s, 100)));
            }
            out.push('\n');
        }
    }

    if !synthesis.key_insights.is_empty() {
        out.push_str("\nKey insights:\n");
        for insight in &synthesis.key_insights {
            out.push_str(&format!("- {}\n", insight));
        }
    }

    if !synthesis.open_questions.is_empty() {
        out.push_str("\nOpen investigations:\n");
        for q in &synthesis.open_questions {
            out.push_str(&format!("- {}\n", q));
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
