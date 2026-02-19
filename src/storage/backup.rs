use crate::{AiError, AiResult};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub struct BackupManager;

/// Backup configuration (stored in backup_config.json).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupConfig {
    pub backup_path: String,
    pub schedule: String, // "manual", "daily", "weekly"
    pub retention_count: usize,
    pub last_backup_at: Option<String>,
    pub auto_backup_hour: u8,
}

impl Default for BackupConfig {
    fn default() -> Self {
        let default_path = crate::storage::path_utils::data_dir()
            .join("backups")
            .to_string_lossy()
            .to_string();
        Self {
            backup_path: default_path,
            schedule: "manual".to_string(),
            retention_count: 5,
            last_backup_at: None,
            auto_backup_hour: 3,
        }
    }
}

/// Info about an existing backup file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupInfo {
    pub path: String,
    pub agent_id: String,
    pub project_hash: String,
    pub date: String,
    pub size_bytes: u64,
}

const BACKUP_CONFIG_FILE: &str = "backup_config.json";

impl BackupConfig {
    /// Load backup config from data dir.
    pub fn load() -> Self {
        let path = crate::storage::path_utils::data_dir().join(BACKUP_CONFIG_FILE);
        if !path.exists() {
            return Self::default();
        }
        match std::fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Check if a scheduled backup is due right now.
    pub fn is_backup_due(&self) -> bool {
        use chrono::Timelike;
        if self.schedule == "manual" {
            return false;
        }
        let now = chrono::Utc::now();
        if now.hour() != self.auto_backup_hour as u32 {
            return false;
        }

        match self.last_backup_at.as_deref() {
            None => true,
            Some(ts) => match chrono::DateTime::parse_from_rfc3339(ts) {
                Ok(last) => {
                    let elapsed = now - last.with_timezone(&chrono::Utc);
                    match self.schedule.as_str() {
                        "daily" => elapsed.num_hours() >= 20,
                        "weekly" => elapsed.num_hours() >= 160,
                        _ => false,
                    }
                }
                Err(_) => true,
            },
        }
    }

    /// Check if a scheduled backup was missed (e.g. computer was off).
    pub fn is_missed(&self) -> bool {
        if self.schedule == "manual" {
            return false;
        }
        match self.last_backup_at.as_deref() {
            None => true,
            Some(ts) => match chrono::DateTime::parse_from_rfc3339(ts) {
                Ok(last) => {
                    let elapsed = chrono::Utc::now() - last.with_timezone(&chrono::Utc);
                    match self.schedule.as_str() {
                        "daily" => elapsed.num_hours() > 36,
                        "weekly" => elapsed.num_hours() > 192,
                        _ => false,
                    }
                }
                Err(_) => true,
            },
        }
    }

    /// Save backup config.
    pub fn save(&self) {
        let dir = crate::storage::path_utils::data_dir();
        std::fs::create_dir_all(&dir).ok();
        let path = dir.join(BACKUP_CONFIG_FILE);
        if let Ok(json) = serde_json::to_string_pretty(self) {
            std::fs::write(&path, json).ok();
        }
    }
}

impl BackupManager {
    /// Backup a SQLite DB via the .backup API.
    pub fn create_backup(conn: &Connection, dest: &Path) -> AiResult<()> {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut dst = Connection::open(dest)
            .map_err(|e| AiError::Storage(format!("Failed to open backup dest: {}", e)))?;

        let backup = rusqlite::backup::Backup::new(conn, &mut dst)
            .map_err(|e| AiError::Storage(format!("Failed to create backup: {}", e)))?;

        backup
            .run_to_completion(100, std::time::Duration::from_millis(50), None)
            .map_err(|e| AiError::Storage(format!("Backup failed: {}", e)))?;

        Ok(())
    }

    /// Restore a DB from a backup.
    pub fn restore_backup(source: &Path, dest: &Path) -> AiResult<()> {
        if !source.exists() {
            return Err(AiError::Storage(format!(
                "Backup file not found: {}",
                source.display()
            )));
        }

        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let src = Connection::open(source)
            .map_err(|e| AiError::Storage(format!("Failed to open backup source: {}", e)))?;

        let mut dst = Connection::open(dest)
            .map_err(|e| AiError::Storage(format!("Failed to open restore dest: {}", e)))?;

        let backup = rusqlite::backup::Backup::new(&src, &mut dst)
            .map_err(|e| AiError::Storage(format!("Failed to create restore: {}", e)))?;

        backup
            .run_to_completion(100, std::time::Duration::from_millis(50), None)
            .map_err(|e| AiError::Storage(format!("Restore failed: {}", e)))?;

        Ok(())
    }

    /// Create a named backup for a specific agent.
    /// Returns the backup file path.
    pub fn backup_agent(
        project_hash: &str,
        agent_id: &str,
        backup_dir: &Path,
    ) -> AiResult<PathBuf> {
        let db_path = crate::storage::path_utils::agent_db_path(project_hash, agent_id);
        if !db_path.exists() {
            return Err(AiError::Storage(format!(
                "Agent DB not found: {}",
                db_path.display()
            )));
        }

        let conn = Connection::open(&db_path)
            .map_err(|e| AiError::Storage(format!("Failed to open agent DB: {}", e)))?;

        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let filename = format!(
            "{}_{}_{}.db",
            &project_hash[..8.min(project_hash.len())],
            agent_id,
            timestamp
        );
        let dest = backup_dir.join(&filename);

        Self::create_backup(&conn, &dest)?;

        tracing::info!(agent = agent_id, dest = %dest.display(), "Backup created");
        Ok(dest)
    }

    /// List all backup files in the backup directory.
    pub fn list_backups(backup_dir: &Path) -> Vec<BackupInfo> {
        let mut backups = Vec::new();
        let entries = match std::fs::read_dir(backup_dir) {
            Ok(e) => e,
            Err(_) => return backups,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("db") {
                continue;
            }
            let filename = match path.file_stem().and_then(|s| s.to_str()) {
                Some(f) => f.to_string(),
                None => continue,
            };

            // Parse: {project_hash_prefix}_{agent_id}_{timestamp}
            let parts: Vec<&str> = filename.splitn(3, '_').collect();
            let (project_hash, agent_id, date) = if parts.len() >= 3 {
                (parts[0].to_string(), parts[1].to_string(), parts[2].to_string())
            } else {
                (String::new(), filename.clone(), String::new())
            };

            let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);

            backups.push(BackupInfo {
                path: path.to_string_lossy().to_string(),
                agent_id,
                project_hash,
                date,
                size_bytes,
            });
        }

        backups.sort_by(|a, b| b.date.cmp(&a.date)); // newest first
        backups
    }

    /// Enforce retention: delete oldest backups exceeding retention_count.
    pub fn enforce_retention(backup_dir: &Path, retention_count: usize) {
        let backups = Self::list_backups(backup_dir);
        if backups.len() <= retention_count {
            return;
        }
        for backup in backups.iter().skip(retention_count) {
            if let Err(e) = std::fs::remove_file(&backup.path) {
                tracing::warn!(path = %backup.path, error = %e, "Failed to delete old backup");
            } else {
                tracing::info!(path = %backup.path, "Old backup deleted (retention)");
            }
        }
    }
}
