//! Injection hook — UserPromptSubmit handler.
//!
//! Reads user prompt from stdin, builds injection layers as <system-reminder> blocks,
//! outputs augmented prompt to stdout.
//!
//! Injection layers:
//!   0.   Agent selection prompt (when unbound)
//!   0.5  Onboarding (first session only)
//!   1.   Lightweight context (thread count, beat, memory pressure)
//!   1.5  Session state (continuity, files modified, resume context)
//!   2.   Cognitive inbox (inter-agent messages)
//!   3.   Pins (user-pinned content + pins.json with expiration)
//!   4.   Memory retrieval (similar threads)
//!   5.   Agent identity (role, hierarchy)
//!   5.5  User profile + rules (preferences, auto-detected behavior)
//!   6.   HealthGuard (merge candidates, capacity alerts — imposed on agent)

use std::path::Path;

use ai_smartness::config::GuardianConfig;
use ai_smartness::constants::{MAX_COGNITIVE_MESSAGES, MAX_CONTEXT_SIZE};
use ai_smartness::healthguard::{self, HealthGuard};
use ai_smartness::thread::{InjectionStats, ThreadStatus};
use ai_smartness::config::EngramConfig;
use ai_smartness::intelligence::engram_retriever::EngramRetriever;
use ai_smartness::session::SessionState;
use ai_smartness::storage::beat::BeatState;
use ai_smartness::user_profile::UserProfile;
use ai_smartness::storage::cognitive_inbox::CognitiveInbox;
use ai_smartness::storage::database::{open_connection, ConnectionRole};
use ai_smartness::storage::migrations;
use ai_smartness::storage::path_utils;
use ai_smartness::storage::threads::ThreadStorage;
use rusqlite::Connection;

/// Run the inject hook.
/// `input` is the raw stdin already read by hook/mod.rs.
/// `session_id` is extracted from the hook JSON (per-session isolation).
pub fn run(project_hash: &str, agent_id: &str, input: &str, session_id: Option<&str>) {
    tracing::info!(project = project_hash, agent = agent_id, session_id = ?session_id, "inject::run() called");

    tracing::debug!(input_len = input.len(), input_preview = %&input[..input.len().min(80)], "stdin read");

    let message = extract_message(input);
    tracing::debug!(message_len = message.len(), "Message extracted");

    if message.trim().is_empty() {
        tracing::info!("Inject: empty prompt, passing through");
        print!("{}", input);
        return;
    }

    tracing::info!(prompt_len = message.len(), budget = MAX_CONTEXT_SIZE, "Inject hook building layers");

    // 2. Build injection layers
    let mut injections: Vec<String> = Vec::new();
    let mut budget = MAX_CONTEXT_SIZE;

    // Layer 0.5: Onboarding prompt (first session only)
    match build_onboarding_prompt(project_hash, agent_id) {
        Some(prompt) => {
            let layer = format!("<system-reminder>\n{}\n</system-reminder>", prompt);
            if layer.len() < budget {
                budget -= layer.len();
                injections.push(layer);
                tracing::info!("Layer 0.5: onboarding prompt injected (first session)");
            }
        }
        None => tracing::debug!("Layer 0.5: onboarding already done or not applicable"),
    }

    // 3. Open DB (Hook role: ephemeral connection)
    tracing::debug!(project = project_hash, agent = agent_id, "Opening agent DB");
    let conn = match open_agent_db(project_hash, agent_id) {
        Some(c) => {
            tracing::info!(agent = agent_id, "Agent DB opened successfully");
            c
        }
        None => {
            tracing::info!(
                agent = agent_id,
                layers = injections.len(),
                "Agent DB not found — outputting with Layer 0 only"
            );
            if !injections.is_empty() {
                let injection = injections.join("\n");
                print!("{}\n\n{}", injection, message);
            } else {
                print!("{}", message);
            }
            return;
        }
    };

    // Resolve agent data dir for beat, pins, etc.
    let agent_data = path_utils::agent_data_dir(project_hash, agent_id);

    // Record interaction in beat system
    let mut beat_state = BeatState::load(&agent_data);
    beat_state.record_interaction(None, None);
    beat_state.save(&agent_data);

    // Load and update session state
    let mut session = SessionState::load(&agent_data, agent_id, project_hash);
    session.record_prompt(&message);
    session.save(&agent_data);

    // Send prompt to daemon for extraction (fire-and-forget)
    let _ = ai_smartness::processing::daemon_ipc_client::send_capture(
        project_hash, agent_id, "prompt", &message,
    );

    // Load user profile, auto-detect traits and rules from message
    let mut profile = UserProfile::load(&agent_data);
    profile.detect_from_message(&message);
    if let Some(rule) = profile.detect_rules(&message) {
        tracing::info!(rule = %rule, "Auto-detected user rule");
    }
    profile.save(&agent_data);

    // Pre-fetch agent identity + authoritative quota from registry.
    // Called early so agent_quota is available to layers 1.7, 4, and 6.
    // The identity string is injected later at Layer 5.
    let (agent_identity, agent_quota) = match build_agent_identity(project_hash, agent_id) {
        Some((ctx, quota)) => (Some(ctx), quota),
        None => {
            tracing::debug!(agent = agent_id, "Agent not found in registry, using beat.json quota fallback");
            (None, beat_state.quota)
        }
    };

    // Layer 1: Lightweight context + beat + session_id
    match build_lightweight_context(&conn, &agent_data, session_id) {
        Some(ctx) => {
            let layer = format!("<system-reminder>\n{}\n</system-reminder>", ctx);
            if layer.len() < budget {
                budget -= layer.len();
                injections.push(layer);
                tracing::debug!("Layer 1: lightweight context added");
            } else {
                tracing::debug!("Layer 1: exceeds budget, skipped");
            }
        }
        None => tracing::debug!("Layer 1: no context available"),
    }

    // Layer 1.5: Session state (continuity context)
    match build_session_context(&session, &beat_state) {
        Some(ctx) => {
            let layer = format!("<system-reminder>\n{}\n</system-reminder>", ctx);
            if layer.len() < budget {
                budget -= layer.len();
                injections.push(layer);
                tracing::debug!("Layer 1.5: session state added");
            } else {
                tracing::debug!("Layer 1.5: exceeds budget, skipped");
            }
        }
        None => tracing::debug!("Layer 1.5: no session context"),
    }

    // Layer 1.7: Cognitive nudge (conditional maintenance reminder)
    if let Some((nudge_type, nudge_msg)) = build_cognitive_nudge(&conn, &beat_state, agent_quota) {
        let truncated = if nudge_msg.len() > 300 { &nudge_msg[..300] } else { &nudge_msg };
        let layer = format!("<system-reminder>\nCognitive maintenance: {}\n</system-reminder>", truncated);
        if layer.len() < budget {
            budget -= layer.len();
            injections.push(layer);
            beat_state.last_nudge_type = nudge_type.clone();
            beat_state.last_nudge_beat = beat_state.beat;
            if nudge_type == "maintenance" {
                beat_state.last_maintenance_beat = beat_state.beat;
            }
            beat_state.save(&agent_data);
            tracing::info!(nudge_type = %nudge_type, "Layer 1.7: Cognitive nudge injected");
        }
    }

    // Layer 2: Cognitive inbox
    // Only consume (mark read) on wake prompts — peek on normal prompts
    // so the message stays pending for the actual wake delivery.
    let is_wake_prompt = message.contains("[automated inbox wake")
        || message.contains("[automated cognitive wake");
    match build_cognitive_inbox(&conn, agent_id, is_wake_prompt) {
        Some(ctx) => {
            let layer = format!("<system-reminder>\n{}\n</system-reminder>", ctx);
            if layer.len() < budget {
                budget -= layer.len();
                injections.push(layer);
                tracing::info!("Layer 2: cognitive inbox injected");
            } else {
                tracing::debug!("Layer 2: exceeds budget, skipped");
            }
        }
        None => tracing::debug!("Layer 2: no pending messages"),
    }

    // Layer 3: Pins (threads tagged __pin__ + pins.json file)
    match build_pins_context(&conn, &agent_data) {
        Some(ctx) => {
            let layer = format!("<system-reminder>\n{}\n</system-reminder>", ctx);
            if layer.len() < budget {
                budget -= layer.len();
                injections.push(layer);
                tracing::debug!("Layer 3: pins injected");
            } else {
                tracing::debug!("Layer 3: exceeds budget, skipped");
            }
        }
        None => tracing::debug!("Layer 3: no pins"),
    }

    // Layer 4: Memory retrieval (similar threads — the main layer)
    match build_memory_context(&conn, &message, agent_quota) {
        Some(ctx) => {
            let layer = format!("<system-reminder>\n{}\n</system-reminder>", ctx);
            if layer.len() < budget {
                let size = layer.len();
                budget -= size;
                injections.push(layer);
                tracing::info!(size, "Layer 4: memory context injected");
            } else {
                tracing::debug!(size = layer.len(), "Layer 4: exceeds budget, skipped");
            }
        }
        None => tracing::debug!("Layer 4: no similar threads found"),
    }

    // Layer 5: Agent identity — injected on every prompt.
    // Per-session agent files (keyed by session_id) ensure correct agent resolution
    // even with multiple panels open simultaneously.
    // (build_agent_identity was called earlier to also extract agent_quota)
    match agent_identity {
        Some(ctx) => {
            let layer = format!("<system-reminder>\n{}\n</system-reminder>", ctx);
            if layer.len() < budget {
                budget -= layer.len();
                injections.push(layer);
                tracing::info!("Layer 5: agent identity injected");
            } else {
                tracing::debug!("Layer 5: exceeds budget, skipped");
            }
        }
        None => tracing::debug!(agent = agent_id, "Layer 5: agent not found in registry"),
    }

    // Layer 5.5: User profile + rules
    if let Some(ctx) = profile.build_injection() {
        let layer = format!("<system-reminder>\n{}\n</system-reminder>", ctx);
        if layer.len() < budget {
            budget -= layer.len();
            injections.push(layer);
            tracing::debug!("Layer 5.5: user profile injected");
        } else {
            tracing::debug!("Layer 5.5: exceeds budget, skipped");
        }
    }

    // Layer 6: HealthGuard — merge candidates, capacity alerts (imposed on agent)
    match build_healthguard_injection(&conn, &agent_data, project_hash, &beat_state, agent_quota) {
        Some(ctx) => {
            let layer = format!("<system-reminder>\n{}\n</system-reminder>", ctx);
            if layer.len() < budget {
                let _ = budget;
                injections.push(layer);
                tracing::info!("Layer 6: HealthGuard injection");
            } else {
                tracing::debug!("Layer 6: exceeds budget, skipped");
            }
        }
        None => tracing::debug!("Layer 6: HealthGuard clean or in cooldown"),
    }

    // 4. Output augmented prompt
    if injections.is_empty() {
        tracing::info!("Inject: no layers built, passing through unchanged");
        print!("{}", message);
    } else {
        let injection = injections.join("\n");
        tracing::info!(
            layers = injections.len(),
            total_size = injection.len(),
            remaining_budget = budget,
            "Inject: augmented prompt ready"
        );
        print!("{}\n\n{}", injection, message);
    }
    tracing::info!("inject::run() completed");
}

fn extract_message(input: &str) -> String {
    // Try parsing as JSON first
    if let Ok(data) = serde_json::from_str::<serde_json::Value>(input) {
        if let Some(msg) = data
            .get("prompt")
            .or_else(|| data.get("message"))
            .and_then(|v| v.as_str())
        {
            tracing::debug!("Message extracted from JSON field");
            return msg.to_string();
        }
    }
    // Fallback: treat entire input as the message
    input.to_string()
}

fn open_agent_db(project_hash: &str, agent_id: &str) -> Option<Connection> {
    let db_path = path_utils::agent_db_path(project_hash, agent_id);
    tracing::debug!(path = %db_path.display(), exists = db_path.exists(), "Checking agent DB");
    if !db_path.exists() {
        tracing::debug!(path = %db_path.display(), "Agent DB does not exist");
        return None;
    }
    let conn = match open_connection(&db_path, ConnectionRole::Hook) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "Failed to open agent DB");
            return None;
        }
    };
    if let Err(e) = migrations::migrate_agent_db(&conn) {
        tracing::warn!(error = %e, "Failed to migrate agent DB");
        return None;
    }
    Some(conn)
}

/// Layer 1: Lightweight context — thread count, beat info, session_id.
fn build_lightweight_context(conn: &Connection, agent_data_dir: &Path, session_id: Option<&str>) -> Option<String> {
    let active = ThreadStorage::count_by_status(conn, &ThreadStatus::Active).unwrap_or(0);
    let suspended = ThreadStorage::count_by_status(conn, &ThreadStatus::Suspended).unwrap_or(0);
    let total = ThreadStorage::count(conn).unwrap_or(0);

    let beat = BeatState::load(agent_data_dir);

    let mut ctx = serde_json::json!({
        "active_threads": active,
        "suspended_threads": suspended,
        "total_threads": total,
        "beat": beat.beat,
        "since_last_interaction": beat.since_last(),
    });

    // Include session_id so the AI can pass it to ai_agent_select for multi-panel isolation
    if let Some(sid) = session_id {
        ctx["session_id"] = serde_json::Value::String(sid.to_string());
    }

    Some(format!(
        "As you answer the user's questions, you can use the following context:\n{}",
        serde_json::to_string_pretty(&ctx).ok()?
    ))
}

/// Layer 2: Cognitive inbox — inter-agent messages.
/// `consume`: true on wake prompts (marks read), false on normal prompts (peek only).
fn build_cognitive_inbox(conn: &Connection, agent_id: &str, consume: bool) -> Option<String> {
    let messages = if consume {
        CognitiveInbox::read_pending(conn, agent_id).ok()?
    } else {
        CognitiveInbox::peek_pending(conn, agent_id).ok()?
    };
    if messages.is_empty() {
        return None;
    }

    let mut ctx = String::from("You have pending cognitive messages:\n");
    for msg in messages.iter().take(MAX_COGNITIVE_MESSAGES) {
        ctx.push_str(&format!(
            "- From {}: [{}] {}\n",
            msg.from_agent, msg.subject, msg.content
        ));
        // Render file attachments inline
        for att in &msg.attachments {
            ctx.push_str(&format!(
                "  [Attached: {}]\n```\n{}\n```\n",
                att.filename, att.content
            ));
        }
    }
    if messages.len() > MAX_COGNITIVE_MESSAGES {
        ctx.push_str(&format!(
            "  ({} more messages pending)\n",
            messages.len() - MAX_COGNITIVE_MESSAGES
        ));
    }

    Some(ctx)
}

/// Layer 3: Pins — user-pinned high-priority content from threads + pins.json.
fn build_pins_context(conn: &Connection, agent_data_dir: &Path) -> Option<String> {
    let mut ctx = String::from("Pinned content:\n");
    let mut count = 0;

    // Source 1: threads with __pin__ tag
    if let Ok(all) = ThreadStorage::list_active(conn) {
        let pins: Vec<_> = all
            .iter()
            .filter(|t| t.tags.contains(&"__pin__".to_string()))
            .collect();
        for pin in pins.iter().take(5) {
            ctx.push_str(&format!("- {}", pin.title));
            if let Some(ref summary) = pin.summary {
                let s: String = summary.chars().take(100).collect();
                ctx.push_str(&format!(": {}", s));
            }
            ctx.push('\n');
            count += 1;
        }
    }

    // Source 2: pins.json file (with expiration support)
    let pins_path = agent_data_dir.join("pins.json");
    if count < 5 {
        if let Ok(content) = std::fs::read_to_string(&pins_path) {
            if let Ok(data) = serde_json::from_str::<serde_json::Value>(&content) {
                let now = chrono::Utc::now();
                if let Some(pins) = data.get("pins").and_then(|p| p.as_array()) {
                    for pin in pins.iter().take(5 - count) {
                        // Check expiration
                        if let Some(expires) = pin.get("expires_at").and_then(|e| e.as_str()) {
                            if let Ok(exp) = chrono::DateTime::parse_from_rfc3339(expires) {
                                if exp < now {
                                    continue;
                                }
                            }
                        }
                        if let Some(pin_content) = pin.get("content").and_then(|c| c.as_str()) {
                            let truncated = &pin_content[..pin_content.len().min(200)];
                            ctx.push_str(&format!("- {}\n", truncated));
                            count += 1;
                        }
                    }
                }
            }
        }
    }

    if count == 0 {
        None
    } else {
        Some(ctx)
    }
}

/// Layer 4: Memory retrieval — find similar threads via Engram 9-validator pipeline.
fn build_memory_context(conn: &Connection, message: &str, agent_quota: usize) -> Option<String> {
    let engram = EngramRetriever::new(conn, EngramConfig::default()).ok()?;
    let threads = engram.get_relevant_context(conn, message, 5).ok()?;
    if threads.is_empty() {
        return None;
    }

    // Re-injection feedback: update last_active, reactivate, and record injection stats
    let active_count = ThreadStorage::count(conn).unwrap_or(0);
    let mut reactivated = 0usize;
    for thread in &threads {
        match thread.status {
            ThreadStatus::Suspended | ThreadStatus::Archived => {
                // Cap reactivations: max 3 per cycle, don't exceed quota
                if reactivated >= 3 || active_count + reactivated >= agent_quota {
                    tracing::debug!(thread_id = %thread.id, "Re-injection skipped: quota or max reactivations reached");
                } else if let Err(e) = ai_smartness::intelligence::thread_manager::ThreadManager::reactivate_thread(conn, &thread.id) {
                    tracing::warn!(thread_id = %thread.id, error = %e, "Re-injection reactivation failed");
                } else {
                    reactivated += 1;
                    tracing::info!(thread_id = %thread.id, status = %thread.status.as_str(), "Re-injection reactivated thread");
                }
            }
            ThreadStatus::Active => {
                // Touch last_active to prevent decay
                let now = ai_smartness::time_utils::to_sqlite(&ai_smartness::time_utils::now());
                let _ = conn.execute(
                    "UPDATE threads SET last_active = ?1 WHERE id = ?2",
                    rusqlite::params![now, thread.id],
                );
            }
        }

        // Record injection for V5 InjectionHistory feedback loop
        if let Ok(Some(mut t)) = ThreadStorage::get(conn, &thread.id) {
            let stats = t.injection_stats.get_or_insert_with(InjectionStats::default);
            stats.record_injection();
            let _ = ThreadStorage::update(conn, &t);
        }
    }

    let mut ctx = String::from("AI Smartness Memory Context:\n");

    if let Some(main) = threads.first() {
        ctx.push_str(&format!("\nCurrent thread: \"{}\"\n", main.title));
        if let Some(ref summary) = main.summary {
            let s = &summary[..summary.len().min(200)];
            ctx.push_str(&format!("Summary: {}\n", s));
        }
        if !main.topics.is_empty() {
            let topics: Vec<&str> = main.topics.iter().take(5).map(|s| s.as_str()).collect();
            ctx.push_str(&format!("Topics: {}\n", topics.join(", ")));
        }
    }

    if threads.len() > 1 {
        ctx.push_str("\nRelated threads:\n");
        for thread in threads.iter().skip(1).take(3) {
            let title = &thread.title[..thread.title.len().min(50)];
            if let Some(ref summary) = thread.summary {
                let s = &summary[..summary.len().min(50)];
                ctx.push_str(&format!("- \"{}\" - {}\n", title, s));
            } else {
                ctx.push_str(&format!("- \"{}\"\n", title));
            }
        }
    }

    Some(ctx)
}

/// Layer 5: Agent identity — role, hierarchy, subordinates.
fn build_agent_identity(project_hash: &str, agent_id: &str) -> Option<(String, usize)> {
    let registry_db = path_utils::registry_db_path();
    if !registry_db.exists() {
        tracing::debug!("Layer 5: registry DB not found");
        return None;
    }
    let conn = open_connection(&registry_db, ConnectionRole::Hook).ok()?;
    migrations::migrate_registry_db(&conn).ok()?;

    let agent =
        ai_smartness::registry::registry::AgentRegistry::get(&conn, agent_id, project_hash)
            .ok()
            .flatten()?;

    let agent_quota = agent.thread_mode.quota();

    let mut ctx = format!("Agent Identity: {} (role: {})\n", agent.name, agent.role);

    if !agent.description.is_empty() {
        ctx.push_str(&format!("Description: {}\n", agent.description));
    }

    // Supervisor chain
    let chain = ai_smartness::registry::registry::AgentRegistry::get_supervisor_chain(
        &conn,
        agent_id,
        project_hash,
    )
    .unwrap_or_default();
    if !chain.is_empty() {
        let names: Vec<&str> = chain.iter().map(|a| a.name.as_str()).collect();
        ctx.push_str(&format!("Supervisor chain: {}\n", names.join(" -> ")));
    }

    // Subordinates
    let subs = ai_smartness::registry::registry::AgentRegistry::list_subordinates(
        &conn,
        agent_id,
        project_hash,
    )
    .unwrap_or_default();
    if !subs.is_empty() {
        let names: Vec<&str> = subs.iter().map(|a| a.name.as_str()).collect();
        ctx.push_str(&format!("Subordinates: {}\n", names.join(", ")));
    }

    // Topology
    if let Some(ref rt) = agent.report_to {
        ctx.push_str(&format!("Reports to: {}\n", rt));
    }
    if let Some(ref cr) = agent.custom_role {
        ctx.push_str(&format!("Custom role: {}\n", cr));
    }

    // Agent switching instructions
    ctx.push_str(
        "\nAgent switching: If the user asks to use a different agent \
         (e.g., \"I want agent cod\", \"switch to cod\", \"use the programmer agent\"), \
         call the ai_agent_select tool with the target agent_id AND the session_id \
         from your context above. The session_id ensures the switch only affects \
         this panel/session, not other open sessions. \
         Available agents can be listed with agent_list. \
         After switching, tell the user the change takes effect from their next message.\n",
    );

    Some((ctx, agent_quota))
}

/// Layer 0.5: Onboarding prompt — injected once at first session.
/// Creates a sentinelle file `onboarding_done` after injection.
fn build_onboarding_prompt(project_hash: &str, agent_id: &str) -> Option<String> {
    let data_dir = path_utils::agent_data_dir(project_hash, agent_id);
    let sentinel = data_dir.join("onboarding_done");

    if sentinel.exists() {
        return None;
    }

    // Create data dir if needed
    std::fs::create_dir_all(&data_dir).ok()?;

    let prompt = "\
AI Smartness Memory System — Quick Reference

You have access to MCP tools for managing your persistent memory across sessions:

Memory Management:
  - ai_status: Check daemon health, thread counts, and system status
  - ai_search <query>: Search through memory threads by keyword
  - ai_threads: List all active memory threads
  - ai_recall <query>: Active memory recall (finds relevant past context)

Content Organization:
  - ai_pin <content>: Pin important content for persistent injection
  - ai_merge <survivor_id> <absorbed_id>: Merge similar threads
  - ai_split <thread_id>: Split a thread that covers multiple topics
  - ai_label <thread_id> <labels>: Tag threads with labels for better retrieval
  - ai_unlock <thread_id>: Unlock a thread locked by the system

Inter-Agent Communication:
  - ai_inbox_send <to> <subject> <content>: Send message to another agent
  - ai_inbox_read: Check your cognitive inbox for pending messages

Your memory persists between sessions. The system automatically captures your work \
and organizes it into threads. Use ai_search or ai_recall to find past work context.

## Cognitive Autonomy Rules

You are responsible for maintaining your own memory. This is not optional.

### Thread Management
- When you encounter an important decision, pattern, or insight: call ai_thread_create
- When you notice threads with overlapping content: call ai_merge
- When a thread is no longer relevant: call ai_thread_suspend

### Context Enrichment
- At the START of complex tasks: call ai_recall with relevant keywords
- After completing a task: rate thread importance with ai_rate_importance
- Periodically: call ai_suggestions to check for maintenance tasks

### Labeling & Organization
- Always label threads you create (ai_label)
- Add concepts to threads missing them (ai_concepts)
- Use ai_focus when working on a specific topic for extended periods

### Communication
- When you discover something relevant for another agent: use ai_msg_focus
- When you finish a task delegated by another: reply via msg_reply
- Schedule follow-ups with beat_wake when needed

### When to create threads manually
The capture pipeline handles most content automatically, but you should manually \
create threads (ai_thread_create) when:
- A user makes an architectural decision
- You discover a bug pattern or anti-pattern
- A conversation produces insights not captured by tool outputs
- You receive requirements or constraints that should persist";

    // Mark onboarding as done
    std::fs::write(&sentinel, chrono::Utc::now().to_rfc3339()).ok();
    tracing::info!("Onboarding sentinelle created at {}", sentinel.display());

    Some(prompt.to_string())
}

/// Layer 1.5: Session state — continuity context based on beat distance.
fn build_session_context(session: &SessionState, beat: &BeatState) -> Option<String> {
    let since = beat.since_last();
    let duration = session.duration_minutes();

    // Only inject if there's meaningful session history
    if session.prompt_count <= 1 && session.files_modified.is_empty() {
        return None;
    }

    let mut ctx = String::new();

    // Session resume message based on beat distance
    if since < 2 {
        // < 10 min — just returned
        ctx.push_str("Session continuity: You were just here.\n");
    } else if since < 6 {
        // < 30 min — brief pause
        ctx.push_str("Session continuity: Brief pause since last interaction.\n");
        if let Some(ref msg) = session.current_work.last_user_message {
            ctx.push_str(&format!("Last request: {}\n", msg));
        }
    } else if since < 12 {
        // < 60 min — moderate absence
        if let Some(real_time) = beat.time_since_last() {
            ctx.push_str(&format!(
                "Session resuming (you left about {} min ago).\n",
                real_time.num_minutes()
            ));
        } else {
            ctx.push_str("Session resuming after a break.\n");
        }
        if let Some(ref msg) = session.current_work.last_user_message {
            ctx.push_str(&format!("Last request: {}\n", msg));
        }
        if let Some(ref action) = session.current_work.last_agent_action {
            ctx.push_str(&format!("Last action: {}\n", action));
        }
    } else {
        // > 60 min — new session context
        ctx.push_str("New session (long absence).\n");
    }

    // Files modified (last 5)
    if !session.files_modified.is_empty() {
        ctx.push_str("Recent files: ");
        let recent: Vec<String> = session
            .files_modified
            .iter()
            .rev()
            .take(5)
            .map(|f| format!("{} ({})", f.path, f.action))
            .collect();
        ctx.push_str(&recent.join(", "));
        ctx.push('\n');
    }

    // Session stats
    ctx.push_str(&format!(
        "Session: {} prompts, {} min\n",
        session.prompt_count, duration
    ));

    Some(ctx)
}

/// Layer 1.7: Cognitive nudge — conditional maintenance reminder.
/// Returns (nudge_type, formatted_message) so the caller can update beat_state.
/// Design: max 1 nudge per prompt, cooldown 10 beats per type, 300 chars max.
fn build_cognitive_nudge(
    conn: &Connection,
    beat_state: &BeatState,
    agent_quota: usize,
) -> Option<(String, String)> {
    let beat = beat_state.beat;
    let cooldown = 10u64;
    let active = ThreadStorage::count(conn).unwrap_or(0);

    // Priority-ordered conditions — first match wins
    // 1. Recall staleness
    if beat_state.last_recall_beat + 10 < beat
        && active > 10
        && (beat_state.last_nudge_type != "recall"
            || beat_state.last_nudge_beat + cooldown <= beat)
    {
        let msg = format!(
            "You haven't used ai_recall in {} prompts and have {} active threads. Search memory for relevant context.",
            beat - beat_state.last_recall_beat, active
        );
        return Some(("recall".into(), msg));
    }

    // 2. Capacity warning (80% of quota)
    let quota = agent_quota;
    if quota > 0
        && active as f64 / quota as f64 > 0.80
        && (beat_state.last_nudge_type != "capacity"
            || beat_state.last_nudge_beat + cooldown <= beat)
    {
        let msg = format!(
            "You have {} active threads ({:.0}% of {} quota). Review and suspend obsolete ones with ai_thread_suspend.",
            active, active as f64 / quota as f64 * 100.0, quota
        );
        return Some(("capacity".into(), msg));
    }

    // 3. Unlabeled ratio
    if active > 5 {
        let unlabeled = ThreadStorage::count_unlabeled(conn).unwrap_or(0);
        let ratio = unlabeled as f64 / active as f64;
        if ratio > 0.4
            && (beat_state.last_nudge_type != "unlabeled"
                || beat_state.last_nudge_beat + cooldown <= beat)
        {
            let msg = format!(
                "You have {} unlabeled threads ({:.0}%). Consider running ai_label on important ones.",
                unlabeled, ratio * 100.0
            );
            return Some(("unlabeled".into(), msg));
        }
    }

    // 4. General maintenance
    if beat > beat_state.last_maintenance_beat + 50
        && (beat_state.last_nudge_type != "maintenance"
            || beat_state.last_nudge_beat + cooldown <= beat)
    {
        return Some((
            "maintenance".into(),
            "Run ai_suggestions and address any findings. Check threads missing labels or concepts.".into(),
        ));
    }

    None
}

/// Layer 6: HealthGuard — proactive memory maintenance injection.
///
/// Runs health analysis (capacity, fragmentation, merge candidates, etc.).
/// Only High/Critical findings are injected (imposed on agent).
/// Respects cooldown (default 30 min between injections).
fn build_healthguard_injection(
    conn: &Connection,
    agent_data_dir: &Path,
    project_hash: &str,
    beat_state: &BeatState,
    agent_quota: usize,
) -> Option<String> {
    let _ = project_hash; // available for future per-project config

    // Load guardian config for thresholds
    let cfg_path = path_utils::data_dir().join("config.json");
    let guardian = std::fs::read_to_string(&cfg_path)
        .ok()
        .and_then(|s| serde_json::from_str::<GuardianConfig>(&s).ok())
        .unwrap_or_default();

    let hg = HealthGuard::new(guardian.healthguard.clone());

    let quota_override = if agent_quota > 0 { Some(agent_quota) } else { None };
    let findings = hg.analyze(conn, agent_data_dir, &guardian.gossip, quota_override)?;

    // High/Critical: always injected
    // Medium: injected every 10 beats (beat % 10 == 0)
    let beat = beat_state.beat;
    let (high_critical, medium, _low) =
        healthguard::HealthGuard::partition_findings_by_priority(&findings);

    let mut injectable: Vec<&healthguard::HealthFinding> = high_critical;
    if beat % 10 == 0 {
        injectable.extend(medium);
    }

    if injectable.is_empty() {
        return None;
    }

    let owned: Vec<healthguard::HealthFinding> = injectable.into_iter().cloned().collect();
    Some(healthguard::formatter::format_injection(&owned))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ai_smartness::healthguard::{HealthFinding, HealthGuard, HealthPriority};
    use ai_smartness::storage::beat::BeatState;

    fn setup_agent_db() -> Connection {
        let conn = Connection::open(":memory:").unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .unwrap();
        ai_smartness::storage::migrations::migrate_agent_db(&conn).unwrap();
        conn
    }

    fn cleanup_project(ph: &str) {
        let dir = ai_smartness::storage::path_utils::project_dir(ph);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Insert threads with optional labels. Uses raw SQL for speed.
    fn insert_threads(conn: &Connection, prefix: &str, count: usize, labeled: bool) {
        let now = ai_smartness::time_utils::to_sqlite(&ai_smartness::time_utils::now());
        let labels = if labeled { r#"["test"]"# } else { "[]" };
        for i in 0..count {
            conn.execute(
                "INSERT INTO threads (id, title, status, summary, origin_type, parent_id, child_ids,
                    weight, importance, importance_manually_set, relevance_score,
                    activation_count, split_locked, split_locked_until,
                    topics, tags, labels, concepts, drift_history,
                    work_context, ratings, injection_stats, embedding,
                    created_at, last_active)
                 VALUES (?1, ?2, 'active', NULL, 'prompt', NULL, '[]',
                    0.5, 0.5, 0, 1.0, 1, 0, NULL,
                    '[]', '[]', ?3, '[]', '[]', NULL, '[]', NULL, NULL,
                    ?4, ?4)",
                rusqlite::params![format!("{}-{}", prefix, i), format!("Thread {}", i), labels, now],
            ).unwrap();
        }
    }

    // === T-P1: Onboarding ===

    #[test]
    fn test_onboarding_creates_sentinel() {
        let ph = "test_ph_ob1";
        let ag = "ag_ob1";
        cleanup_project(ph);

        let result = build_onboarding_prompt(ph, ag);
        assert!(result.is_some(), "First call should return prompt");

        let sentinel = ai_smartness::storage::path_utils::agent_data_dir(ph, ag)
            .join("onboarding_done");
        assert!(sentinel.exists(), "Sentinel file should be created");

        cleanup_project(ph);
    }

    #[test]
    fn test_onboarding_not_repeated() {
        let ph = "test_ph_ob2";
        let ag = "ag_ob2";
        cleanup_project(ph);

        assert!(build_onboarding_prompt(ph, ag).is_some());
        assert!(
            build_onboarding_prompt(ph, ag).is_none(),
            "Second call should return None"
        );

        cleanup_project(ph);
    }

    #[test]
    fn test_onboarding_contains_cognitive_rules() {
        let ph = "test_ph_ob3";
        let ag = "ag_ob3";
        cleanup_project(ph);

        let prompt = build_onboarding_prompt(ph, ag).unwrap();
        assert!(prompt.contains("Cognitive Autonomy Rules"));
        assert!(prompt.contains("Thread Management"));
        assert!(prompt.contains("Context Enrichment"));
        assert!(prompt.contains("Labeling"));
        assert!(prompt.contains("Communication"));

        cleanup_project(ph);
    }

    // === T-P2: Cognitive nudge ===

    #[test]
    fn test_nudge_recall_staleness() {
        let conn = setup_agent_db();
        insert_threads(&conn, "t", 12, false);

        let bs = BeatState {
            beat: 15,
            last_recall_beat: 0,
            ..Default::default()
        };

        let result = build_cognitive_nudge(&conn, &bs, bs.quota);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, "recall");
    }

    #[test]
    fn test_nudge_capacity_dynamic_threshold() {
        let conn = setup_agent_db();
        insert_threads(&conn, "t", 85, true); // all labeled

        let bs = BeatState {
            beat: 20,
            last_recall_beat: 15, // 15+10=25 > 20 → no recall
            quota: 100,
            ..Default::default()
        };

        let result = build_cognitive_nudge(&conn, &bs, bs.quota);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, "capacity");

        // 30 labeled threads → 30/100 = 30% < 80% → no capacity
        let conn2 = setup_agent_db();
        insert_threads(&conn2, "t", 30, true);
        let result2 = build_cognitive_nudge(&conn2, &bs, bs.quota);
        assert!(result2.is_none(), "30/100 should not trigger any nudge");
    }

    #[test]
    fn test_nudge_unlabeled_ratio() {
        let conn = setup_agent_db();
        insert_threads(&conn, "lab", 5, true);  // labeled
        insert_threads(&conn, "unl", 5, false); // unlabeled

        let bs = BeatState {
            beat: 20,
            last_recall_beat: 15,
            quota: 100,
            ..Default::default()
        };

        // 10 total, 5 unlabeled active → 5/10 = 50% > 40%
        let result = build_cognitive_nudge(&conn, &bs, bs.quota);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, "unlabeled");
    }

    #[test]
    fn test_nudge_maintenance_after_50_beats() {
        let conn = setup_agent_db();
        insert_threads(&conn, "t", 3, true); // few threads, all labeled

        let bs = BeatState {
            beat: 55,
            last_recall_beat: 50,
            last_maintenance_beat: 0,
            quota: 100,
            ..Default::default()
        };

        let result = build_cognitive_nudge(&conn, &bs, bs.quota);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, "maintenance");
    }

    #[test]
    fn test_nudge_cooldown_prevents_repeat() {
        let conn = setup_agent_db();
        insert_threads(&conn, "t", 12, false);

        let bs = BeatState {
            beat: 15,
            last_recall_beat: 0,
            last_nudge_type: "recall".to_string(),
            last_nudge_beat: 10, // 10+10=20 > 15 → recall in cooldown
            quota: 100,
            ..Default::default()
        };

        // Recall in cooldown → skips to unlabeled (12/12 = 100% > 40%)
        let result = build_cognitive_nudge(&conn, &bs, bs.quota);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, "unlabeled");
    }

    #[test]
    fn test_nudge_only_one_per_prompt() {
        let conn = setup_agent_db();
        insert_threads(&conn, "t", 85, false);

        let bs = BeatState {
            beat: 15,
            last_recall_beat: 0,
            quota: 100,
            ..Default::default()
        };

        // Both recall and capacity could trigger — recall has priority
        let result = build_cognitive_nudge(&conn, &bs, bs.quota);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, "recall");
    }

    // === T-P3: Medium findings injection ===

    fn make_finding(priority: HealthPriority) -> HealthFinding {
        HealthFinding {
            priority,
            category: "test".into(),
            message: "test".into(),
            action: "test".into(),
            metric_value: 0.0,
            threshold: 0.0,
        }
    }

    #[test]
    fn test_medium_findings_injected_every_10_beats() {
        let findings = vec![
            make_finding(HealthPriority::Medium),
            make_finding(HealthPriority::High),
        ];
        let (hc, med, _) = HealthGuard::partition_findings_by_priority(&findings);

        // Beat 10: medium included
        let mut inj: Vec<&HealthFinding> = hc.clone();
        if 10u64 % 10 == 0 { inj.extend(&med); }
        assert_eq!(inj.len(), 2, "Beat 10: both high and medium");

        // Beat 11: only high/critical
        let mut inj2: Vec<&HealthFinding> = hc;
        if 11u64 % 10 == 0 { inj2.extend(&med); }
        assert_eq!(inj2.len(), 1, "Beat 11: only high/critical");
    }

    #[test]
    fn test_high_critical_always_injected() {
        let findings = vec![
            make_finding(HealthPriority::High),
            make_finding(HealthPriority::Critical),
            make_finding(HealthPriority::Medium),
            make_finding(HealthPriority::Low),
        ];
        let (hc, med, _) = HealthGuard::partition_findings_by_priority(&findings);

        // Beat 7 (not divisible by 10): high/critical still included
        let mut inj: Vec<&HealthFinding> = hc;
        if 7u64 % 10 == 0 { inj.extend(&med); }
        assert_eq!(inj.len(), 2, "High+Critical always injected");
    }
}
