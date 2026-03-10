use anyhow::{Context, Result};
use ai_smartness::storage::bridges::BridgeStorage;
use ai_smartness::storage::database::{open_connection, ConnectionRole};
use ai_smartness::storage::path_utils;

use super::{resolve_project_hash, resolve_agent_id};

pub fn run(project_hash: Option<&str>, agent_id: Option<&str>) -> Result<()> {
    let hash = resolve_project_hash(project_hash)?;
    let agent_id = resolve_agent_id(agent_id, &hash)?;
    let db_path = path_utils::agent_db_path(&hash, &agent_id);
    let conn = open_connection(&db_path, ConnectionRole::Cli)
        .context("Failed to open agent database")?;

    let bridges = BridgeStorage::list_all(&conn)
        .context("Failed to list bridges")?;

    if bridges.is_empty() {
        println!("No bridges found.");
        return Ok(());
    }

    println!(
        "{:<12}  {:<12}  {:<12}  {:<12}  {:>6}  {:<8}",
        "ID", "SOURCE", "TARGET", "TYPE", "WEIGHT", "STATUS"
    );
    println!("{}", "-".repeat(70));

    for b in &bridges {
        let id_short = if b.id.len() > 11 { &b.id[..11] } else { &b.id };
        let src_short = if b.source_id.len() > 11 { &b.source_id[..11] } else { &b.source_id };
        let tgt_short = if b.target_id.len() > 11 { &b.target_id[..11] } else { &b.target_id };

        println!(
            "{:<12}  {:<12}  {:<12}  {:<12}  {:>6.2}  {:<8}",
            id_short,
            src_short,
            tgt_short,
            b.relation_type.as_str(),
            b.weight,
            b.status.as_str(),
        );
    }

    println!("\nTotal: {} bridges", bridges.len());

    Ok(())
}
