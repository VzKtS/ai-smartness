use serde::{Deserialize, Serialize};

/// Network protocol messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkMessage {
    Hello { agent_id: String, version: String },
    HelloAck { agent_id: String, version: String },
    SyncRequest { thread_ids: Vec<String> },
    SyncResponse { threads: Vec<serde_json::Value> },
    RouteMessage { to_agent: String, payload: serde_json::Value },
    Heartbeat { agent_id: String, timestamp: String },
    Disconnect { agent_id: String, reason: String },
}
