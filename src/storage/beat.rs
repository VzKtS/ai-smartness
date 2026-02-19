//! Beat system — abstract temporal perception for AI agents.
//!
//! The daemon increments a "beat" counter every prune cycle (default 5 min).
//! Agents perceive time through beats rather than clock time.
//! This enables self-wake for autonomous task chaining.
//!
//! Storage file: `{agent_data_dir}/beat.json`

use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledWake {
    pub target_beat: u64,
    pub reason: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeatState {
    pub beat: u64,
    pub started_at: String,
    pub last_beat_at: String,
    pub last_interaction_at: String,
    pub last_interaction_beat: u64,
    pub last_session_id: Option<String>,
    pub last_thread_id: Option<String>,
    /// PID of the MCP server process owning this agent.
    #[serde(default)]
    pub pid: Option<u32>,
    /// PID of the parent Claude CLI process (for extension injection targeting).
    #[serde(default)]
    pub cli_pid: Option<u32>,
    /// Scheduled self-wakes (beat_wake tool).
    #[serde(default)]
    pub scheduled_wakes: Vec<ScheduledWake>,
    /// Approximate context window tokens used (estimated from injection size).
    #[serde(default)]
    pub context_tokens: Option<u64>,
    /// Context window usage percentage (0.0 - 100.0).
    #[serde(default)]
    pub context_percent: Option<f64>,
    /// When context tracking was last updated.
    #[serde(default)]
    pub context_updated_at: Option<String>,
}

impl Default for BeatState {
    fn default() -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            beat: 0,
            started_at: now.clone(),
            last_beat_at: now.clone(),
            last_interaction_at: now,
            last_interaction_beat: 0,
            last_session_id: None,
            last_thread_id: None,
            pid: None,
            cli_pid: None,
            scheduled_wakes: Vec::new(),
            context_tokens: None,
            context_percent: None,
            context_updated_at: None,
        }
    }
}

const BEAT_FILE: &str = "beat.json";

impl BeatState {
    /// Load beat state from file, or create default if absent/corrupted.
    pub fn load(agent_data_dir: &Path) -> Self {
        let path = agent_data_dir.join(BEAT_FILE);
        if !path.exists() {
            return Self::default();
        }
        match std::fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Save beat state to file. Creates parent dir if needed.
    pub fn save(&self, agent_data_dir: &Path) {
        if let Err(e) = std::fs::create_dir_all(agent_data_dir) {
            tracing::warn!(error = %e, "Failed to create agent data dir for beat");
            return;
        }
        let path = agent_data_dir.join(BEAT_FILE);
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&path, json) {
                    tracing::warn!(error = %e, "Failed to write beat.json");
                }
            }
            Err(e) => tracing::warn!(error = %e, "Failed to serialize beat state"),
        }
    }

    /// Increment beat counter. Called by daemon each prune cycle.
    pub fn increment(&mut self) {
        self.beat += 1;
        self.last_beat_at = Utc::now().to_rfc3339();
    }

    /// Record that an agent interaction occurred at current beat.
    pub fn record_interaction(
        &mut self,
        session_id: Option<&str>,
        thread_id: Option<&str>,
    ) {
        self.last_interaction_at = Utc::now().to_rfc3339();
        self.last_interaction_beat = self.beat;
        if let Some(sid) = session_id {
            self.last_session_id = Some(sid.to_string());
        }
        if let Some(tid) = thread_id {
            self.last_thread_id = Some(tid.to_string());
        }
    }

    /// Number of beats since last interaction.
    pub fn since_last(&self) -> u64 {
        self.beat.saturating_sub(self.last_interaction_beat)
    }

    /// Check if this is a new session (different session_id).
    pub fn is_new_session(&self, session_id: &str) -> bool {
        match &self.last_session_id {
            Some(last) => last != session_id,
            None => true,
        }
    }

    /// Get real time since last interaction, if parseable.
    pub fn time_since_last(&self) -> Option<chrono::Duration> {
        let last: DateTime<Utc> = self.last_interaction_at.parse().ok()?;
        Some(Utc::now() - last)
    }

    /// Schedule a self-wake after N beats.
    pub fn schedule_wake(&mut self, after_beats: u64, reason: String) {
        self.scheduled_wakes.push(ScheduledWake {
            target_beat: self.beat + after_beats,
            reason,
            created_at: Utc::now().to_rfc3339(),
        });
    }

    /// Drain all scheduled wakes that are due (target_beat <= current beat).
    pub fn drain_due_wakes(&mut self) -> Vec<ScheduledWake> {
        let current = self.beat;
        let (due, remaining): (Vec<_>, Vec<_>) = self.scheduled_wakes
            .drain(..)
            .partition(|w| w.target_beat <= current);
        self.scheduled_wakes = remaining;
        due
    }
}
