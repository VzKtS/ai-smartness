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
    /// Source of context data: "transcript" (99%) or "tool_io" (30-50% fallback).
    #[serde(default)]
    pub context_source: Option<String>,
    /// Current model detected from transcript (e.g. "claude-sonnet-4-20250514").
    #[serde(default)]
    pub model: Option<String>,
    /// True when a context compaction is suspected (tokens dropped >40%).
    #[serde(default)]
    pub compaction_suspected: bool,
    /// Cognitive nudge tracking
    #[serde(default)]
    pub last_nudge_type: String,
    #[serde(default)]
    pub last_nudge_beat: u64,
    #[serde(default)]
    pub last_maintenance_beat: u64,
    #[serde(default)]
    pub last_recall_beat: u64,
    /// Thread quota (synced from agent's ThreadMode by daemon).
    #[serde(default = "default_quota")]
    pub quota: usize,
    // ── E3: Response latency tracking ──
    #[serde(default)]
    pub last_prompt_at: Option<String>,
    #[serde(default)]
    pub response_latency_ms: Option<u64>,
    // ── E4: Uptime — PID that owns started_at ──
    #[serde(default)]
    pub started_pid: Option<u32>,
    // ── E5: Error tracking ──
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub last_error_at: Option<String>,
    // ── E6: Interaction counters ──
    #[serde(default)]
    pub tool_call_count: u64,
    #[serde(default)]
    pub prompt_count: u64,
    // ── MAX quota tracking ──
    #[serde(default)]
    pub plan_type: Option<String>,
    #[serde(default)]
    pub plan_tier: Option<String>,
    #[serde(default)]
    pub quota_5h: Option<f64>,
    #[serde(default)]
    pub quota_7d: Option<f64>,
    #[serde(default)]
    pub quota_status_5h: Option<String>,
    #[serde(default)]
    pub quota_status_7d: Option<String>,
    #[serde(default)]
    pub quota_constraint: Option<String>,
    #[serde(default)]
    pub quota_reset_5h: Option<u64>,
    #[serde(default)]
    pub quota_reset_7d: Option<u64>,
    #[serde(default)]
    pub quota_updated_at: Option<String>,
    #[serde(default)]
    pub quota_alert: Option<String>,
}

fn default_quota() -> usize { 50 }

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
            context_source: None,
            model: None,
            compaction_suspected: false,
            last_nudge_type: String::new(),
            last_nudge_beat: 0,
            last_maintenance_beat: 0,
            last_recall_beat: 0,
            quota: default_quota(),
            last_prompt_at: None,
            response_latency_ms: None,
            started_pid: None,
            last_error: None,
            last_error_at: None,
            tool_call_count: 0,
            prompt_count: 0,
            plan_type: None,
            plan_tier: None,
            quota_5h: None,
            quota_7d: None,
            quota_status_5h: None,
            quota_status_7d: None,
            quota_constraint: None,
            quota_reset_5h: None,
            quota_reset_7d: None,
            quota_updated_at: None,
            quota_alert: None,
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

    /// Record that an agent interaction (prompt) occurred at current beat.
    pub fn record_interaction(
        &mut self,
        session_id: Option<&str>,
        thread_id: Option<&str>,
    ) {
        let now = Utc::now();
        self.last_interaction_at = now.to_rfc3339();
        self.last_interaction_beat = self.beat;
        if let Some(sid) = session_id {
            self.last_session_id = Some(sid.to_string());
        }
        if let Some(tid) = thread_id {
            self.last_thread_id = Some(tid.to_string());
        }
        // B1: Auto-initialize last_recall_beat on first interaction with existing beats.
        // Prevents parasitic "recall" nudge at beat 11 for new agents.
        if self.last_recall_beat == 0 && self.beat > 0 {
            self.last_recall_beat = self.beat;
        }
        // E3: Record prompt timestamp for latency measurement
        self.last_prompt_at = Some(now.to_rfc3339());
        // E6: Increment prompt counter
        self.prompt_count += 1;
    }

    /// E6: Record a tool call.
    pub fn record_tool_call(&mut self) {
        self.tool_call_count += 1;
    }

    /// E5: Record an error from MCP tool execution.
    pub fn record_error(&mut self, error: &str) {
        self.last_error = Some(if error.len() > 200 {
            error[..200].to_string()
        } else {
            error.to_string()
        });
        self.last_error_at = Some(Utc::now().to_rfc3339());
    }

    /// E3: Record response latency (call after tool response).
    pub fn record_response(&mut self) {
        if let Some(ref prompt_at) = self.last_prompt_at {
            if let Ok(t) = prompt_at.parse::<DateTime<Utc>>() {
                let ms = (Utc::now() - t).num_milliseconds();
                if ms >= 0 {
                    self.response_latency_ms = Some(ms as u64);
                }
            }
        }
    }

    /// E4: Reset started_at when PID changes (new session).
    pub fn reset_uptime_if_pid_changed(&mut self, current_pid: u32) {
        if self.started_pid != Some(current_pid) {
            self.started_at = Utc::now().to_rfc3339();
            self.started_pid = Some(current_pid);
            // Reset session counters
            self.tool_call_count = 0;
            self.prompt_count = 0;
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
        const MAX_SCHEDULED_WAKES: usize = 200;
        if self.scheduled_wakes.len() >= MAX_SCHEDULED_WAKES {
            self.scheduled_wakes.remove(0);
        }
        self.scheduled_wakes.push(ScheduledWake {
            target_beat: self.beat + after_beats,
            reason,
            created_at: Utc::now().to_rfc3339(),
        });
    }

    /// Adaptive throttle: should we update context tokens this prompt?
    /// Below 70%: time-based (30s). At/above 70%: delta-based (5% change).
    pub fn should_update_context(&self, new_percent: f64) -> bool {
        let elapsed = match &self.context_updated_at {
            Some(ts) => {
                let last: DateTime<Utc> = match ts.parse() {
                    Ok(t) => t,
                    Err(_) => return true,
                };
                (Utc::now() - last).num_seconds()
            }
            None => i64::MAX, // Never updated
        };

        let current = self.context_percent.unwrap_or(0.0);
        if current < 70.0 {
            elapsed >= 30 // Time-based below 70%
        } else {
            (new_percent - current).abs() >= 5.0 // Delta-based at/above 70%
        }
    }

    /// Update context tracking fields and detect compaction.
    pub fn update_context(&mut self, tokens: u64, percent: f64, source: &str, model: Option<String>) {
        // E2: Compaction detection — tokens dropped >40% from previous reading
        if let Some(prev) = self.context_tokens {
            if prev > 0 && tokens < prev * 60 / 100 {
                self.compaction_suspected = true;
                tracing::info!(
                    prev_tokens = prev, new_tokens = tokens,
                    "Compaction suspected: tokens dropped {:.0}%",
                    (1.0 - tokens as f64 / prev as f64) * 100.0
                );
            } else {
                self.compaction_suspected = false;
            }
        }
        self.context_tokens = Some(tokens);
        self.context_percent = Some(percent);
        self.context_updated_at = Some(Utc::now().to_rfc3339());
        self.context_source = Some(source.to_string());
        if model.is_some() {
            self.model = model;
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quota_persistence_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = BeatState::default();
        state.quota = 100;
        state.beat = 42;
        state.save(dir.path());

        let loaded = BeatState::load(dir.path());
        assert_eq!(loaded.quota, 100);
        assert_eq!(loaded.beat, 42);
    }

    #[test]
    fn test_backward_compat_missing_quota() {
        let dir = tempfile::tempdir().unwrap();
        // Write JSON without quota field
        let json = r#"{"beat":5,"started_at":"2026-01-01T00:00:00Z","last_beat_at":"2026-01-01T00:00:00Z","last_interaction_at":"2026-01-01T00:00:00Z","last_interaction_beat":0}"#;
        std::fs::write(dir.path().join("beat.json"), json).unwrap();

        let loaded = BeatState::load(dir.path());
        assert_eq!(loaded.beat, 5);
        assert_eq!(loaded.quota, 50, "Missing quota should default to 50");
    }
}
