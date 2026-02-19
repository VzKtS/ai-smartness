use crate::AiResult;
use rusqlite::Connection;

use super::database::{self, ConnectionRole};
use super::migrations;
use super::path_utils;

/// Gestionnaire de storage centralise
pub struct StorageManager {
    role: ConnectionRole,
}

impl StorageManager {
    pub fn new(role: ConnectionRole) -> Self {
        Self { role }
    }

    /// Ouvre la DB d'un agent (cree si necessaire + migration)
    pub fn open_agent_db(&self, project_hash: &str, agent_id: &str) -> AiResult<Connection> {
        let path = path_utils::agent_db_path(project_hash, agent_id);
        let conn = database::open_connection(&path, self.role)?;
        migrations::migrate_agent_db(&conn)?;
        Ok(conn)
    }

    /// Ouvre shared.db (cree si necessaire + migration)
    pub fn open_shared_db(&self, project_hash: &str) -> AiResult<Connection> {
        let path = path_utils::shared_db_path(project_hash);
        let conn = database::open_connection(&path, self.role)?;
        migrations::migrate_shared_db(&conn)?;
        Ok(conn)
    }

    /// Ouvre registry.db (cree si necessaire + migration)
    pub fn open_registry_db(&self) -> AiResult<Connection> {
        let path = path_utils::registry_db_path();
        let conn = database::open_connection(&path, self.role)?;
        migrations::migrate_registry_db(&conn)?;
        Ok(conn)
    }

    /// Cree la structure de repertoires pour un nouveau projet
    pub fn init_project(&self, project_hash: &str) -> AiResult<()> {
        let proj_dir = path_utils::project_dir(project_hash);
        std::fs::create_dir_all(proj_dir.join("agents"))?;
        std::fs::create_dir_all(proj_dir.join("backups"))?;

        // Initialize shared.db
        let _shared = self.open_shared_db(project_hash)?;

        // Write default config.json if it doesn't exist
        let config_path = path_utils::data_dir().join("config.json");
        if !config_path.exists() {
            let default_config = crate::config::GuardianConfig::default();
            if let Ok(json) = serde_json::to_string_pretty(&default_config) {
                let _ = std::fs::write(&config_path, json);
            }
        }

        Ok(())
    }
}
