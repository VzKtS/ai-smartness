use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Mode de messagerie inter-agents (exclusif par projet)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessagingMode {
    /// msg_focus -> cognitive inbox SQLite dans la DB cible
    Cognitive,
    /// msg_send -> broker MCP dans shared.db
    Mcp,
}

impl Default for MessagingMode {
    fn default() -> Self {
        Self::Cognitive
    }
}

/// Entree du registre de projets
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectEntry {
    pub hash: String,
    pub path: String,
    pub name: Option<String>,
    pub provider: String,
    pub messaging_mode: MessagingMode,
    pub provider_config: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub last_accessed: Option<DateTime<Utc>>,
}

/// Trait pour le registre de projets (WASM-safe, pas d'I/O)
pub trait ProjectRegistryTrait {
    type Error: std::error::Error;

    fn add_project(&mut self, entry: ProjectEntry) -> Result<(), Self::Error>;
    fn remove_project(&mut self, hash: &str) -> Result<(), Self::Error>;
    fn get_project(&self, hash: &str) -> Result<Option<ProjectEntry>, Self::Error>;
    fn get_project_by_path(&self, path: &str) -> Result<Option<ProjectEntry>, Self::Error>;
    fn list_projects(&self) -> Result<Vec<ProjectEntry>, Self::Error>;
    fn update_last_accessed(&mut self, hash: &str) -> Result<(), Self::Error>;
    fn update_project(&mut self, hash: &str, name: Option<&str>, path: Option<&str>, provider: Option<&str>) -> Result<(), Self::Error>;
}
