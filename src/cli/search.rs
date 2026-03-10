use anyhow::{Context, Result};
use ai_smartness::storage::database::{open_connection, ConnectionRole};
use ai_smartness::storage::path_utils;
use ai_smartness::storage::threads::ThreadStorage;

use ai_smartness::constants::truncate_safe;

use super::{resolve_project_hash, resolve_agent_id};

pub fn run(query: &str, project_hash: Option<&str>, agent_id: Option<&str>) -> Result<()> {
    let hash = resolve_project_hash(project_hash)?;
    let agent_id = resolve_agent_id(agent_id, &hash)?;
    let db_path = path_utils::agent_db_path(&hash, &agent_id);
    let conn = open_connection(&db_path, ConnectionRole::Cli)
        .context("Failed to open agent database")?;

    let results = ThreadStorage::search(&conn, query)
        .context("Search failed")?;

    if results.is_empty() {
        println!("No results for: {}", query);
        return Ok(());
    }

    println!("Results for: {}\n", query);
    println!(
        "{:<12}  {:<30}  {:<10}  {}",
        "ID", "TITLE", "STATUS", "TOPICS"
    );
    println!("{}", "-".repeat(70));

    for t in &results {
        let id_short = if t.id.len() > 11 { &t.id[..11] } else { &t.id };
        let title = if t.title.len() > 29 {
            format!("{}...", truncate_safe(&t.title, 26))
        } else {
            t.title.clone()
        };
        let topics = t.topics.join(", ");

        println!(
            "{:<12}  {:<30}  {:<10}  {}",
            id_short,
            title,
            t.status.as_str(),
            topics,
        );
    }

    println!("\nFound: {} threads", results.len());

    Ok(())
}
