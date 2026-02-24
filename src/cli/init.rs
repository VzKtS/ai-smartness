use anyhow::{Context, Result};
use ai_smartness::agent::{Agent, AgentStatus, CoordinationMode};
use ai_smartness::storage::database::{open_connection, ConnectionRole};
use ai_smartness::storage::migrations;
use ai_smartness::storage::path_utils;

/// ai init: initialize AI Smartness for a project
pub fn run(path: Option<&str>) -> Result<()> {
    let project_path = match path {
        Some(p) => std::path::PathBuf::from(p),
        None => std::env::current_dir().context("Failed to get current directory")?,
    };

    let project_path = project_path
        .canonicalize()
        .context("Failed to resolve project path")?;

    let hash = path_utils::project_hash(&project_path)
        .context("Failed to compute project hash")?;

    let project_name = project_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    println!("Initializing AI Smartness for: {}", project_path.display());
    println!("Project hash: {}", hash);

    // 1. Create project directory structure
    let proj_dir = path_utils::project_dir(&hash);
    std::fs::create_dir_all(&proj_dir)
        .context("Failed to create project directory")?;
    println!("  Created project directory");

    // 2. Initialize registry DB + register project
    let reg_path = path_utils::registry_db_path();
    let reg_conn = open_connection(&reg_path, ConnectionRole::Cli)
        .context("Failed to open registry database")?;
    migrations::migrate_registry_db(&reg_conn)
        .context("Failed to migrate registry database")?;

    // Register project in registry
    let mut registry = ai_smartness::storage::project_registry_impl::SqliteProjectRegistry::new(reg_conn);
    use ai_smartness::project_registry::{MessagingMode, ProjectEntry, ProjectRegistryTrait};
    let entry = ProjectEntry {
        hash: hash.clone(),
        path: project_path.to_string_lossy().to_string(),
        name: Some(project_name.clone()),
        provider: "claude".to_string(),
        messaging_mode: MessagingMode::Cognitive,
        provider_config: serde_json::json!({}),
        created_at: chrono::Utc::now(),
        last_accessed: Some(chrono::Utc::now()),
    };
    registry.add_project(entry)
        .context("Failed to register project")?;
    println!("  Registered project in registry");

    // 3. Initialize agent DB for main agent
    let agent_db_path = path_utils::agent_db_path(&hash, "main");
    let agent_conn = open_connection(&agent_db_path, ConnectionRole::Cli)
        .context("Failed to open agent database")?;
    migrations::migrate_agent_db(&agent_conn)
        .context("Failed to migrate agent database")?;
    println!("  Initialized main agent database");

    // 4. Initialize shared DB
    let shared_path = path_utils::shared_db_path(&hash);
    let shared_conn = open_connection(&shared_path, ConnectionRole::Cli)
        .context("Failed to open shared database")?;
    migrations::migrate_shared_db(&shared_conn)
        .context("Failed to migrate shared database")?;
    println!("  Initialized shared database");

    // 5. Register main agent in registry
    // Re-open registry connection (previous one was moved into SqliteProjectRegistry)
    let reg_conn2 = open_connection(&reg_path, ConnectionRole::Cli)
        .context("Failed to reopen registry database")?;
    let agent = Agent {
        id: "main".to_string(),
        project_hash: hash.clone(),
        name: "Main Agent".to_string(),
        description: "Primary memory agent".to_string(),
        role: "owner".to_string(),
        capabilities: vec!["memory".to_string(), "recall".to_string(), "gossip".to_string()],
        status: AgentStatus::Active,
        last_seen: chrono::Utc::now(),
        registered_at: chrono::Utc::now(),
        supervisor_id: None,
        coordination_mode: CoordinationMode::Autonomous,
        team: None,
        specializations: vec![],
        thread_mode: ai_smartness::agent::ThreadMode::Normal,
        current_activity: String::new(),
        report_to: None,
        custom_role: None,
        workspace_path: String::new(),
        full_permissions: false,
    };
    ai_smartness::registry::registry::AgentRegistry::register(&reg_conn2, &agent)
        .context("Failed to register main agent")?;
    println!("  Registered agent: main (owner)");

    // 6. Create wake signals directory
    let wake_dir = path_utils::wake_signals_dir();
    std::fs::create_dir_all(&wake_dir)
        .context("Failed to create wake signals directory")?;

    // 7. Write default config.json if it doesn't exist
    let config_path = path_utils::data_dir().join("config.json");
    if !config_path.exists() {
        let default_config = ai_smartness::config::GuardianConfig::default();
        if let Ok(json) = serde_json::to_string_pretty(&default_config) {
            std::fs::write(&config_path, &json)
                .context("Failed to write default config.json")?;
            println!("  Created default config.json");
        }
    }

    println!("\nAI Smartness initialized successfully!");
    println!("Project: {} ({})", project_name, &hash[..8]);
    println!("\nNext steps:");
    println!("  ai daemon start    — Start the background daemon");
    println!("  ai status          — Check memory status");
    println!("  ai agent add <id>  — Register additional agents");

    Ok(())
}
