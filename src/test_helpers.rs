//! Shared test utilities — builders, DB setup, time helpers.
//!
//! Available only under `#[cfg(test)]`.

use chrono::{DateTime, Duration, Utc};
use crate::bridge::{BridgeStatus, BridgeType, ThinkBridge};
use crate::thread::{Thread, ThreadMessage, ThreadStatus, OriginType, InjectionStats};

// ============================================================================
// ThreadBuilder
// ============================================================================

pub struct ThreadBuilder {
    thread: Thread,
}

impl ThreadBuilder {
    pub fn new() -> Self {
        Self {
            thread: Thread {
                id: uuid::Uuid::new_v4().to_string(),
                title: "Test thread".to_string(),
                status: ThreadStatus::Active,
                weight: 1.0,
                importance: 0.5,
                importance_manually_set: false,
                created_at: Utc::now(),
                last_active: Utc::now(),
                activation_count: 1,
                split_locked: false,
                split_locked_until: None,
                origin_type: OriginType::Prompt,
                drift_history: vec![],
                parent_id: None,
                child_ids: vec![],
                summary: None,
                topics: vec![],
                tags: vec![],
                labels: vec![],
                concepts: vec![],
                embedding: None,
                relevance_score: 0.0,
                ratings: vec![],
                work_context: None,
                injection_stats: Some(InjectionStats::default()),
            },
        }
    }

    /// Quick builder — just set a title, everything else defaults.
    pub fn build_minimal(title: &str) -> Thread {
        Self::new().title(title).build()
    }

    pub fn id(mut self, id: &str) -> Self {
        self.thread.id = id.to_string();
        self
    }

    pub fn title(mut self, t: &str) -> Self {
        self.thread.title = t.to_string();
        self
    }

    pub fn status(mut self, s: ThreadStatus) -> Self {
        self.thread.status = s;
        self
    }

    pub fn weight(mut self, w: f64) -> Self {
        self.thread.weight = w;
        self
    }

    pub fn importance(mut self, i: f64) -> Self {
        self.thread.importance = i;
        self
    }

    pub fn last_active(mut self, dt: DateTime<Utc>) -> Self {
        self.thread.last_active = dt;
        self
    }

    pub fn created_at(mut self, dt: DateTime<Utc>) -> Self {
        self.thread.created_at = dt;
        self
    }

    pub fn topics(mut self, t: Vec<&str>) -> Self {
        self.thread.topics = t.into_iter().map(String::from).collect();
        self
    }

    pub fn labels(mut self, l: Vec<&str>) -> Self {
        self.thread.labels = l.into_iter().map(String::from).collect();
        self
    }

    pub fn concepts(mut self, c: Vec<&str>) -> Self {
        self.thread.concepts = c.into_iter().map(String::from).collect();
        self
    }

    pub fn summary(mut self, s: &str) -> Self {
        self.thread.summary = Some(s.to_string());
        self
    }

    pub fn origin_type(mut self, o: OriginType) -> Self {
        self.thread.origin_type = o;
        self
    }

    pub fn build(self) -> Thread {
        self.thread
    }
}

// ============================================================================
// BridgeBuilder
// ============================================================================

pub struct BridgeBuilder {
    bridge: ThinkBridge,
}

impl BridgeBuilder {
    pub fn new() -> Self {
        Self {
            bridge: ThinkBridge {
                id: uuid::Uuid::new_v4().to_string(),
                source_id: "source-placeholder".to_string(),
                target_id: "target-placeholder".to_string(),
                relation_type: BridgeType::Extends,
                reason: "Test bridge".to_string(),
                shared_concepts: vec![],
                weight: 1.0,
                confidence: 0.8,
                status: BridgeStatus::Active,
                propagated_from: None,
                propagation_depth: 0,
                created_by: "test-agent".to_string(),
                use_count: 0,
                created_at: Utc::now(),
                last_reinforced: None,
            },
        }
    }

    pub fn id(mut self, id: &str) -> Self {
        self.bridge.id = id.to_string();
        self
    }

    pub fn source_id(mut self, id: &str) -> Self {
        self.bridge.source_id = id.to_string();
        self
    }

    pub fn target_id(mut self, id: &str) -> Self {
        self.bridge.target_id = id.to_string();
        self
    }

    pub fn relation_type(mut self, r: BridgeType) -> Self {
        self.bridge.relation_type = r;
        self
    }

    pub fn weight(mut self, w: f64) -> Self {
        self.bridge.weight = w;
        self
    }

    pub fn confidence(mut self, c: f64) -> Self {
        self.bridge.confidence = c;
        self
    }

    pub fn status(mut self, s: BridgeStatus) -> Self {
        self.bridge.status = s;
        self
    }

    pub fn created_at(mut self, dt: DateTime<Utc>) -> Self {
        self.bridge.created_at = dt;
        self
    }

    pub fn created_by(mut self, agent: &str) -> Self {
        self.bridge.created_by = agent.to_string();
        self
    }

    pub fn build(self) -> ThinkBridge {
        self.bridge
    }
}

// ============================================================================
// ThreadMessageBuilder
// ============================================================================

pub struct ThreadMessageBuilder {
    msg: ThreadMessage,
}

impl ThreadMessageBuilder {
    pub fn new(thread_id: &str) -> Self {
        Self {
            msg: ThreadMessage {
                thread_id: thread_id.to_string(),
                msg_id: uuid::Uuid::new_v4().to_string(),
                content: "Test message content".to_string(),
                source: "test".to_string(),
                source_type: "prompt".to_string(),
                timestamp: Utc::now(),
                metadata: serde_json::json!({}),
                is_truncated: false,
            },
        }
    }

    pub fn content(mut self, c: &str) -> Self {
        self.msg.content = c.to_string();
        self
    }

    pub fn source(mut self, s: &str) -> Self {
        self.msg.source = s.to_string();
        self
    }

    pub fn build(self) -> ThreadMessage {
        self.msg
    }
}

// ============================================================================
// Time helpers
// ============================================================================

pub fn hours_ago(h: i64) -> DateTime<Utc> {
    Utc::now() - Duration::hours(h)
}

pub fn days_ago(d: i64) -> DateTime<Utc> {
    Utc::now() - Duration::days(d)
}

// ============================================================================
// DB setup helpers
// ============================================================================

use rusqlite::Connection;
use crate::storage::migrations;

/// Create an in-memory agent DB with all migrations applied.
pub fn setup_agent_db() -> Connection {
    let conn = Connection::open(":memory:").unwrap();
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;").unwrap();
    migrations::migrate_agent_db(&conn).unwrap();
    conn
}

/// Create an in-memory agent DB WITHOUT foreign key enforcement.
/// Use for tests that need to insert orphan bridges.
pub fn setup_agent_db_no_fk() -> Connection {
    let conn = Connection::open(":memory:").unwrap();
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=OFF;").unwrap();
    migrations::migrate_agent_db(&conn).unwrap();
    // Ensure FK is still off after migrations
    conn.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
    conn
}

/// Create an in-memory shared DB with all migrations applied.
pub fn setup_shared_db() -> Connection {
    let conn = Connection::open(":memory:").unwrap();
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;").unwrap();
    migrations::migrate_shared_db(&conn).unwrap();
    conn
}

/// Create an in-memory registry DB with all migrations applied.
pub fn setup_registry_db() -> Connection {
    let conn = Connection::open(":memory:").unwrap();
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;").unwrap();
    migrations::migrate_registry_db(&conn).unwrap();
    conn
}

/// Create a full tool context (agent + registry + shared DBs).
pub fn setup_tool_context() -> (Connection, Connection, Connection) {
    (setup_agent_db(), setup_registry_db(), setup_shared_db())
}
