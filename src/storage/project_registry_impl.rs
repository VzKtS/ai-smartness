use crate::project_registry::{MessagingMode, ProjectEntry, ProjectRegistryTrait};
use crate::time_utils;
use crate::AiError;
use rusqlite::{params, Connection, Row};

/// Implementation SQLite du registre de projets
pub struct SqliteProjectRegistry {
    conn: Connection,
}

fn project_from_row(row: &Row) -> rusqlite::Result<ProjectEntry> {
    let created_str: String = row.get("created_at")?;
    let accessed_str: Option<String> = row.get("last_accessed")?;
    let messaging_str: String = row.get("messaging_mode")?;
    let config_str: String = row.get("provider_config")?;

    Ok(ProjectEntry {
        hash: row.get("hash")?,
        path: row.get("path")?,
        name: row.get("name")?,
        provider: row.get("provider")?,
        messaging_mode: match messaging_str.as_str() {
            "mcp" => MessagingMode::Mcp,
            _ => MessagingMode::Cognitive,
        },
        provider_config: serde_json::from_str(&config_str)
            .unwrap_or(serde_json::Value::Object(Default::default())),
        created_at: time_utils::from_sqlite(&created_str).unwrap_or_else(|_| chrono::Utc::now()),
        last_accessed: accessed_str.and_then(|s| time_utils::from_sqlite(&s).ok()),
    })
}

impl SqliteProjectRegistry {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }
}

impl ProjectRegistryTrait for SqliteProjectRegistry {
    type Error = AiError;

    fn add_project(&mut self, entry: ProjectEntry) -> Result<(), Self::Error> {
        let now = time_utils::to_sqlite(&time_utils::now());
        let messaging = match entry.messaging_mode {
            MessagingMode::Mcp => "mcp",
            MessagingMode::Cognitive => "cognitive",
        };
        self.conn
            .execute(
                "INSERT OR REPLACE INTO projects (hash, path, name, provider, messaging_mode, provider_config, created_at, last_accessed)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    entry.hash,
                    entry.path,
                    entry.name,
                    entry.provider,
                    messaging,
                    serde_json::to_string(&entry.provider_config).unwrap_or_else(|_| "{}".into()),
                    now,
                    now,
                ],
            )
            .map_err(|e| AiError::Storage(format!("Add project failed: {}", e)))?;
        Ok(())
    }

    fn remove_project(&mut self, hash: &str) -> Result<(), Self::Error> {
        self.conn
            .execute("DELETE FROM projects WHERE hash = ?1", params![hash])
            .map_err(|e| AiError::Storage(format!("Remove project failed: {}", e)))?;
        Ok(())
    }

    fn get_project(&self, hash: &str) -> Result<Option<ProjectEntry>, Self::Error> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM projects WHERE hash = ?1")
            .map_err(|e| AiError::Storage(e.to_string()))?;

        let result = stmt
            .query_row(params![hash], project_from_row)
            .optional()
            .map_err(|e| AiError::Storage(e.to_string()))?;

        Ok(result)
    }

    fn get_project_by_path(&self, path: &str) -> Result<Option<ProjectEntry>, Self::Error> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM projects WHERE path = ?1")
            .map_err(|e| AiError::Storage(e.to_string()))?;

        let result = stmt
            .query_row(params![path], project_from_row)
            .optional()
            .map_err(|e| AiError::Storage(e.to_string()))?;

        Ok(result)
    }

    fn list_projects(&self) -> Result<Vec<ProjectEntry>, Self::Error> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM projects ORDER BY last_accessed DESC")
            .map_err(|e| AiError::Storage(e.to_string()))?;

        let projects = stmt
            .query_map([], project_from_row)
            .map_err(|e| AiError::Storage(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(projects)
    }

    fn update_last_accessed(&mut self, hash: &str) -> Result<(), Self::Error> {
        let now = time_utils::to_sqlite(&time_utils::now());
        self.conn
            .execute(
                "UPDATE projects SET last_accessed = ?1 WHERE hash = ?2",
                params![now, hash],
            )
            .map_err(|e| AiError::Storage(format!("Update last_accessed failed: {}", e)))?;
        Ok(())
    }

    fn update_project(&mut self, hash: &str, name: Option<&str>, path: Option<&str>, provider: Option<&str>) -> Result<(), Self::Error> {
        if let Some(n) = name {
            self.conn
                .execute("UPDATE projects SET name = ?1 WHERE hash = ?2", params![n, hash])
                .map_err(|e| AiError::Storage(format!("Update project name failed: {}", e)))?;
        }
        if let Some(p) = path {
            self.conn
                .execute("UPDATE projects SET path = ?1 WHERE hash = ?2", params![p, hash])
                .map_err(|e| AiError::Storage(format!("Update project path failed: {}", e)))?;
        }
        if let Some(prov) = provider {
            self.conn
                .execute("UPDATE projects SET provider = ?1 WHERE hash = ?2", params![prov, hash])
                .map_err(|e| AiError::Storage(format!("Update project provider failed: {}", e)))?;
        }
        Ok(())
    }
}

trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
