use anyhow::{Context, Result};
use ai_smartness::thread::ThreadStatus;
use ai_smartness::storage::bridges::BridgeStorage;
use ai_smartness::storage::cognitive_inbox::CognitiveInbox;
use ai_smartness::storage::database::{open_connection, ConnectionRole};
use ai_smartness::storage::path_utils;
use ai_smartness::storage::threads::ThreadStorage;

use super::{resolve_project_hash, resolve_agent_id};

pub fn run(project_hash: Option<&str>, agent_id: Option<&str>) -> Result<()> {
    let hash = resolve_project_hash(project_hash)?;
    let agent_id = resolve_agent_id(agent_id, &hash)?;
    let db_path = path_utils::agent_db_path(&hash, &agent_id);
    let conn = open_connection(&db_path, ConnectionRole::Cli)
        .context("Failed to open agent database")?;

    let active = ThreadStorage::count_by_status(&conn, &ThreadStatus::Active).unwrap_or(0);
    let suspended = ThreadStorage::count_by_status(&conn, &ThreadStatus::Suspended).unwrap_or(0);
    let archived = ThreadStorage::count_by_status(&conn, &ThreadStatus::Archived).unwrap_or(0);
    let total_threads = active + suspended + archived;
    let bridges = BridgeStorage::count(&conn).unwrap_or(0);
    let inbox = CognitiveInbox::count_pending(&conn, &agent_id).unwrap_or(0);

    println!("AI Smartness Status");
    println!("===================");
    println!("Project hash: {}", &hash[..std::cmp::min(12, hash.len())]);
    println!("Agent: {}", agent_id);
    println!();
    println!("Threads:");
    println!("  Active:    {:>5}", active);
    println!("  Suspended: {:>5}", suspended);
    println!("  Archived:  {:>5}", archived);
    println!("  Total:     {:>5}", total_threads);
    println!();
    println!("Bridges:     {:>5}", bridges);
    println!("Inbox:       {:>5}", inbox);

    // Check daemon status
    let data_dir = path_utils::data_dir();
    let pid_file = data_dir.join("daemon.pid");
    if pid_file.exists() {
        if let Ok(pid_str) = std::fs::read_to_string(&pid_file) {
            println!();
            println!("Daemon: running (PID {})", pid_str.trim());
        }
    } else {
        println!();
        println!("Daemon: not running");
    }

    Ok(())
}
