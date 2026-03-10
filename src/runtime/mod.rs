//! Custom Agent Runtime — direct Anthropic Messages API integration.
//!
//! Replaces Claude Code as the conversational layer, enabling push-not-pull
//! memory architecture. Each turn builds a fresh messages[] array from:
//!   1. System prompt (agent identity + rules)
//!   2. Engram-selected thread context (pushed, not pulled)
//!   3. User prompt
//!   4. Tool use/result pairs (agentic loop)
//!
//! The runtime controls the context window directly — no transcript
//! accumulation, no compaction. Engram decides what the agent remembers.

pub mod anthropic_client;
pub mod conversation;
pub mod tool_executor;

use crate::mcp::tools::ToolContext;
use ai_smartness::storage::database::{self, ConnectionRole};
use ai_smartness::storage::migrations;
use ai_smartness::storage::path_utils;

use std::io::{self, BufRead, Write};

/// Run the interactive runtime loop.
pub fn run(project_hash: &str, agent_id: &str) {
    ai_smartness::tracing_init::init_file_tracing(project_hash);
    tracing::info!(project = project_hash, agent = agent_id, "Runtime starting");

    // Resolve API key
    let api_key = match std::env::var("ANTHROPIC_API_KEY") {
        Ok(k) if !k.is_empty() => k,
        _ => {
            eprintln!("[runtime] Error: ANTHROPIC_API_KEY not set");
            std::process::exit(1);
        }
    };

    // Open database connections (same as MCP server)
    let agent_db = path_utils::agent_db_path(project_hash, agent_id);
    let registry_db = path_utils::registry_db_path();
    let shared_db = path_utils::shared_db_path(project_hash);

    let agent_conn = database::open_connection(&agent_db, ConnectionRole::Mcp)
        .unwrap_or_else(|e| { eprintln!("[runtime] DB error: {}", e); std::process::exit(1); });
    let registry_conn = database::open_connection(&registry_db, ConnectionRole::Mcp)
        .unwrap_or_else(|e| { eprintln!("[runtime] DB error: {}", e); std::process::exit(1); });
    let shared_conn = database::open_connection(&shared_db, ConnectionRole::Mcp)
        .unwrap_or_else(|e| { eprintln!("[runtime] DB error: {}", e); std::process::exit(1); });

    migrations::migrate_agent_db(&agent_conn).ok();
    migrations::migrate_registry_db(&registry_conn).ok();
    migrations::migrate_shared_db(&shared_conn).ok();

    let ctx = ToolContext {
        agent_conn: &agent_conn,
        registry_conn: &registry_conn,
        shared_conn: &shared_conn,
        project_hash,
        agent_id,
    };

    // Build system prompt from agent profile
    let system_prompt = conversation::build_system_prompt(&ctx);

    // Build tool definitions in Anthropic format
    let tools = tool_executor::anthropic_tool_definitions();

    // Model selection (configurable later)
    let model = std::env::var("AI_SMARTNESS_RUNTIME_MODEL")
        .unwrap_or_else(|_| "claude-sonnet-4-20250514".to_string());

    println!("ai-smartness runtime v{}", env!("CARGO_PKG_VERSION"));
    println!("Agent: {} | Project: {} | Model: {}", agent_id, project_hash, model);
    println!("Type your prompt (Ctrl+D to exit)\n");

    let stdin = io::stdin();
    let mut turn_number: u32 = 0;

    loop {
        // Prompt indicator
        print!(">>> ");
        io::stdout().flush().ok();

        // Read user input (multi-line: read until empty line or EOF)
        let user_prompt = match read_user_input(&stdin) {
            Some(p) if !p.is_empty() => p,
            _ => break, // EOF or empty → exit
        };

        turn_number += 1;
        tracing::info!(turn = turn_number, prompt_len = user_prompt.len(), "New turn");

        // Build messages for this turn (engram context + user prompt)
        let messages = conversation::build_turn_messages(&ctx, &user_prompt);

        // Agentic loop: send to API, handle tool_use, repeat until end_turn
        let response = match anthropic_client::agentic_loop(
            &api_key,
            &model,
            &system_prompt,
            messages,
            &tools,
            &ctx,
        ) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("\n[runtime] API error: {}", e);
                continue;
            }
        };

        // Capture the final response text via daemon IPC
        if !response.is_empty() {
            conversation::capture_response(project_hash, agent_id, &user_prompt, &response);
        }

        println!(); // Spacing between turns
    }

    println!("\n[runtime] Session ended.");
}

/// Read multi-line user input. Returns None on EOF.
fn read_user_input(stdin: &io::Stdin) -> Option<String> {
    let mut lines = Vec::new();
    let reader = stdin.lock();

    for line in reader.lines() {
        match line {
            Ok(l) => {
                if l.is_empty() && !lines.is_empty() {
                    break; // Empty line after content → submit
                }
                lines.push(l);
            }
            Err(_) => return None, // EOF
        }
    }

    if lines.is_empty() {
        return None;
    }

    Some(lines.join("\n"))
}
