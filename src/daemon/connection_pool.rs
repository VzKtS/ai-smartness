//! Connection Pool — manages lazy SQLite connections for all (project, agent) pairs.
//!
//! The global daemon maintains a single ConnectionPool that opens agent databases
//! on demand and evicts idle connections to conserve resources.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use ai_smartness::storage::database::{open_connection, ConnectionRole};
use ai_smartness::storage::migrations;
use ai_smartness::storage::path_utils;
use rusqlite::Connection;

use super::processor::PendingContext;

/// Identifies a unique agent across all projects.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct AgentKey {
    pub project_hash: String,
    pub agent_id: String,
}

impl std::fmt::Display for AgentKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", &self.project_hash[..self.project_hash.len().min(8)], self.agent_id)
    }
}

/// A single entry in the connection pool.
struct PoolEntry {
    conn: Arc<Mutex<Connection>>,
    pending: Arc<Mutex<Option<PendingContext>>>,
    last_used: Instant,
    locked: bool,
    /// Cached thread quota from the agent's ThreadMode (default: 50 = Normal).
    thread_quota: AtomicUsize,
    /// True once we've loaded the quota from the registry DB.
    quota_initialized: AtomicBool,
}

/// Pool statistics returned by `stats()`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PoolStats {
    pub active: usize,
    pub idle: usize,
    pub locked: usize,
    pub max_connections: usize,
}

/// Thread-safe connection pool for the global daemon.
///
/// Each (project_hash, agent_id) pair maps to a lazily opened SQLite connection.
/// Idle connections are evicted after `max_idle_secs`.
pub struct ConnectionPool {
    pool: Mutex<HashMap<AgentKey, PoolEntry>>,
    max_idle_secs: u64,
    max_connections: usize,
}

impl ConnectionPool {
    /// Create a new connection pool with the given limits.
    pub fn new(max_idle_secs: u64, max_connections: usize) -> Self {
        Self {
            pool: Mutex::new(HashMap::new()),
            max_idle_secs,
            max_connections,
        }
    }

    /// Get or lazily open a connection for the given agent.
    /// Runs migrations on first open. Evicts oldest idle if pool is full.
    pub fn get_or_open(&self, key: &AgentKey) -> Result<Arc<Mutex<Connection>>, String> {
        let mut pool = self.pool.lock().map_err(|e| format!("Pool lock poisoned: {}", e))?;

        // Cache hit — update last_used and return
        if let Some(entry) = pool.get_mut(key) {
            entry.last_used = Instant::now();
            tracing::debug!(project = %key.project_hash, agent = %key.agent_id, "Connection pool hit");
            return Ok(entry.conn.clone());
        }

        // Pool full — evict oldest idle
        if pool.len() >= self.max_connections {
            tracing::warn!(
                pool_size = pool.len(),
                max = self.max_connections,
                "Pool at max capacity, evicting oldest idle"
            );
            self.evict_oldest_idle(&mut pool);

            // Still full after eviction — evict absolute oldest
            if pool.len() >= self.max_connections {
                self.evict_oldest(&mut pool);
            }
        }

        // Open new connection
        let db_path = path_utils::agent_db_path(&key.project_hash, &key.agent_id);
        tracing::info!(
            project = %key.project_hash,
            agent = %key.agent_id,
            path = %db_path.display(),
            "Opening new agent DB connection"
        );

        let conn = open_connection(&db_path, ConnectionRole::Daemon)
            .map_err(|e| format!("Failed to open DB for {}: {}", key, e))?;

        migrations::migrate_agent_db(&conn)
            .map_err(|e| format!("Migration failed for {}: {}", key, e))?;

        let conn = Arc::new(Mutex::new(conn));
        let pending = Arc::new(Mutex::new(None::<PendingContext>));

        pool.insert(key.clone(), PoolEntry {
            conn: conn.clone(),
            pending,
            last_used: Instant::now(),
            locked: false,
            thread_quota: AtomicUsize::new(50), // Normal mode default
            quota_initialized: AtomicBool::new(false),
        });

        Ok(conn)
    }

    /// Get the per-agent pending context for coherence tracking.
    pub fn get_pending(&self, key: &AgentKey) -> Result<Arc<Mutex<Option<PendingContext>>>, String> {
        let pool = self.pool.lock().map_err(|e| format!("Pool lock poisoned: {}", e))?;
        pool.get(key)
            .map(|e| e.pending.clone())
            .ok_or_else(|| format!("Agent {} not in pool", key))
    }

    /// Check if an agent's memory is locked (skips prune operations).
    pub fn is_locked(&self, key: &AgentKey) -> bool {
        self.pool.lock()
            .ok()
            .and_then(|pool| pool.get(key).map(|e| e.locked))
            .unwrap_or(false)
    }

    /// Set the memory lock state for an agent.
    pub fn set_locked(&self, key: &AgentKey, locked: bool) {
        if let Ok(mut pool) = self.pool.lock() {
            if let Some(entry) = pool.get_mut(key) {
                entry.locked = locked;
            }
        }
    }

    /// Get the cached thread quota for an agent. Returns 50 (Normal) if not yet initialized.
    pub fn get_thread_quota(&self, key: &AgentKey) -> usize {
        self.pool.lock()
            .ok()
            .and_then(|pool| pool.get(key).map(|e| e.thread_quota.load(Ordering::Relaxed)))
            .unwrap_or(50)
    }

    /// Set the thread quota for an agent and mark it as initialized.
    pub fn set_thread_quota(&self, key: &AgentKey, quota: usize) {
        if let Ok(pool) = self.pool.lock() {
            if let Some(entry) = pool.get(key) {
                entry.thread_quota.store(quota, Ordering::Relaxed);
                entry.quota_initialized.store(true, Ordering::Relaxed);
            }
        }
    }

    /// Force-refresh the thread quota from the registry (used by prune cycle).
    /// Unlike the lazy `set_thread_quota`, this always writes regardless of initialization state.
    pub fn refresh_quota(&self, key: &AgentKey, quota: usize) {
        self.set_thread_quota(key, quota);
    }

    /// Check if the thread quota has been loaded from the registry for this agent.
    pub fn is_quota_initialized(&self, key: &AgentKey) -> bool {
        self.pool.lock()
            .ok()
            .and_then(|pool| pool.get(key).map(|e| e.quota_initialized.load(Ordering::Relaxed)))
            .unwrap_or(false)
    }

    /// Evict connections idle longer than `max_idle_secs`. Returns count evicted.
    pub fn evict_idle(&self) -> usize {
        let mut pool = match self.pool.lock() {
            Ok(p) => p,
            Err(_) => return 0,
        };
        let threshold = self.max_idle_secs;
        let before = pool.len();

        pool.retain(|key, entry| {
            let idle_secs = entry.last_used.elapsed().as_secs();
            if idle_secs > threshold {
                tracing::info!(
                    project = %key.project_hash,
                    agent = %key.agent_id,
                    idle_secs = idle_secs,
                    "Evicting idle connection"
                );
                false
            } else {
                true
            }
        });

        let evicted = before - pool.len();
        if evicted > 0 {
            tracing::info!(count = evicted, "Evicted idle connections");
        }
        evicted
    }

    /// Snapshot of all active agent keys.
    pub fn active_keys(&self) -> Vec<AgentKey> {
        self.pool.lock()
            .ok()
            .map(|pool| pool.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Force-evict a specific agent's connection (e.g., after Mutex poisoning).
    /// Next call to `get_or_open()` for this agent will create a fresh connection.
    pub fn force_evict(&self, key: &AgentKey) {
        if let Ok(mut pool) = self.pool.lock() {
            if pool.remove(key).is_some() {
                tracing::warn!(
                    project = %key.project_hash,
                    agent = %key.agent_id,
                    "Force-evicted poisoned connection from pool"
                );
            }
        }
    }

    /// Graceful shutdown — close all connections.
    pub fn close_all(&self) {
        if let Ok(mut pool) = self.pool.lock() {
            let count = pool.len();
            pool.clear();
            tracing::info!(count = count, "All pool connections closed");
        }
    }

    /// Pool statistics for monitoring.
    pub fn stats(&self) -> PoolStats {
        let pool = match self.pool.lock() {
            Ok(p) => p,
            Err(_) => return PoolStats { active: 0, idle: 0, locked: 0, max_connections: self.max_connections },
        };

        let idle_threshold = self.max_idle_secs / 2; // consider "idle" at half the eviction time
        let mut active = 0;
        let mut idle = 0;
        let mut locked = 0;

        for entry in pool.values() {
            if entry.locked {
                locked += 1;
            }
            if entry.last_used.elapsed().as_secs() > idle_threshold {
                idle += 1;
            } else {
                active += 1;
            }
        }

        PoolStats {
            active,
            idle,
            locked,
            max_connections: self.max_connections,
        }
    }

    /// Evict the single oldest idle connection from the pool.
    fn evict_oldest_idle(&self, pool: &mut HashMap<AgentKey, PoolEntry>) {
        let oldest = pool.iter()
            .filter(|(_, e)| e.last_used.elapsed().as_secs() > self.max_idle_secs)
            .max_by_key(|(_, e)| e.last_used.elapsed())
            .map(|(k, _)| k.clone());

        if let Some(key) = oldest {
            tracing::info!(agent = %key, "Evicting oldest idle connection for capacity");
            pool.remove(&key);
        }
    }

    /// Evict the absolute oldest connection (last resort).
    fn evict_oldest(&self, pool: &mut HashMap<AgentKey, PoolEntry>) {
        let oldest = pool.iter()
            .max_by_key(|(_, e)| e.last_used.elapsed())
            .map(|(k, _)| k.clone());

        if let Some(key) = oldest {
            tracing::info!(agent = %key, "Evicting oldest connection (forced)");
            pool.remove(&key);
        }
    }
}
