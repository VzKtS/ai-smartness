use anyhow::{bail, Context, Result};
use ai_smartness::thread::ThreadStatus;
use ai_smartness::storage::database::{open_connection, ConnectionRole};
use ai_smartness::storage::path_utils;
use ai_smartness::storage::threads::ThreadStorage;

use ai_smartness::constants::truncate_safe;

use super::{resolve_project_hash, resolve_agent_id};

pub fn run(status_filter: Option<&str>, project_hash: Option<&str>, agent_id: Option<&str>) -> Result<()> {
    let hash = resolve_project_hash(project_hash)?;
    let agent_id = resolve_agent_id(agent_id, &hash)?;
    let db_path = path_utils::agent_db_path(&hash, &agent_id);
    let conn = open_connection(&db_path, ConnectionRole::Cli)
        .context("Failed to open agent database")?;

    let threads = match status_filter {
        Some("active") => ThreadStorage::list_by_status(&conn, &ThreadStatus::Active),
        Some("suspended") => ThreadStorage::list_by_status(&conn, &ThreadStatus::Suspended),
        Some("archived") => ThreadStorage::list_by_status(&conn, &ThreadStatus::Archived),
        Some(other) => bail!("Unknown status: {}. Use: active, suspended, archived", other),
        None => ThreadStorage::list_all(&conn),
    }
    .context("Failed to list threads")?;

    if threads.is_empty() {
        println!("No threads found.");
        return Ok(());
    }

    println!(
        "{:<12}  {:<30}  {:<10}  {:>6}  {:>5}  {}",
        "ID", "TITLE", "STATUS", "WEIGHT", "IMP", "TOPICS"
    );
    println!("{}", "-".repeat(85));

    for t in &threads {
        let id_short = if t.id.len() > 11 { &t.id[..11] } else { &t.id };
        let title = if t.title.len() > 29 {
            format!("{}...", truncate_safe(&t.title, 26))
        } else {
            t.title.clone()
        };
        let topics = t.topics.join(", ");
        let topics_display = if topics.len() > 20 {
            format!("{}...", truncate_safe(&topics, 17))
        } else {
            topics
        };

        println!(
            "{:<12}  {:<30}  {:<10}  {:>6.2}  {:>5.2}  {}",
            id_short,
            title,
            t.status.as_str(),
            t.weight,
            t.importance,
            topics_display,
        );
    }

    println!("\nTotal: {} threads", threads.len());

    Ok(())
}
