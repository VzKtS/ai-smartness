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
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA cache_size = -2000;
         PRAGMA foreign_keys = ON;
         PRAGMA temp_store = MEMORY;",
    )
    .map_err(|e| AiError::Storage(format!("Failed to configure pragmas: {}", e)))?;
    conn.execute(&format!("PRAGMA busy_timeout = {}", SQLITE_BUSY_TIMEOUT_MS), [])
        .map_err(|e| AiError::Storage(format!("Failed to set busy_timeout: {}", e)))?;
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
