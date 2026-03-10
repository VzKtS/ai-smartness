use anyhow::Result;
use ai_smartness::storage::path_utils;
use ai_smartness::user_profile::UserProfile;

pub fn list(project_hash: &str, agent_id: &str) -> Result<()> {
    let data_dir = path_utils::agent_data_dir(project_hash, agent_id);
    let profile = UserProfile::load(&data_dir);

    if profile.context_rules.is_empty() {
        println!("No rules set. Use 'ai rule add \"...\"' to add one.");
        return Ok(());
    }

    println!("Rules for agent '{}' ({} total):", agent_id, profile.context_rules.len());
    for (i, rule) in profile.context_rules.iter().enumerate() {
        println!("  {}. {}", i + 1, rule);
    }
    Ok(())
}

pub fn add(project_hash: &str, agent_id: &str, rule: &str) -> Result<()> {
    let data_dir = path_utils::agent_data_dir(project_hash, agent_id);
    let mut profile = UserProfile::load(&data_dir);

    if profile.add_rule(rule.to_string()) {
        profile.save(&data_dir);
        println!("Rule added: {}", rule);
        println!("Total: {} rules", profile.context_rules.len());
    } else {
        println!("Rule already exists (duplicate).");
    }
    Ok(())
}

pub fn remove(project_hash: &str, agent_id: &str, number: usize) -> Result<()> {
    let data_dir = path_utils::agent_data_dir(project_hash, agent_id);
    let mut profile = UserProfile::load(&data_dir);

    let idx = number.saturating_sub(1); // 1-based → 0-based
    match profile.remove_rule(idx) {
        Some(removed) => {
            profile.save(&data_dir);
            println!("Removed rule {}: {}", number, removed);
            println!("Remaining: {} rules", profile.context_rules.len());
        }
        None => {
            println!("Rule {} not found (valid range: 1-{}).", number, profile.context_rules.len());
        }
    }
    Ok(())
}
