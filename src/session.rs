//! Session state — tracks current session context for continuity.
//!
//! Persisted as `{agent_data_dir}/session_state.json`.
//! Updated by inject hook (on each prompt) and capture hook (on each tool use).

use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub agent_id: String,
    pub project_hash: String,
    pub started_at: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
    pub prompt_count: u32,
    /// Current work context
    #[serde(default)]
    pub current_work: CurrentWork,
    /// Recently modified files (last 20)
    #[serde(default)]
    pub files_modified: Vec<FileModification>,
    /// Pending tasks (max 10)
    #[serde(default)]
    pub pending_tasks: Vec<String>,
    /// Recent tool calls (last 50)
    #[serde(default)]
    pub tool_history: Vec<ToolCall>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CurrentWork {
    pub thread_id: Option<String>,
    pub thread_title: Option<String>,
    /// Last user message (truncated 200 chars)
    pub last_user_message: Option<String>,
    /// Last agent action (truncated 100 chars)
    pub last_agent_action: Option<String>,
    /// Current intent (truncated 200 chars)
    pub intent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileModification {
    pub path: String,
    /// "Read", "Edit", "Write"
    pub action: String,
    pub timestamp: String,
    /// Truncated 100 chars
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub tool: String,
    pub target: String,
    /// HH:MM:SS
    pub at: String,
}

const SESSION_FILE: &str = "session_state.json";
const MAX_FILES_MODIFIED: usize = 20;
const MAX_TOOL_HISTORY: usize = 50;

impl SessionState {
    /// Create a new session state for a fresh session.
    pub fn new(agent_id: &str, project_hash: &str) -> Self {
        let now = Utc::now();
        Self {
            agent_id: agent_id.to_string(),
            project_hash: project_hash.to_string(),
            started_at: now,
            last_activity: now,
            prompt_count: 0,
            current_work: CurrentWork::default(),
            files_modified: Vec::new(),
            pending_tasks: Vec::new(),
            tool_history: Vec::new(),
        }
    }

    /// Load session state from file, or create new if absent/corrupted.
    pub fn load(agent_data_dir: &Path, agent_id: &str, project_hash: &str) -> Self {
        let path = agent_data_dir.join(SESSION_FILE);
        if !path.exists() {
            return Self::new(agent_id, project_hash);
        }
        match std::fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content)
                .unwrap_or_else(|_| Self::new(agent_id, project_hash)),
            Err(_) => Self::new(agent_id, project_hash),
        }
    }

    /// Save session state to file.
    pub fn save(&self, agent_data_dir: &Path) {
        if let Err(e) = std::fs::create_dir_all(agent_data_dir) {
            tracing::warn!(error = %e, "Failed to create agent data dir for session");
            return;
        }
        let path = agent_data_dir.join(SESSION_FILE);
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&path, json) {
                    tracing::warn!(error = %e, "Failed to write session_state.json");
                }
            }
            Err(e) => tracing::warn!(error = %e, "Failed to serialize session state"),
        }
    }

    /// Record a prompt (inject hook calls this).
    pub fn record_prompt(&mut self, message: &str) {
        self.last_activity = Utc::now();
        self.prompt_count += 1;
        self.current_work.last_user_message =
            Some(message[..message.len().min(200)].to_string());
    }

    /// Record a tool call (capture hook calls this).
    pub fn record_tool_call(&mut self, tool_name: &str, target: &str) {
        self.last_activity = Utc::now();
        self.tool_history.push(ToolCall {
            tool: tool_name.to_string(),
            target: target[..target.len().min(100)].to_string(),
            at: Utc::now().format("%H:%M:%S").to_string(),
        });
        // Keep last N
        if self.tool_history.len() > MAX_TOOL_HISTORY {
            self.tool_history.remove(0);
        }
        // Track last agent action
        self.current_work.last_agent_action =
            Some(format!("{}: {}", tool_name, &target[..target.len().min(80)]));
    }

    /// Record a file modification (capture hook calls this for Edit/Write/Read).
    pub fn record_file_modification(&mut self, path: &str, action: &str, summary: &str) {
        self.files_modified.push(FileModification {
            path: path.to_string(),
            action: action.to_string(),
            timestamp: Utc::now().to_rfc3339(),
            summary: summary[..summary.len().min(100)].to_string(),
        });
        // Keep last N
        if self.files_modified.len() > MAX_FILES_MODIFIED {
            self.files_modified.remove(0);
        }
    }

    /// Duration since session start in minutes.
    pub fn duration_minutes(&self) -> i64 {
        (Utc::now() - self.started_at).num_minutes()
    }
}
