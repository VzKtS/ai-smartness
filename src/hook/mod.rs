pub mod capture;
pub mod compact;
pub mod health;
pub mod inject;
pub mod pretool;
pub mod providers;
pub mod setup;
pub mod virtual_paths;

use std::io::Read;

/// Hook subcommands.
pub enum HookAction {
    Inject { project_hash: String, agent_id: Option<String> },
    Capture { project_hash: String, agent_id: Option<String> },
    Health { project_hash: String, agent_id: Option<String> },
    PreTool { project_hash: String, agent_id: Option<String> },
}

/// Run hook action. CRITICAL: Always exits 0, even on panic.
/// Exception: PreTool may exit 2 to block a tool call.
pub fn run(action: HookAction) {
    let guard_env_name = std::env::var("AI_SMARTNESS_GUARD_ENV")
        .unwrap_or_else(|_| "AI_SMARTNESS_HOOK_RUNNING".to_string());

    let result = std::panic::catch_unwind(|| {
        // Anti-hook guard: prevent infinite hook -> tool -> hook loops
        let guard_env = std::env::var("AI_SMARTNESS_GUARD_ENV")
            .unwrap_or_else(|_| "AI_SMARTNESS_HOOK_RUNNING".to_string());
        if std::env::var(&guard_env).is_ok() {
            passthrough_stdin();
            return;
        }

        // SAFETY: single-threaded at this point
        #[allow(unused_unsafe)]
        unsafe {
            std::env::set_var(&guard_env, "1");
        }

        // Initialize tracing to daemon.log (append mode) so hook events
        // appear in the Debug Console alongside daemon events.
        let (project_hash_for_tracing, action_name) = match &action {
            HookAction::Inject { project_hash, .. } => (project_hash.clone(), "inject"),
            HookAction::Capture { project_hash, .. } => (project_hash.clone(), "capture"),
            HookAction::Health { project_hash, .. } => (project_hash.clone(), "health"),
            HookAction::PreTool { project_hash, .. } => (project_hash.clone(), "pretool"),
        };
        ai_smartness::tracing_init::init_file_tracing(&project_hash_for_tracing);

        tracing::info!(
            action = action_name,
            project = %project_hash_for_tracing,
            pid = std::process::id(),
            "Hook process started"
        );

        // Read stdin for hooks that need it (all except Health)
        let (input, session_id) = match &action {
            HookAction::Health { .. } => (String::new(), None),
            _ => {
                let raw = read_stdin();
                let sid = extract_session_id(&raw);
                tracing::debug!(
                    session_id = ?sid,
                    input_len = raw.len(),
                    "Extracted session context from stdin"
                );
                (raw, sid)
            }
        };

        // Resolve agent_id: explicit → env var → per-session file → global session → registry
        let resolve_agent = |explicit: &Option<String>, project_hash: &str| -> String {
            tracing::debug!(explicit_agent = ?explicit, "Agent resolution starting");

            // 0. Explicit CLI arg (if provided)
            if let Some(ref id) = explicit {
                if !id.is_empty() {
                    tracing::info!(resolved = %id, source = "cli_arg", "Agent resolved");
                    return id.clone();
                }
            }
            // 1. Env var has highest priority
            if let Ok(env_agent) = std::env::var("AI_SMARTNESS_AGENT") {
                if !env_agent.is_empty() {
                    tracing::info!(resolved = %env_agent, source = "env_var", "Agent resolved");
                    return env_agent;
                }
            }
            // 2. Per-session file (keyed by Claude Code session_id — isolates multi-panel)
            if let Some(ref sid) = session_id {
                let per_session_path =
                    ai_smartness::storage::path_utils::per_session_agent_path(project_hash, sid);
                tracing::debug!(path = %per_session_path.display(), session_id = %sid, "Checking per-session agent file");
                if let Ok(contents) = std::fs::read_to_string(&per_session_path) {
                    let trimmed = contents.trim().to_string();
                    if !trimmed.is_empty() {
                        tracing::info!(resolved = %trimmed, source = "per_session", session_id = %sid, "Agent resolved");
                        return trimmed;
                    }
                }
            }
            // 3. Global session file (set via `ai-smartness agent select` CLI)
            let session_path = ai_smartness::storage::path_utils::agent_session_path(project_hash);
            tracing::debug!(path = %session_path.display(), "Checking global session file");
            if let Ok(contents) = std::fs::read_to_string(&session_path) {
                let trimmed = contents.trim().to_string();
                if !trimmed.is_empty() {
                    tracing::info!(resolved = %trimmed, source = "session_file", "Agent resolved");
                    return trimmed;
                }
            }
            // 4. Recent wake signal: if a signal was just acknowledged, use its agent_id
            let signals_dir = ai_smartness::storage::path_utils::wake_signals_dir();
            if let Ok(entries) = std::fs::read_dir(&signals_dir) {
                let now = std::time::SystemTime::now();
                for entry in entries.flatten() {
                    if let Ok(contents) = std::fs::read_to_string(entry.path()) {
                        if let Ok(sig) = serde_json::from_str::<serde_json::Value>(&contents) {
                            let is_acked = sig.get("acknowledged").and_then(|v| v.as_bool()).unwrap_or(false);
                            let agent = sig.get("agent_id").and_then(|v| v.as_str()).unwrap_or("");
                            if is_acked && !agent.is_empty() {
                                // Check if acknowledged recently (within 15 seconds)
                                if let Ok(meta) = entry.path().metadata() {
                                    if let Ok(modified) = meta.modified() {
                                        if let Ok(elapsed) = now.duration_since(modified) {
                                            if elapsed.as_secs() < 15 {
                                                tracing::info!(resolved = agent, source = "wake_signal", "Agent resolved");
                                                return agent.to_string();
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // 5. Registry-based resolution: single-agent shortcut or first-registered fallback
            if let Ok(reg_conn) = ai_smartness::storage::database::open_connection(
                &ai_smartness::storage::path_utils::registry_db_path(),
                ai_smartness::storage::database::ConnectionRole::Hook,
            ) {
                let _ = ai_smartness::storage::migrations::migrate_registry_db(&reg_conn);
                if let Ok(agents) = ai_smartness::registry::registry::AgentRegistry::list(
                    &reg_conn, Some(project_hash), None, None,
                ) {
                    if agents.len() == 1 {
                        tracing::info!(resolved = %agents[0].id, source = "single_agent_shortcut", "Agent resolved");
                        return agents[0].id.clone();
                    }
                    if let Some(first) = agents.first() {
                        tracing::info!(resolved = %first.id, source = "first_registered", "Agent resolved (fallback to first registered)");
                        return first.id.clone();
                    }
                }
            }
            // 6. Last resort — no agents registered at all. Use project hash as identifier.
            tracing::warn!(source = "no_agents", "No agents registered for this project");
            format!("anon-{}", &project_hash[..8.min(project_hash.len())])
        };

        match &action {
            HookAction::Inject { project_hash, agent_id } => {
                let agent = resolve_agent(agent_id, project_hash);
                tracing::info!(agent = %agent, "Dispatching inject");
                inject::run(project_hash, &agent, &input, session_id.as_deref());
            }
            HookAction::Capture { project_hash, agent_id } => {
                let agent = resolve_agent(agent_id, project_hash);
                tracing::info!(agent = %agent, "Dispatching capture");
                capture::run(project_hash, &agent, &input);
            }
            HookAction::Health { project_hash, agent_id } => {
                let agent = resolve_agent(agent_id, project_hash);
                tracing::info!(agent = %agent, "Dispatching health");
                let status = health::check_and_heal(project_hash, &agent);
                if let Ok(json) = serde_json::to_string(&status) {
                    println!("{}", json);
                }
            }
            HookAction::PreTool { project_hash, agent_id } => {
                let agent = resolve_agent(agent_id, project_hash);
                tracing::info!(agent = %agent, "Dispatching pretool");
                pretool::run(project_hash, &agent, &input);
            }
        }

        // SAFETY: single-threaded cleanup
        #[allow(unused_unsafe)]
        unsafe {
            std::env::remove_var(&guard_env);
        }
    });

    if let Err(e) = result {
        eprintln!("[ai-hook] panic: {:?}", e);
        #[allow(unused_unsafe)]
        unsafe {
            std::env::remove_var(&guard_env_name);
        }
    }

    // ALWAYS exit 0 — hooks must never break the AI session
    std::process::exit(0);
}

/// Read stdin fully.
fn read_stdin() -> String {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).ok();
    input
}

/// Extract session_id from Claude Code hook stdin JSON.
/// Claude Code sends { "session_id": "...", ... } for UserPromptSubmit, PostToolUse, PreToolUse.
fn extract_session_id(input: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(input)
        .ok()?
        .get("session_id")?
        .as_str()
        .map(|s| s.to_string())
}

/// Pass stdin to stdout unchanged (for guard bypass).
fn passthrough_stdin() {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).ok();
    if !input.is_empty() {
        print!("{}", input);
    }
}
