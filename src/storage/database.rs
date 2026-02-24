use crate::constants::SQLITE_BUSY_TIMEOUT_MS;
use crate::{AiError, AiResult};
use rusqlite::Connection;

/// Configuration role: differencie les pragmas selon le binaire appelant
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionRole {
    Daemon, // wal_autocheckpoint = 1000
    Hook,   // wal_autocheckpoint = 0
    Mcp,    // wal_autocheckpoint = 0
    Cli,    // wal_autocheckpoint = 0
}

/// Ouvre une connexion SQLite avec les pragmas appropries
pub fn open_connection(path: &std::path::Path, role: ConnectionRole) -> AiResult<Connection> {
    // Create parent directories if needed
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let conn = Connection::open(path)
        .map_err(|e| AiError::Storage(format!("Failed to open {}: {}", path.display(), e)))?;

    tracing::debug!(path = %path.display(), role = ?role, "Database connection opened");

    configure_common(&conn)?;

    match role {
        ConnectionRole::Daemon => configure_daemon(&conn)?,
        ConnectionRole::Hook | ConnectionRole::Mcp | ConnectionRole::Cli => {
            configure_ephemeral(&conn)?
        }
    }

    Ok(conn)
}

/// Pragmas communs a toutes les connexions:
/// - journal_mode = WAL
/// - busy_timeout = SQLITE_BUSY_TIMEOUT_MS (constants.rs)
/// - synchronous = NORMAL
/// - cache_size = -2000 (2 MB)
/// - foreign_keys = ON
/// - temp_store = MEMORY
fn configure_common(conn: &Connection) -> AiResult<()> {
    conn.execute_batch(&format!(
        "PRAGMA journal_mode = WAL;
         PRAGMA busy_timeout = {};
         PRAGMA synchronous = NORMAL;
         PRAGMA cache_size = -2000;
         PRAGMA foreign_keys = ON;
         PRAGMA temp_store = MEMORY;",
        SQLITE_BUSY_TIMEOUT_MS,
    ))
    .map_err(|e| AiError::Storage(format!("Failed to configure pragmas: {}", e)))?;
    Ok(())
}

/// Pragmas specifiques au daemon
fn configure_daemon(conn: &Connection) -> AiResult<()> {
    conn.execute_batch(
        &format!(
            "PRAGMA wal_autocheckpoint = {};",
            crate::constants::DAEMON_WAL_AUTOCHECKPOINT
        ),
    )
    .map_err(|e| AiError::Storage(format!("Failed to configure daemon pragmas: {}", e)))?;
    Ok(())
}

/// Pragmas specifiques aux hooks/mcp/cli
fn configure_ephemeral(conn: &Connection) -> AiResult<()> {
    conn.execute_batch(
        &format!(
            "PRAGMA wal_autocheckpoint = {};",
            crate::constants::HOOK_WAL_AUTOCHECKPOINT
        ),
    )
    .map_err(|e| AiError::Storage(format!("Failed to configure ephemeral pragmas: {}", e)))?;
    Ok(())
}

/// Checkpoint PASSIVE (daemon uniquement)
pub fn checkpoint_passive(conn: &Connection) -> AiResult<()> {
    conn.execute_batch("PRAGMA wal_checkpoint(PASSIVE);")
        .map_err(|e| AiError::Storage(format!("WAL checkpoint failed: {}", e)))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::{DAEMON_WAL_AUTOCHECKPOINT, HOOK_WAL_AUTOCHECKPOINT, SQLITE_BUSY_TIMEOUT_MS};

    fn tmp_db_path() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        (dir, path)
    }

    #[test]
    fn test_open_connection_smoke() {
        let (_dir, path) = tmp_db_path();
        let conn = open_connection(&path, ConnectionRole::Hook);
        assert!(conn.is_ok(), "open_connection should not error");
    }

    #[test]
    fn test_busy_timeout_set_correctly() {
        let (_dir, path) = tmp_db_path();
        let conn = open_connection(&path, ConnectionRole::Hook).unwrap();
        let timeout: u32 = conn
            .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
            .unwrap();
        assert_eq!(timeout, SQLITE_BUSY_TIMEOUT_MS);
    }

    #[test]
    fn test_daemon_wal_autocheckpoint() {
        let (_dir, path) = tmp_db_path();
        let conn = open_connection(&path, ConnectionRole::Daemon).unwrap();
        let ckpt: u32 = conn
            .query_row("PRAGMA wal_autocheckpoint", [], |row| row.get(0))
            .unwrap();
        assert_eq!(ckpt, DAEMON_WAL_AUTOCHECKPOINT);
    }

    #[test]
    fn test_hook_wal_autocheckpoint() {
        let (_dir, path) = tmp_db_path();
        let conn = open_connection(&path, ConnectionRole::Hook).unwrap();
        let ckpt: u32 = conn
            .query_row("PRAGMA wal_autocheckpoint", [], |row| row.get(0))
            .unwrap();
        assert_eq!(ckpt, HOOK_WAL_AUTOCHECKPOINT);
        assert_ne!(ckpt, DAEMON_WAL_AUTOCHECKPOINT);
    }
}
