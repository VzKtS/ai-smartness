//! Controller loop — CLI-first fallback for wake signal injection.
//!
//! When the VSCode extension is not active, the daemon controller takes over:
//! 1. Polls wake signals for all active agents (every 3s)
//! 2. Detects Claude CLI processes via beat.json PIDs
//! 3. Injects prompts via /proc/{pid}/fd/0 (Linux) when idle
//! 4. Falls back to inject_queue file if /proc is unavailable
//!
//! The controller coexists with the VSCode extension — only activates when
//! no extension heartbeat is detected.

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ai_smartness::storage::beat::BeatState;
use ai_smartness::storage::path_utils;

// ─── Constants ───

const POLL_INTERVAL_MS: u64 = 3_000;
const COOLDOWN_MS: u64 = 10_000;
const RETRY_BACKOFF_MS: u64 = 15_000;
const IDLE_CHECK_INTERVAL_MS: u64 = 1_000;
const MAX_ATTEMPTS: u32 = 3;
/// Idle threshold: time since last beat update to consider process idle.
const IDLE_THRESHOLD_SECS: u64 = 10;
/// TTL for inject_queue files (seconds). Older files are discarded.
const INJECT_QUEUE_TTL_SECS: u64 = 60;

// ─── Wake Signal ───

#[derive(Debug, serde::Deserialize)]
struct WakeSignal {
    agent_id: String,
    from: String,
    message: String,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    interrupt: bool,
    timestamp: String,
    #[serde(default)]
    acknowledged: bool,
}

// ─── Inject Queue Entry ───

#[derive(Debug, serde::Serialize)]
struct InjectQueueEntry {
    #[serde(rename = "type")]
    entry_type: String,
    text: String,
    timestamp: String,
    agent_id: String,
}

// ─── Agent Controller FSM ───

#[derive(Debug, Clone, Copy, PartialEq)]
enum ControllerState {
    Idle,
    Pending,
    Cooldown,
}

struct AgentController {
    agent_id: String,
    project_hash: String,
    state: ControllerState,
    current_signal_key: Option<String>,
    current_signal_text: Option<String>,
    current_signal_interrupt: bool,
    attempts: u32,
    last_attempt: Instant,
    cooldown_until: Instant,
    processed_keys: HashSet<String>,
}

impl AgentController {
    fn new(agent_id: String, project_hash: String) -> Self {
        let now = Instant::now();
        Self {
            agent_id,
            project_hash,
            state: ControllerState::Idle,
            current_signal_key: None,
            current_signal_text: None,
            current_signal_interrupt: false,
            attempts: 0,
            last_attempt: now,
            cooldown_until: now,
            processed_keys: HashSet::new(),
        }
    }

    /// Non-blocking tick. Returns immediately after advancing state machine one step.
    fn tick(&mut self) {
        match self.state {
            ControllerState::Idle => self.check_for_signal(),
            ControllerState::Pending => self.try_inject_now(),
            ControllerState::Cooldown => {
                if Instant::now() >= self.cooldown_until {
                    if self.current_signal_key.is_some() {
                        self.state = ControllerState::Pending;
                    } else {
                        self.state = ControllerState::Idle;
                    }
                }
            }
        }
    }

    fn check_for_signal(&mut self) {
        let signal = match read_wake_signal(&self.agent_id) {
            Some(s) => s,
            None => return,
        };

        if signal.acknowledged {
            return;
        }

        let key = format!("{}_{}", signal.agent_id, signal.timestamp);
        if self.processed_keys.contains(&key) {
            return;
        }
        self.processed_keys.insert(key.clone());

        let mode = signal.mode.as_deref().unwrap_or("inbox");
        let text = build_prompt_text(&self.agent_id, &signal.from, &signal.message, mode);

        tracing::info!(
            agent = %self.agent_id,
            from = %signal.from,
            "Controller: wake signal detected"
        );

        // Interrupt: bypass idle check, inject immediately
        if signal.interrupt {
            let ok = try_inject(
                &self.project_hash,
                &self.agent_id,
                &text,
                true, // skip idle check
            );
            if ok {
                tracing::info!(agent = %self.agent_id, "Controller: interrupt injected");
                acknowledge_signal(&self.agent_id);
                self.enter_cooldown();
                return;
            }
            // Fall through to pending path if injection failed
        }

        self.current_signal_key = Some(key);
        self.current_signal_text = Some(text);
        self.current_signal_interrupt = signal.interrupt;
        self.attempts = 0;
        self.state = ControllerState::Pending;
    }

    fn try_inject_now(&mut self) {
        let text = match &self.current_signal_text {
            Some(t) => t.clone(),
            None => {
                self.state = ControllerState::Idle;
                return;
            }
        };

        // Rate-limit attempts
        let elapsed = self.last_attempt.elapsed().as_millis() as u64;
        if elapsed < IDLE_CHECK_INTERVAL_MS {
            return;
        }
        self.last_attempt = Instant::now();
        self.attempts += 1;

        let ok = try_inject(
            &self.project_hash,
            &self.agent_id,
            &text,
            self.current_signal_interrupt,
        );

        if ok {
            tracing::info!(agent = %self.agent_id, "Controller: wake injected");
            acknowledge_signal(&self.agent_id);
            self.current_signal_key = None;
            self.current_signal_text = None;
            self.enter_cooldown();
            return;
        }

        if self.attempts >= MAX_ATTEMPTS {
            tracing::debug!(
                agent = %self.agent_id,
                "Controller: injection round failed, retrying in {}s",
                RETRY_BACKOFF_MS / 1000
            );
            self.attempts = 0;
            self.cooldown_until = Instant::now() + Duration::from_millis(RETRY_BACKOFF_MS);
            self.state = ControllerState::Cooldown;
        }
    }

    fn enter_cooldown(&mut self) {
        self.state = ControllerState::Cooldown;
        self.cooldown_until = Instant::now() + Duration::from_millis(COOLDOWN_MS);
    }

    fn cleanup(&mut self) {
        if self.processed_keys.len() > 100 {
            self.processed_keys.clear();
        }
    }
}

// ─── Wake Signal I/O ───

fn read_wake_signal(agent_id: &str) -> Option<WakeSignal> {
    let path = path_utils::wake_signal_path(agent_id);
    let content = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

fn acknowledge_signal(agent_id: &str) {
    let path = path_utils::wake_signal_path(agent_id);
    if let Ok(content) = std::fs::read_to_string(&path) {
        if let Ok(mut signal) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(obj) = signal.as_object_mut() {
                obj.insert("acknowledged".into(), serde_json::json!(true));
                obj.insert(
                    "acknowledged_at".into(),
                    serde_json::json!(chrono::Utc::now().to_rfc3339()),
                );
                let _ = std::fs::write(&path, serde_json::to_string_pretty(&signal).unwrap_or_default());
            }
        }
    }
}

// ─── Prompt Building ───

fn build_prompt_text(agent_id: &str, from_agent: &str, message_body: &str, mode: &str) -> String {
    if mode == "cognitive" {
        format!(
            "[automated cognitive wake for {}] You have pending cognitive messages from \"{}\" \
             about: \"{}\". Check your cognitive inbox context above and respond to the message. \
             Use ai_msg_ack to acknowledge after processing.",
            agent_id, from_agent, message_body
        )
    } else {
        format!(
            "[automated inbox wake for {}] You have a message from \"{}\": \"{}\". \
             Call msg_inbox to read your pending messages and reply.",
            agent_id, from_agent, message_body
        )
    }
}

/// Build the JSON payload for stdin injection (Claude Code internal format).
fn build_stdin_payload(text: &str) -> String {
    let payload = serde_json::json!({
        "type": "user",
        "session_id": "",
        "message": {
            "role": "user",
            "content": [{"type": "text", "text": text}]
        },
        "parent_tool_use_id": null
    });
    format!("{}\n", payload)
}

// ─── Injection ───

/// Try to inject a prompt into a Claude CLI process.
/// Strategy:
///   1. Read cli_pid from beat.json
///   2. Check process alive (kill -0)
///   3. Check idle (beat.json last_beat_at age)
///   4. Write to /proc/{pid}/fd/0 (Linux)
///   5. If /proc fails: fall back to inject_queue file
fn try_inject(project_hash: &str, agent_id: &str, text: &str, skip_idle_check: bool) -> bool {
    let data_dir = path_utils::agent_data_dir(project_hash, agent_id);
    let beat = BeatState::load(&data_dir);

    // Read cli_pid (or mcp pid as fallback)
    let target_pid = match beat.cli_pid.or(beat.pid) {
        Some(pid) => pid,
        None => {
            tracing::debug!(agent = agent_id, "No PID in beat.json, cannot inject");
            return false;
        }
    };

    // Check process alive
    if !is_process_alive(target_pid) {
        tracing::debug!(agent = agent_id, pid = target_pid, "Target process not alive");
        return false;
    }

    // Check idle (unless skip requested for interrupt signals)
    if !skip_idle_check && !is_process_idle(&beat) {
        tracing::debug!(agent = agent_id, "Process not idle, skipping injection");
        return false;
    }

    let payload = build_stdin_payload(text);

    // Strategy 1: /proc/{pid}/fd/0 (Linux)
    #[cfg(target_os = "linux")]
    {
        let stdin_path = format!("/proc/{}/fd/0", target_pid);
        let path = std::path::Path::new(&stdin_path);

        // Verify the fd exists and is accessible
        if path.exists() {
            // Verify it points to a PTY (not a pipe or /dev/null)
            if let Ok(link) = std::fs::read_link(path) {
                let link_str = link.to_string_lossy();
                if link_str.starts_with("/dev/pts/") || link_str.starts_with("/dev/tty") {
                    match std::fs::OpenOptions::new().write(true).open(path) {
                        Ok(mut f) => {
                            match f.write_all(payload.as_bytes()) {
                                Ok(()) => {
                                    tracing::info!(
                                        agent = agent_id,
                                        pid = target_pid,
                                        "Injected via /proc/fd/0"
                                    );
                                    return true;
                                }
                                Err(e) => {
                                    tracing::debug!(
                                        agent = agent_id,
                                        error = %e,
                                        "Write to /proc/fd/0 failed, falling back to inject_queue"
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            tracing::debug!(
                                agent = agent_id,
                                error = %e,
                                "Cannot open /proc/fd/0, falling back to inject_queue"
                            );
                        }
                    }
                } else {
                    tracing::debug!(
                        agent = agent_id,
                        link = %link_str,
                        "/proc/fd/0 not a PTY, falling back to inject_queue"
                    );
                }
            }
        }
    }

    // Strategy 2: inject_queue file (cross-platform fallback)
    write_inject_queue(project_hash, agent_id, text);
    true
}

/// Check if a process is alive via kill(pid, 0).
fn is_process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

/// Check if the Claude process is idle based on beat.json timestamps.
/// Considers idle if last_beat_at is older than IDLE_THRESHOLD_SECS.
fn is_process_idle(beat: &BeatState) -> bool {
    let last_beat = match chrono::DateTime::parse_from_rfc3339(&beat.last_beat_at) {
        Ok(dt) => dt,
        Err(_) => return true, // Can't parse → assume idle
    };
    let elapsed = chrono::Utc::now()
        .signed_duration_since(last_beat)
        .num_seconds();
    elapsed >= IDLE_THRESHOLD_SECS as i64
}

// ─── Inject Queue (cross-platform fallback) ───

/// Write an inject_queue file for the hook to pick up.
fn write_inject_queue(project_hash: &str, agent_id: &str, text: &str) {
    let data_dir = path_utils::agent_data_dir(project_hash, agent_id);
    let queue_dir = data_dir.join("inject_queue");
    if std::fs::create_dir_all(&queue_dir).is_err() {
        tracing::warn!(agent = agent_id, "Failed to create inject_queue dir");
        return;
    }

    let entry = InjectQueueEntry {
        entry_type: "controller_wake".into(),
        text: text.into(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        agent_id: agent_id.into(),
    };

    let filename = format!("{}_{}.json", chrono::Utc::now().timestamp_millis(), agent_id);
    let path = queue_dir.join(&filename);

    match std::fs::write(&path, serde_json::to_string(&entry).unwrap_or_default()) {
        Ok(()) => {
            tracing::info!(
                agent = agent_id,
                path = %path.display(),
                "Wrote inject_queue entry"
            );
        }
        Err(e) => {
            tracing::warn!(agent = agent_id, error = %e, "Failed to write inject_queue");
        }
    }
}

/// Read and consume inject_queue files for an agent. Returns consumed text entries.
/// Called by hook inject.rs (Layer 1.8). Deletes files after reading.
pub fn consume_inject_queue(project_hash: &str, agent_id: &str) -> Vec<String> {
    let data_dir = path_utils::agent_data_dir(project_hash, agent_id);
    let queue_dir = data_dir.join("inject_queue");
    if !queue_dir.exists() {
        return Vec::new();
    }

    let entries = match std::fs::read_dir(&queue_dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let now = chrono::Utc::now();
    let mut results = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }

        // Read and parse
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => {
                let _ = std::fs::remove_file(&path); // Remove unreadable
                continue;
            }
        };

        let parsed: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => {
                let _ = std::fs::remove_file(&path); // Remove unparseable
                continue;
            }
        };

        // TTL check: discard files older than INJECT_QUEUE_TTL_SECS
        if let Some(ts) = parsed.get("timestamp").and_then(|v| v.as_str()) {
            if let Ok(file_time) = chrono::DateTime::parse_from_rfc3339(ts) {
                let age = now.signed_duration_since(file_time).num_seconds();
                if age > INJECT_QUEUE_TTL_SECS as i64 {
                    tracing::debug!(path = %path.display(), age, "inject_queue: TTL expired, discarding");
                    let _ = std::fs::remove_file(&path);
                    continue;
                }
            }
        }

        // Extract text
        if let Some(text) = parsed.get("text").and_then(|v| v.as_str()) {
            results.push(text.to_string());
        }

        // Delete consumed file
        let _ = std::fs::remove_file(&path);
    }

    results
}

/// Cleanup stale inject_queue files (called periodically by controller).
fn cleanup_inject_queue(project_hash: &str, agent_id: &str) {
    let data_dir = path_utils::agent_data_dir(project_hash, agent_id);
    let queue_dir = data_dir.join("inject_queue");
    if !queue_dir.exists() {
        return;
    }

    let now = chrono::Utc::now();
    let entries = match std::fs::read_dir(&queue_dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }

        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(ts) = parsed.get("timestamp").and_then(|v| v.as_str()) {
                    if let Ok(file_time) = chrono::DateTime::parse_from_rfc3339(ts) {
                        let age = now.signed_duration_since(file_time).num_seconds();
                        if age > INJECT_QUEUE_TTL_SECS as i64 {
                            let _ = std::fs::remove_file(&path);
                        }
                    }
                }
            }
        }
    }
}

// ─── Agent Discovery ───

/// Discover all active agents across all projects by scanning session_agents/ directories
/// and beat.json files for alive PIDs.
fn discover_active_agents() -> Vec<(String, String)> {
    let projects_dir = path_utils::projects_dir();
    let mut agents = Vec::new();

    let project_entries = match std::fs::read_dir(&projects_dir) {
        Ok(e) => e,
        Err(_) => return agents,
    };

    for project_entry in project_entries.flatten() {
        let project_hash = project_entry.file_name().to_string_lossy().to_string();

        // 1. Session agents directory
        let sa_dir = path_utils::session_agents_dir(&project_hash);
        if let Ok(session_entries) = std::fs::read_dir(&sa_dir) {
            for se in session_entries.flatten() {
                // Files are named {session_id}_{agent_id} or just {agent_id}
                if let Ok(content) = std::fs::read_to_string(se.path()) {
                    let agent_id = content.trim().to_string();
                    if !agent_id.is_empty() {
                        agents.push((project_hash.clone(), agent_id));
                    }
                }
            }
        }

        // 2. Agents directory — check beat.json for alive PIDs
        let agents_dir = project_entry.path().join("agents");
        if let Ok(agent_entries) = std::fs::read_dir(&agents_dir) {
            for ae in agent_entries.flatten() {
                let agent_path = ae.path();
                if !agent_path.is_dir() {
                    continue;
                }
                let agent_id = ae.file_name().to_string_lossy().to_string();
                let beat_path = agent_path.join("beat.json");
                if beat_path.exists() {
                    let beat = BeatState::load(&agent_path);
                    if let Some(pid) = beat.cli_pid.or(beat.pid) {
                        if is_process_alive(pid) {
                            agents.push((project_hash.clone(), agent_id));
                        }
                    }
                }
            }
        }
    }

    // Deduplicate
    agents.sort();
    agents.dedup();
    agents
}

// ─── VSCode Extension Detection ───

/// Check if the VSCode extension is handling wake signals for this agent.
/// The extension writes to a pidfile or we detect it via beat.json patterns.
/// For now: check if a VSCode extension process is actively modifying wake signal files.
///
/// Simple heuristic: if the wake signal was acknowledged within the last 30s,
/// something else (likely the extension) is handling it. Don't interfere.
fn is_extension_active_for_agent(agent_id: &str) -> bool {
    let signal = match read_wake_signal(agent_id) {
        Some(s) => s,
        None => return false, // No signal → no one is handling anything
    };

    // If signal is acknowledged recently, extension (or something) is active
    if signal.acknowledged {
        // Check if acknowledged_at is recent (within 30s)
        // Parse from the raw file since our struct doesn't have acknowledged_at
        let path = path_utils::wake_signal_path(agent_id);
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(ack_at) = parsed.get("acknowledged_at").and_then(|v| v.as_str()) {
                    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ack_at) {
                        let age = chrono::Utc::now().signed_duration_since(dt).num_seconds();
                        if age < 30 {
                            return true; // Recently acknowledged → extension is active
                        }
                    }
                }
            }
        }
    }

    false
}

// ─── Main Controller Loop ───

/// Run the controller loop. Spawned as a thread alongside the prune loop.
pub fn run_controller_loop(running: Arc<AtomicBool>) {
    let interval = Duration::from_millis(POLL_INTERVAL_MS);
    let mut controllers: HashMap<(String, String), AgentController> = HashMap::new();
    let mut last_cleanup = Instant::now();

    tracing::info!("Controller loop started (poll={}ms)", POLL_INTERVAL_MS);

    while running.load(Ordering::Relaxed) {
        std::thread::sleep(interval);
        if !running.load(Ordering::Relaxed) {
            break;
        }

        // Discover active agents
        let active_agents = discover_active_agents();

        if active_agents.is_empty() {
            continue;
        }

        // Sync controllers: add new, remove disappeared
        let active_set: HashSet<(String, String)> = active_agents.iter().cloned().collect();

        // Remove controllers for agents no longer active
        controllers.retain(|key, _| active_set.contains(key));

        // Add controllers for new agents
        for (project_hash, agent_id) in &active_agents {
            let key = (project_hash.clone(), agent_id.clone());
            controllers.entry(key).or_insert_with(|| {
                tracing::debug!(agent = %agent_id, "Controller: tracking new agent");
                AgentController::new(agent_id.clone(), project_hash.clone())
            });
        }

        // Tick each controller (skip agents where extension is active)
        for ((_, agent_id), ctrl) in controllers.iter_mut() {
            if is_extension_active_for_agent(agent_id) {
                continue;
            }
            ctrl.tick();
        }

        // Periodic cleanup (every 60s)
        if last_cleanup.elapsed() > Duration::from_secs(60) {
            for ((project_hash, agent_id), ctrl) in controllers.iter_mut() {
                ctrl.cleanup();
                cleanup_inject_queue(project_hash, agent_id);
            }
            last_cleanup = Instant::now();
        }
    }

    tracing::info!("Controller loop stopped");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_prompt_text_inbox() {
        let text = build_prompt_text("coder1", "cor", "Task delegated: Fix tests", "inbox");
        assert!(text.contains("[automated inbox wake for coder1]"));
        assert!(text.contains("\"cor\""));
        assert!(text.contains("msg_inbox"));
    }

    #[test]
    fn test_build_prompt_text_cognitive() {
        let text = build_prompt_text("coder1", "cor", "Review needed", "cognitive");
        assert!(text.contains("[automated cognitive wake for coder1]"));
        assert!(text.contains("ai_msg_ack"));
    }

    #[test]
    fn test_build_stdin_payload_json_line() {
        let payload = build_stdin_payload("Hello agent");
        assert!(payload.ends_with('\n'));
        let parsed: serde_json::Value = serde_json::from_str(payload.trim()).unwrap();
        assert_eq!(parsed["type"], "user");
        assert_eq!(parsed["session_id"], "");
        assert_eq!(parsed["message"]["role"], "user");
        assert_eq!(parsed["message"]["content"][0]["type"], "text");
        assert_eq!(parsed["message"]["content"][0]["text"], "Hello agent");
        assert!(parsed["parent_tool_use_id"].is_null());
    }

    #[test]
    fn test_controller_fsm_initial_state() {
        let ctrl = AgentController::new("test".into(), "proj123".into());
        assert_eq!(ctrl.state, ControllerState::Idle);
        assert_eq!(ctrl.attempts, 0);
        assert!(ctrl.processed_keys.is_empty());
    }

    #[test]
    fn test_controller_cleanup_over_100() {
        let mut ctrl = AgentController::new("test".into(), "proj123".into());
        for i in 0..150 {
            ctrl.processed_keys.insert(format!("key_{}", i));
        }
        assert_eq!(ctrl.processed_keys.len(), 150);
        ctrl.cleanup();
        assert!(ctrl.processed_keys.is_empty());
    }

    #[test]
    fn test_is_process_idle_old_beat() {
        let mut beat = BeatState::default();
        // Set last_beat_at to 60 seconds ago
        beat.last_beat_at = (chrono::Utc::now() - chrono::Duration::seconds(60)).to_rfc3339();
        assert!(is_process_idle(&beat));
    }

    #[test]
    fn test_is_process_idle_recent_beat() {
        let mut beat = BeatState::default();
        // Set last_beat_at to now
        beat.last_beat_at = chrono::Utc::now().to_rfc3339();
        assert!(!is_process_idle(&beat));
    }

    #[test]
    fn test_is_process_idle_invalid_timestamp() {
        let mut beat = BeatState::default();
        beat.last_beat_at = "not-a-timestamp".into();
        // Invalid timestamp → assume idle
        assert!(is_process_idle(&beat));
    }

    #[test]
    fn test_consume_inject_queue_empty() {
        let dir = tempfile::tempdir().unwrap();
        // No inject_queue dir → empty result
        let results = consume_inject_queue("nonexistent", "nonexistent");
        assert!(results.is_empty());
        drop(dir);
    }

    #[test]
    fn test_inject_queue_entry_serialization() {
        let entry = InjectQueueEntry {
            entry_type: "controller_wake".into(),
            text: "Hello agent".into(),
            timestamp: "2026-02-21T00:00:00Z".into(),
            agent_id: "test".into(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["type"], "controller_wake");
        assert_eq!(parsed["text"], "Hello agent");
    }
}
