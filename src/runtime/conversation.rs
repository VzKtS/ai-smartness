//! Conversation management — builds messages[] per turn from engram context.
//!
//! Each turn starts with a fresh messages array:
//!   1. Engram-selected threads as context (pushed into messages[0] as user context)
//!   2. User prompt (messages[1])
//!
//! No transcript accumulation. Engram decides what the agent sees.

use crate::mcp::tools::ToolContext;
use ai_smartness::config::EngramConfig;
use ai_smartness::intelligence::engram_retriever::{EngramRetriever, ScoredThread};
use ai_smartness::processing::daemon_ipc_client;
use ai_smartness::registry::registry::AgentRegistry;
use ai_smartness::storage::cognitive_inbox::CognitiveInbox;
use ai_smartness::storage::threads::ThreadStorage;
use ai_smartness::thread::ThreadStatus;
use ai_smartness::user_profile::UserProfile;
use ai_smartness::storage::path_utils;

/// Build the system prompt from agent profile + rules.
///
/// This is the persistent identity of the agent — stays constant across turns.
pub fn build_system_prompt(ctx: &ToolContext) -> String {
    let agent_data = path_utils::agent_data_dir(ctx.project_hash, ctx.agent_id);
    let profile = UserProfile::load(&agent_data);

    let mut parts = Vec::new();

    // Agent identity
    let role = get_agent_role(ctx);
    let name = profile.identity.name.as_deref().unwrap_or(ctx.agent_id);
    parts.push(format!(
        "You are {}, a {} agent powered by ai-smartness cognitive memory.",
        name, role
    ));

    // Language preference
    parts.push(format!(
        "Communication language: {}. Technical level: {:?}.",
        profile.preferences.language, profile.preferences.technical_level
    ));

    // Golden rules (non-negotiable)
    parts.push(String::new());
    parts.push("Rules:".to_string());
    parts.push("- All memory operations MUST go through the provided tools (ai_recall, ai_thread_create, etc.)".to_string());
    parts.push("- Be proactive with your memory tools. Don't wait to be asked.".to_string());
    parts.push("- Do LITERALLY what the user asks.".to_string());

    // User-defined rules
    for rule in &profile.context_rules {
        parts.push(format!("- {}", rule));
    }

    // Context about the runtime
    parts.push(String::new());
    parts.push(format!(
        "ai-smartness runtime v{} | agent: {} | project: {}",
        env!("CARGO_PKG_VERSION"),
        ctx.agent_id,
        ctx.project_hash,
    ));
    parts.push(
        "Your context is rebuilt each turn from engram (semantic memory retrieval). \
         You have no persistent conversation history — use your memory tools to persist \
         important information across turns."
            .to_string(),
    );

    parts.join("\n")
}

/// Build the messages array for a single turn.
///
/// Structure:
///   messages[0]: user role — engram context block (if threads found)
///   messages[1]: assistant role — acknowledgment of context
///   messages[2]: user role — actual user prompt
///
/// When no engram context, just:
///   messages[0]: user role — user prompt
pub fn build_turn_messages(ctx: &ToolContext, user_prompt: &str) -> Vec<serde_json::Value> {
    let mut messages = Vec::new();

    // Query engram for relevant threads based on user prompt
    let engram_context = build_engram_context(ctx, user_prompt);

    if !engram_context.is_empty() {
        // Inject engram threads as context
        messages.push(serde_json::json!({
            "role": "user",
            "content": format!(
                "<memory-context>\n{}</memory-context>\n\n\
                 The above is your cognitive memory context, retrieved by engram \
                 based on semantic relevance to the current prompt. Use ai_recall \
                 for deeper search if needed.",
                engram_context
            ),
        }));

        messages.push(serde_json::json!({
            "role": "assistant",
            "content": "I've integrated the memory context. Ready to proceed.",
        }));
    }

    // Pending messages / alerts
    let alerts = build_alerts(ctx);
    let prompt_with_alerts = if !alerts.is_empty() {
        format!("{}\n\n{}", alerts, user_prompt)
    } else {
        user_prompt.to_string()
    };

    // Actual user prompt
    messages.push(serde_json::json!({
        "role": "user",
        "content": prompt_with_alerts,
    }));

    messages
}

/// Query engram and format matching threads as context block.
fn build_engram_context(ctx: &ToolContext, query: &str) -> String {
    // Ensure ONNX runtime path is set
    crate::hook::ensure_ort_dylib_path();

    let config = EngramConfig::default();
    let retriever = match EngramRetriever::new(ctx.agent_conn, config) {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!(error = %e, "Engram retriever init failed, using fallback");
            return build_fallback_context(ctx);
        }
    };

    // Search with user prompt as query (get_relevant_context returns ScoredThread)
    let scored: Vec<ScoredThread> = match retriever.get_relevant_context(ctx.agent_conn, query, 8) {
        Ok(s) if !s.is_empty() => s,
        _ => return build_fallback_context(ctx),
    };

    format_scored_threads(&scored)
}

/// Format scored threads into a readable context block.
fn format_scored_threads(scored: &[ScoredThread]) -> String {
    let mut parts = Vec::new();

    for (i, st) in scored.iter().enumerate() {
        let t = &st.thread;
        let summary = t.summary.as_deref().unwrap_or("(no summary)");
        let topics = t.topics.join(", ");
        let labels = t.labels.join(", ");

        // Degressive detail: first 3 get full summary, rest get title only
        if i < 3 {
            let mut entry = format!(
                "## {} [w={:.2} pass={}/10]\n",
                t.title, t.weight, st.pass_count
            );
            if !topics.is_empty() {
                entry.push_str(&format!("Topics: {}\n", topics));
            }
            if !labels.is_empty() {
                entry.push_str(&format!("Labels: {}\n", labels));
            }
            // Truncate summary for context budget
            let max_summary = if i == 0 { 500 } else { 250 };
            let truncated = if summary.len() > max_summary {
                let byte_end: usize = summary
                    .chars()
                    .take(max_summary)
                    .map(|c| c.len_utf8())
                    .sum();
                format!("{}...", &summary[..byte_end])
            } else {
                summary.to_string()
            };
            entry.push_str(&truncated);
            parts.push(entry);
        } else {
            parts.push(format!(
                "- {} [w={:.2}] {}",
                t.title,
                t.weight,
                if !topics.is_empty() {
                    format!("({})", topics)
                } else {
                    String::new()
                }
            ));
        }
    }

    parts.join("\n\n")
}

/// Fallback context when engram is unavailable: top threads by weight.
fn build_fallback_context(ctx: &ToolContext) -> String {
    let threads = ThreadStorage::list_all(ctx.agent_conn)
        .unwrap_or_default()
        .into_iter()
        .filter(|t| {
            t.status == ThreadStatus::Active
                && !t.tags.contains(&"__pin__".to_string())
                && !t.tags.contains(&"__focus__".to_string())
        })
        .take(5)
        .collect::<Vec<_>>();

    if threads.is_empty() {
        return String::new();
    }

    let mut parts = Vec::new();
    for t in &threads {
        let summary = t.summary.as_deref().unwrap_or("(no summary)");
        let truncated = if summary.len() > 200 {
            let end = summary.char_indices()
                .take_while(|&(i, _)| i <= 200)
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
            format!("{}...", &summary[..end])
        } else {
            summary.to_string()
        };
        parts.push(format!("## {} [w={:.2}]\n{}", t.title, t.weight, truncated));
    }

    parts.join("\n\n")
}

/// Build alerts string (pending tasks, messages).
fn build_alerts(ctx: &ToolContext) -> String {
    let mut alerts = Vec::new();

    // Cognitive inbox
    let cog_msgs = CognitiveInbox::peek_pending(ctx.agent_conn, ctx.agent_id).unwrap_or_default();
    if !cog_msgs.is_empty() {
        alerts.push(format!("[{} pending messages — use ai_msg_ack after processing]", cog_msgs.len()));
        for m in cog_msgs.iter().take(3) {
            alerts.push(format!("  From {}: \"{}\"", m.from_agent, m.content));
        }
    }

    alerts.join("\n")
}

/// Capture the agent's response via daemon IPC for thread extraction.
pub fn capture_response(project_hash: &str, agent_id: &str, prompt: &str, response: &str) {
    // Send prompt for capture continuity
    match daemon_ipc_client::send_prompt_capture(project_hash, agent_id, prompt, None) {
        Ok(_) => {}
        Err(e) => tracing::debug!(error = %e, "Prompt capture failed"),
    }

    // Send response as a "response" source_type capture
    match daemon_ipc_client::send_capture(project_hash, agent_id, "response", response, None) {
        Ok(_) => tracing::info!(response_len = response.len(), "Response captured"),
        Err(e) => tracing::warn!(error = %e, "Response capture failed"),
    }
}

/// Lookup agent role from registry.
fn get_agent_role(ctx: &ToolContext) -> String {
    AgentRegistry::get(ctx.registry_conn, ctx.agent_id, ctx.project_hash)
        .ok()
        .flatten()
        .map(|a| a.role)
        .unwrap_or_else(|| "agent".to_string())
}
