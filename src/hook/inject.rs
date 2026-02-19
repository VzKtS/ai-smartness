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

use std::path::Path;

use ai_smartness::constants::{MAX_COGNITIVE_MESSAGES, MAX_CONTEXT_SIZE};
use ai_smartness::thread::ThreadStatus;
use ai_smartness::intelligence::memory_retriever::MemoryRetriever;
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

    // Load user profile, auto-detect traits and rules from message
    let mut profile = UserProfile::load(&agent_data);
    profile.detect_from_message(&message);
    if let Some(rule) = profile.detect_rules(&message) {
        tracing::info!(rule = %rule, "Auto-detected user rule");
    }
    profile.save(&agent_data);

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

    // Layer 2: Cognitive inbox (always inject when pending)
    match build_cognitive_inbox(&conn, agent_id) {
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
    match build_memory_context(&conn, &message) {
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
    match build_agent_identity(project_hash, agent_id) {
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
            let _ = budget;
            injections.push(layer);
            tracing::debug!("Layer 5.5: user profile injected");
        } else {
            tracing::debug!("Layer 5.5: exceeds budget, skipped");
        }
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
fn build_cognitive_inbox(conn: &Connection, agent_id: &str) -> Option<String> {
    let messages = CognitiveInbox::read_pending(conn, agent_id).ok()?;
    if messages.is_empty() {
        return None;
    }

    let mut ctx = String::from("You have pending cognitive messages:\n");
    for msg in messages.iter().take(MAX_COGNITIVE_MESSAGES) {
        ctx.push_str(&format!(
            "- From {}: [{}] {}\n",
            msg.from_agent, msg.subject, msg.content
        ));
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
                let s = &summary[..summary.len().min(100)];
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

/// Layer 4: Memory retrieval — find similar threads to the user's prompt.
fn build_memory_context(conn: &Connection, message: &str) -> Option<String> {
    let threads = MemoryRetriever::recall(conn, message).ok()?;
    if threads.is_empty() {
        return None;
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
fn build_agent_identity(project_hash: &str, agent_id: &str) -> Option<String> {
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

    Some(ctx)
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
and organizes it into threads. Use ai_search or ai_recall to find past work context.";

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
