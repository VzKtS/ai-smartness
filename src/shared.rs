use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedThread {
    pub shared_id: String,
    pub thread_id: String,
    pub owner_agent: String,
    pub title: String,
    pub topics: Vec<String>,
    pub visibility: SharedVisibility,
    pub allowed_agents: Vec<String>,
    pub published_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SharedVisibility {
    Network,
    Restricted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    pub shared_id: String,
    pub subscriber_agent: String,
    pub subscribed_at: DateTime<Utc>,
    pub last_synced: Option<DateTime<Utc>>,
}
