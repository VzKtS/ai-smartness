use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BridgeType {
    Extends,
    Contradicts,
    Depends,
    Replaces,
    ChildOf,
    Sibling,
}

impl BridgeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Extends => "extends",
            Self::Contradicts => "contradicts",
            Self::Depends => "depends",
            Self::Replaces => "replaces",
            Self::ChildOf => "child_of",
            Self::Sibling => "sibling",
        }
    }
}

impl std::fmt::Display for BridgeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for BridgeType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "extends" => Ok(Self::Extends),
            "contradicts" => Ok(Self::Contradicts),
            "depends" => Ok(Self::Depends),
            "replaces" => Ok(Self::Replaces),
            "child_of" => Ok(Self::ChildOf),
            "sibling" => Ok(Self::Sibling),
            _ => Err(format!("Unknown bridge type: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BridgeStatus {
    Active,
    Weak,
    Invalid,
}

impl BridgeStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Weak => "weak",
            Self::Invalid => "invalid",
        }
    }
}

impl std::fmt::Display for BridgeStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for BridgeStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active" => Ok(Self::Active),
            "weak" => Ok(Self::Weak),
            "invalid" => Ok(Self::Invalid),
            _ => Err(format!("Unknown bridge status: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkBridge {
    pub id: String,
    pub source_id: String,
    pub target_id: String,
    pub relation_type: BridgeType,
    pub reason: String,
    pub shared_concepts: Vec<String>,
    pub weight: f64,
    pub confidence: f64,
    pub status: BridgeStatus,
    pub propagated_from: Option<String>,
    pub propagation_depth: u32,
    pub created_by: String,
    pub use_count: u32,
    pub created_at: DateTime<Utc>,
    pub last_reinforced: Option<DateTime<Utc>>,
}
