use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessagePriority {
    Low,
    Normal,
    High,
    Urgent,
}

impl MessagePriority {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Normal => "normal",
            Self::High => "high",
            Self::Urgent => "urgent",
        }
    }
}

impl std::str::FromStr for MessagePriority {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "low" => Ok(Self::Low),
            "normal" => Ok(Self::Normal),
            "high" => Ok(Self::High),
            "urgent" => Ok(Self::Urgent),
            _ => Err(format!("Unknown message priority: {}", s)),
        }
    }
}

impl Default for MessagePriority {
    fn default() -> Self {
        Self::Normal
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessageStatus {
    Pending,
    Read,
    Acked,
    Expired,
}

impl MessageStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Read => "read",
            Self::Acked => "acked",
            Self::Expired => "expired",
        }
    }
}

impl std::str::FromStr for MessageStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "read" => Ok(Self::Read),
            "acked" => Ok(Self::Acked),
            "expired" => Ok(Self::Expired),
            _ => Err(format!("Unknown message status: {}", s)),
        }
    }
}

impl Default for MessageStatus {
    fn default() -> Self {
        Self::Pending
    }
}

/// Message cognitif (cognitive inbox) ou MCP
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub from_agent: String,
    pub to_agent: String,
    pub subject: String,
    pub content: String,
    pub priority: MessagePriority,
    pub ttl_expiry: DateTime<Utc>,
    pub status: MessageStatus,
    pub created_at: DateTime<Utc>,
    pub read_at: Option<DateTime<Utc>>,
    pub acked_at: Option<DateTime<Utc>>,
}
