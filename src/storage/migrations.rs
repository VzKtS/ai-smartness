use crate::{AiError, AiResult};
use rusqlite::Connection;

/// Schema version actuelle
pub const CURRENT_SCHEMA_VERSION: u32 = 6;

/// Retourne la version de schema actuelle (0 si table absente)
pub fn get_schema_version(conn: &Connection) -> AiResult<u32> {
    // Check if schema_version table exists
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='schema_version'",
            [],
            |r| r.get(0),
        )
        .map_err(|e| AiError::Storage(e.to_string()))?;

    if !exists {
        return Ok(0);
    }

    let version: u32 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |r| r.get(0),
        )
        .map_err(|e| AiError::Storage(e.to_string()))?;

    Ok(version)
}

fn set_schema_version(conn: &Connection, version: u32) -> AiResult<()> {
    conn.execute(
        "INSERT OR REPLACE INTO schema_version (version, applied_at) VALUES (?1, datetime('now'))",
        rusqlite::params![version],
    )
    .map_err(|e| AiError::Storage(e.to_string()))?;
    Ok(())
}

// ── Agent DB ──

const AGENT_DB_V1: &str = "
CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS threads (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    summary TEXT DEFAULT '',
    origin_type TEXT DEFAULT 'prompt',
    parent_id TEXT,
    child_ids TEXT DEFAULT '[]',
    weight REAL DEFAULT 1.0,
    importance REAL DEFAULT 0.5,
    importance_manually_set INTEGER DEFAULT 0,
    relevance_score REAL DEFAULT 1.0,
    activation_count INTEGER DEFAULT 0,
    split_locked INTEGER DEFAULT 0,
    split_locked_until TEXT,
    topics TEXT DEFAULT '[]',
    tags TEXT DEFAULT '[]',
    labels TEXT DEFAULT '[]',
    drift_history TEXT DEFAULT '[]',
    work_context TEXT,
    ratings TEXT DEFAULT '[]',
    injection_stats TEXT,
    embedding BLOB,
    created_at TEXT NOT NULL,
    last_active TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_threads_status ON threads(status);
CREATE INDEX IF NOT EXISTS idx_threads_weight ON threads(weight);
CREATE INDEX IF NOT EXISTS idx_threads_last_active ON threads(last_active);

CREATE TABLE IF NOT EXISTS thread_messages (
    id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    source TEXT NOT NULL,
    source_type TEXT DEFAULT 'prompt',
    timestamp TEXT NOT NULL,
    metadata TEXT DEFAULT '{}'
);
CREATE INDEX IF NOT EXISTS idx_messages_thread ON thread_messages(thread_id);

CREATE TABLE IF NOT EXISTS bridges (
    id TEXT PRIMARY KEY,
    source_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    target_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    relation_type TEXT NOT NULL,
    reason TEXT DEFAULT '',
    shared_concepts TEXT DEFAULT '[]',
    confidence REAL DEFAULT 0.8,
    weight REAL DEFAULT 1.0,
    status TEXT NOT NULL DEFAULT 'active',
    propagated_from TEXT,
    propagation_depth INTEGER DEFAULT 0,
    created_by TEXT DEFAULT 'llm',
    use_count INTEGER DEFAULT 0,
    created_at TEXT NOT NULL,
    last_reinforced TEXT
);
CREATE INDEX IF NOT EXISTS idx_bridges_source ON bridges(source_id);
CREATE INDEX IF NOT EXISTS idx_bridges_target ON bridges(target_id);
CREATE INDEX IF NOT EXISTS idx_bridges_status ON bridges(status);

CREATE TABLE IF NOT EXISTS cognitive_inbox (
    id TEXT PRIMARY KEY,
    from_agent TEXT NOT NULL,
    to_agent TEXT NOT NULL,
    subject TEXT NOT NULL,
    content TEXT NOT NULL,
    priority TEXT DEFAULT 'normal',
    ttl_expiry TEXT,
    status TEXT DEFAULT 'pending',
    created_at TEXT NOT NULL,
    read_at TEXT,
    acked_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_inbox_to ON cognitive_inbox(to_agent, status);
CREATE INDEX IF NOT EXISTS idx_inbox_ttl ON cognitive_inbox(ttl_expiry) WHERE ttl_expiry IS NOT NULL;

CREATE TABLE IF NOT EXISTS dead_letters (
    id TEXT PRIMARY KEY,
    from_agent TEXT NOT NULL,
    to_agent TEXT NOT NULL,
    subject TEXT NOT NULL,
    content TEXT NOT NULL,
    priority TEXT,
    original_ttl TEXT,
    expired_at TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS health_check (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    last_check TEXT
);
";

/// Verifie et applique les migrations pour une agent DB
pub fn migrate_agent_db(conn: &Connection) -> AiResult<()> {
    let version = get_schema_version(conn)?;

    if version < 1 {
        conn.execute_batch(AGENT_DB_V1)
            .map_err(|e| AiError::Storage(format!("Agent DB V1 migration failed: {}", e)))?;
        set_schema_version(conn, 1)?;
    }

    // Agent DB has no V2 changes — just bump version marker
    if version < 2 {
        set_schema_version(conn, 2)?;
    }

    // V3: add attachments column to cognitive_inbox and dead_letters
    if version < 3 {
        conn.execute_batch(
            "ALTER TABLE cognitive_inbox ADD COLUMN attachments TEXT DEFAULT '[]';
             ALTER TABLE dead_letters ADD COLUMN attachments TEXT DEFAULT '[]';"
        ).map_err(|e| AiError::Storage(format!("Agent DB V3 migration failed: {}", e)))?;
        set_schema_version(conn, 3)?;
    }

    // V4: add concepts column to threads (semantic explosion)
    if version < 4 {
        conn.execute_batch(
            "ALTER TABLE threads ADD COLUMN concepts TEXT DEFAULT '[]';"
        ).map_err(|e| AiError::Storage(format!("Agent DB V4 migration failed: {}", e)))?;
        set_schema_version(conn, 4)?;
    }

    // V5: add is_truncated column to thread_messages
    if version < 5 {
        conn.execute_batch(
            "ALTER TABLE thread_messages ADD COLUMN is_truncated BOOLEAN DEFAULT 0;"
        ).map_err(|e| AiError::Storage(format!("Agent DB V5 migration failed: {}", e)))?;
        set_schema_version(conn, 5)?;
    }

    Ok(())
}

// ── Shared DB ──

const SHARED_DB_V1: &str = "
CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS shared_threads (
    shared_id TEXT PRIMARY KEY,
    source_thread_id TEXT NOT NULL,
    owner_agent TEXT NOT NULL,
    title TEXT NOT NULL,
    summary TEXT DEFAULT '',
    topics TEXT DEFAULT '[]',
    visibility TEXT DEFAULT 'network',
    allowed_agents TEXT DEFAULT '[]',
    include_messages INTEGER DEFAULT 0,
    snapshot TEXT DEFAULT '{}',
    published_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_shared_owner ON shared_threads(owner_agent);

CREATE TABLE IF NOT EXISTS subscriptions (
    id TEXT PRIMARY KEY,
    shared_id TEXT NOT NULL REFERENCES shared_threads(shared_id) ON DELETE CASCADE,
    subscriber_agent TEXT NOT NULL,
    subscribed_at TEXT NOT NULL,
    last_synced TEXT,
    UNIQUE(shared_id, subscriber_agent)
);

CREATE TABLE IF NOT EXISTS mcp_messages (
    id TEXT PRIMARY KEY,
    from_agent TEXT NOT NULL,
    to_agent TEXT NOT NULL,
    msg_type TEXT DEFAULT 'request',
    subject TEXT NOT NULL,
    payload TEXT DEFAULT '{}',
    priority TEXT DEFAULT 'normal',
    status TEXT DEFAULT 'pending',
    reply_to TEXT,
    thread_id TEXT,
    created_at TEXT NOT NULL,
    delivered_at TEXT,
    read_at TEXT,
    expires_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_mcp_to ON mcp_messages(to_agent, status);
CREATE INDEX IF NOT EXISTS idx_mcp_thread ON mcp_messages(thread_id);
";

/// Verifie et applique les migrations pour shared.db
pub fn migrate_shared_db(conn: &Connection) -> AiResult<()> {
    let version = get_schema_version(conn)?;

    if version < 1 {
        conn.execute_batch(SHARED_DB_V1)
            .map_err(|e| AiError::Storage(format!("Shared DB V1 migration failed: {}", e)))?;
        set_schema_version(conn, 1)?;
    }

    // Shared DB has no V2 changes — just bump version marker
    if version < 2 {
        set_schema_version(conn, 2)?;
    }

    // V3: add attachments column to mcp_messages
    if version < 3 {
        conn.execute_batch(
            "ALTER TABLE mcp_messages ADD COLUMN attachments TEXT DEFAULT '[]';"
        ).map_err(|e| AiError::Storage(format!("Shared DB V3 migration failed: {}", e)))?;
        set_schema_version(conn, 3)?;
    }

    Ok(())
}

// ── Registry DB ──

const REGISTRY_DB_V1: &str = "
CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS projects (
    hash TEXT PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    name TEXT,
    provider TEXT DEFAULT 'claude',
    agent_mode TEXT NOT NULL DEFAULT 'single',
    channel_mode TEXT NOT NULL DEFAULT 'isolated',
    messaging_mode TEXT DEFAULT 'cognitive',
    allowed_channels TEXT NOT NULL DEFAULT '[]',
    provider_config TEXT DEFAULT '{}',
    created_at TEXT NOT NULL,
    last_accessed TEXT
);

CREATE TABLE IF NOT EXISTS agents (
    id TEXT NOT NULL,
    project_hash TEXT NOT NULL REFERENCES projects(hash) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT DEFAULT '',
    role TEXT DEFAULT '',
    capabilities TEXT DEFAULT '[]',
    status TEXT DEFAULT 'available',
    last_seen TEXT NOT NULL,
    registered_at TEXT NOT NULL,
    supervisor_id TEXT,
    coordination_mode TEXT DEFAULT 'autonomous',
    team TEXT,
    specializations TEXT DEFAULT '[]',
    PRIMARY KEY (id, project_hash)
);
CREATE INDEX IF NOT EXISTS idx_agents_project ON agents(project_hash);
CREATE INDEX IF NOT EXISTS idx_agents_supervisor ON agents(supervisor_id, project_hash);
CREATE INDEX IF NOT EXISTS idx_agents_team ON agents(team, project_hash);

CREATE TABLE IF NOT EXISTS agent_tasks (
    id TEXT PRIMARY KEY,
    project_hash TEXT NOT NULL,
    assigned_to TEXT NOT NULL,
    assigned_by TEXT NOT NULL DEFAULT 'admin',
    title TEXT NOT NULL,
    description TEXT DEFAULT '',
    priority TEXT DEFAULT 'normal',
    status TEXT DEFAULT 'pending',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deadline TEXT,
    dependencies TEXT DEFAULT '[]',
    result TEXT,
    FOREIGN KEY (project_hash) REFERENCES projects(hash) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_tasks_agent ON agent_tasks(assigned_to, project_hash);
CREATE INDEX IF NOT EXISTS idx_tasks_status ON agent_tasks(status);
CREATE INDEX IF NOT EXISTS idx_tasks_assigned_by ON agent_tasks(assigned_by);

CREATE TABLE IF NOT EXISTS agent_permissions (
    agent_id TEXT NOT NULL,
    project_hash TEXT NOT NULL REFERENCES projects(hash) ON DELETE CASCADE,
    permission_level TEXT NOT NULL DEFAULT 'supervised',
    allowed_tools TEXT NOT NULL DEFAULT '[]',
    denied_tools TEXT NOT NULL DEFAULT '[]',
    can_send_messages BOOLEAN NOT NULL DEFAULT 1,
    can_broadcast BOOLEAN NOT NULL DEFAULT 0,
    can_delegate_tasks BOOLEAN NOT NULL DEFAULT 1,
    allowed_recipients TEXT NOT NULL DEFAULT '[\"*\"]',
    can_create_threads BOOLEAN NOT NULL DEFAULT 1,
    can_delete_threads BOOLEAN NOT NULL DEFAULT 1,
    can_merge_threads BOOLEAN NOT NULL DEFAULT 1,
    can_share_threads BOOLEAN NOT NULL DEFAULT 1,
    can_subscribe BOOLEAN NOT NULL DEFAULT 1,
    max_threads_override INTEGER,
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_by TEXT NOT NULL DEFAULT 'install',
    PRIMARY KEY (agent_id, project_hash),
    FOREIGN KEY (agent_id, project_hash) REFERENCES agents(id, project_hash) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS project_backups (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project_hash TEXT NOT NULL REFERENCES projects(hash) ON DELETE CASCADE,
    agent_id TEXT NOT NULL,
    backup_enabled BOOLEAN NOT NULL DEFAULT 0,
    backup_interval_hours INTEGER NOT NULL DEFAULT 24,
    max_backups INTEGER NOT NULL DEFAULT 5,
    last_backup_at TEXT,
    last_backup_path TEXT,
    last_backup_size_bytes INTEGER,
    backup_count INTEGER NOT NULL DEFAULT 0,
    auto_backup_on_prune BOOLEAN NOT NULL DEFAULT 1,
    UNIQUE (project_hash, agent_id)
);

CREATE TABLE IF NOT EXISTS federation_links (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project_hash_a TEXT NOT NULL REFERENCES projects(hash),
    project_hash_b TEXT NOT NULL REFERENCES projects(hash),
    direction TEXT NOT NULL DEFAULT 'bidirectional',
    status TEXT NOT NULL DEFAULT 'active',
    created_at TEXT NOT NULL,
    created_by TEXT NOT NULL,
    UNIQUE (project_hash_a, project_hash_b),
    CHECK (project_hash_a < project_hash_b)
);
";

/// V2 migration for registry DB — add thread_mode column to agents
const REGISTRY_DB_V2: &str = "ALTER TABLE agents ADD COLUMN thread_mode TEXT NOT NULL DEFAULT 'normal';";

/// Verifie et applique les migrations pour registry.db
pub fn migrate_registry_db(conn: &Connection) -> AiResult<()> {
    let version = get_schema_version(conn)?;

    if version < 1 {
        conn.execute_batch(REGISTRY_DB_V1)
            .map_err(|e| AiError::Storage(format!("Registry DB V1 migration failed: {}", e)))?;
        set_schema_version(conn, 1)?;
    }

    if version < 2 {
        conn.execute_batch(REGISTRY_DB_V2)
            .map_err(|e| AiError::Storage(format!("Registry DB V2 migration failed: {}", e)))?;
        set_schema_version(conn, 2)?;
    }

    // V3: add current_activity column to agents
    if version < 3 {
        conn.execute_batch(
            "ALTER TABLE agents ADD COLUMN current_activity TEXT DEFAULT '';"
        ).map_err(|e| AiError::Storage(format!("Registry DB V3 migration failed: {}", e)))?;
        set_schema_version(conn, 3)?;
    }

    // V4: add report_to, custom_role, workspace_path columns to agents
    if version < 4 {
        conn.execute_batch(
            "ALTER TABLE agents ADD COLUMN report_to TEXT DEFAULT '';
             ALTER TABLE agents ADD COLUMN custom_role TEXT DEFAULT '';
             ALTER TABLE agents ADD COLUMN workspace_path TEXT DEFAULT '';"
        ).map_err(|e| AiError::Storage(format!("Registry DB V4 migration failed: {}", e)))?;
        set_schema_version(conn, 4)?;
    }

    // V5: normalize report_to/custom_role — empty strings → NULL
    if version < 5 {
        conn.execute_batch(
            "UPDATE agents SET report_to = NULL WHERE report_to = '';
             UPDATE agents SET custom_role = NULL WHERE custom_role = '';"
        ).map_err(|e| AiError::Storage(format!("Registry DB V5 migration failed: {}", e)))?;
        set_schema_version(conn, 5)?;
    }

    // V6: add full_permissions toggle, drop dead agent_permissions table
    if version < 6 {
        conn.execute_batch(
            "ALTER TABLE agents ADD COLUMN full_permissions BOOLEAN NOT NULL DEFAULT 0;
             DROP TABLE IF EXISTS agent_permissions;"
        ).map_err(|e| AiError::Storage(format!("Registry DB V6 migration failed: {}", e)))?;
        set_schema_version(conn, 6)?;
    }

    // V7: add expected_model column to agents
    if version < 7 {
        conn.execute_batch(
            "ALTER TABLE agents ADD COLUMN expected_model TEXT;"
        ).map_err(|e| AiError::Storage(format!("Registry DB V7 migration failed: {}", e)))?;
        set_schema_version(conn, 7)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::setup_registry_db;

    #[test]
    fn test_registry_v4_columns_exist() {
        let conn = setup_registry_db();
        // V4 adds report_to, custom_role, workspace_path. Verify with a query.
        let _result: String = conn.query_row(
            "SELECT typeof(report_to) FROM agents LIMIT 0",
            [],
            |r| r.get(0),
        ).unwrap_or_else(|_| {
            // No rows, but the column parse worked — try PRAGMA instead
            "ok".to_string()
        });
        // If no rows, verify columns exist via pragma
        let cols: Vec<String> = conn
            .prepare("PRAGMA table_info(agents)")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        assert!(cols.contains(&"report_to".to_string()), "V4 should add report_to");
        assert!(cols.contains(&"custom_role".to_string()), "V4 should add custom_role");
        assert!(cols.contains(&"workspace_path".to_string()), "V4 should add workspace_path");
    }

    #[test]
    fn test_registry_v5_empty_to_null() {
        // Create a DB at V4 (before V5 migration)
        let conn = Connection::open(":memory:").unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();

        // Run only V1-V4 migrations
        conn.execute_batch(super::REGISTRY_DB_V1).unwrap();
        set_schema_version(&conn, 1).unwrap();
        conn.execute_batch(super::REGISTRY_DB_V2).unwrap();
        set_schema_version(&conn, 2).unwrap();
        conn.execute_batch("ALTER TABLE agents ADD COLUMN current_activity TEXT DEFAULT '';").unwrap();
        set_schema_version(&conn, 3).unwrap();
        conn.execute_batch(
            "ALTER TABLE agents ADD COLUMN report_to TEXT DEFAULT '';
             ALTER TABLE agents ADD COLUMN custom_role TEXT DEFAULT '';
             ALTER TABLE agents ADD COLUMN workspace_path TEXT DEFAULT '';"
        ).unwrap();
        set_schema_version(&conn, 4).unwrap();

        // Insert a project + agent with empty report_to/custom_role
        conn.execute(
            "INSERT INTO projects (hash, path, created_at) VALUES ('ph1', '/tmp/test', datetime('now'))",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO agents (id, project_hash, name, status, last_seen, registered_at, report_to, custom_role)
             VALUES ('a1', 'ph1', 'test', 'active', datetime('now'), datetime('now'), '', '')",
            [],
        ).unwrap();

        // Now run V5 migration
        migrate_registry_db(&conn).unwrap();

        // Verify empty strings became NULL
        let report_to: Option<String> = conn.query_row(
            "SELECT report_to FROM agents WHERE id = 'a1'", [], |r| r.get(0),
        ).unwrap();
        assert!(report_to.is_none(), "V5 should convert empty report_to to NULL");

        let custom_role: Option<String> = conn.query_row(
            "SELECT custom_role FROM agents WHERE id = 'a1'", [], |r| r.get(0),
        ).unwrap();
        assert!(custom_role.is_none(), "V5 should convert empty custom_role to NULL");
    }
}
