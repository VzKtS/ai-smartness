//! Shared config propagation logic.
//!
//! Used by both the GUI (`save_settings`) and CLI (`config set`) to propagate
//! hooks/capture settings from the global `config.json` to per-project
//! `guardian_config.json` files read by hooks at runtime.

use crate::storage::database::{open_connection, ConnectionRole};
use crate::storage::path_utils;
use crate::storage::migrations;
use crate::storage::project_registry_impl::SqliteProjectRegistry;
use crate::project_registry::ProjectRegistryTrait;

/// Propagate `hooks` and `capture` sections from global settings to each
/// registered project's `guardian_config.json`.
///
/// Hooks (`pretool.rs`, `capture.rs`) read their config from
/// `{project_dir}/guardian_config.json`. This function ensures that changes
/// made via the GUI or CLI are visible to hooks at runtime.
pub fn sync_guardian_configs(settings: &serde_json::Value) {
    let hooks_section = settings.get("hooks").cloned().unwrap_or(serde_json::json!({}));
    let capture_section = settings.get("capture").cloned().unwrap_or(serde_json::json!({}));

    let reg_path = path_utils::registry_db_path();
    let reg_conn = match open_connection(&reg_path, ConnectionRole::Cli) {
        Ok(c) => c,
        Err(_) => return,
    };
    let _ = migrations::migrate_registry_db(&reg_conn);

    let registry = SqliteProjectRegistry::new(reg_conn);
    let projects = match registry.list_projects() {
        Ok(p) => p,
        Err(_) => return,
    };

    for project in &projects {
        let project_dir = path_utils::project_dir(&project.hash);
        if !project_dir.exists() {
            if std::fs::create_dir_all(&project_dir).is_err() {
                continue;
            }
        }

        let config_path = project_dir.join("guardian_config.json");

        // Read existing guardian_config or start fresh
        let mut guardian: serde_json::Value = if config_path.exists() {
            std::fs::read_to_string(&config_path)
                .ok()
                .and_then(|c| serde_json::from_str(&c).ok())
                .unwrap_or(serde_json::json!({}))
        } else {
            serde_json::json!({})
        };

        if !guardian.is_object() {
            guardian = serde_json::json!({});
        }

        // Merge hooks and capture sections
        let obj = guardian.as_object_mut().unwrap();
        obj.insert("hooks".to_string(), hooks_section.clone());
        obj.insert("capture".to_string(), capture_section.clone());

        if let Ok(out) = serde_json::to_string_pretty(&guardian) {
            let _ = std::fs::write(&config_path, out);
        }
    }
}
