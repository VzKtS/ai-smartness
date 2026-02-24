use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── Coordination Mode ──

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CoordinationMode {
    Autonomous,
    Supervised,
    Coordinator,
}

impl CoordinationMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Autonomous => "autonomous",
            Self::Supervised => "supervised",
            Self::Coordinator => "coordinator",
        }
    }
}

impl std::fmt::Display for CoordinationMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for CoordinationMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "autonomous" => Ok(Self::Autonomous),
            "supervised" => Ok(Self::Supervised),
            "coordinator" => Ok(Self::Coordinator),
            _ => Err(format!("Unknown coordination mode: {}", s)),
        }
    }
}

impl Default for CoordinationMode {
    fn default() -> Self {
        Self::Autonomous
    }
}

// ── Agent Status ──

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentStatus {
    Active,
    Idle,
    Offline,
}

impl AgentStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Idle => "idle",
            Self::Offline => "offline",
        }
    }
}

impl std::fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for AgentStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active" | "available" => Ok(Self::Active),
            "idle" | "busy" => Ok(Self::Idle),
            "offline" => Ok(Self::Offline),
            _ => Err(format!("Unknown agent status: {}", s)),
        }
    }
}

impl Default for AgentStatus {
    fn default() -> Self {
        Self::Active
    }
}

// ── Thread Mode ──

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ThreadMode {
    Light,
    Normal,
    Heavy,
    Max,
}

impl ThreadMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Normal => "normal",
            Self::Heavy => "heavy",
            Self::Max => "max",
        }
    }

    pub fn quota(&self) -> usize {
        match self {
            Self::Light => 15,
            Self::Normal => 50,
            Self::Heavy => 100,
            Self::Max => 200,
        }
    }
}

impl std::fmt::Display for ThreadMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for ThreadMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "light" => Ok(Self::Light),
            "normal" => Ok(Self::Normal),
            "heavy" => Ok(Self::Heavy),
            "max" => Ok(Self::Max),
            _ => Err(format!("Unknown thread mode: {}", s)),
        }
    }
}

impl Default for ThreadMode {
    fn default() -> Self {
        Self::Normal
    }
}

// ── Agent (enrichi V2) ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: String,
    pub project_hash: String,
    pub name: String,
    pub description: String,
    pub role: String,
    pub capabilities: Vec<String>,
    pub status: AgentStatus,
    pub last_seen: DateTime<Utc>,
    pub registered_at: DateTime<Utc>,

    // --- Hierarchy ---
    pub supervisor_id: Option<String>,
    pub coordination_mode: CoordinationMode,
    pub team: Option<String>,

    // --- Specializations ---
    pub specializations: Vec<String>,

    // --- Thread quota ---
    pub thread_mode: ThreadMode,

    // --- Activity tracking ---
    pub current_activity: String,

    // --- Topology ---
    pub report_to: Option<String>,
    pub custom_role: Option<String>,

    // --- Workspace isolation ---
    pub workspace_path: String,

    // --- Permissions ---
    pub full_permissions: bool,
}

// ── Role Templates ──

pub struct AgentRoleTemplate {
    pub role: &'static str,
    pub default_mode: CoordinationMode,
    pub default_capabilities: &'static [&'static str],
    pub description_hint: &'static str,
}

pub const ROLE_TEMPLATES: &[AgentRoleTemplate] = &[
    AgentRoleTemplate {
        role: "programmer",
        default_mode: CoordinationMode::Supervised,
        default_capabilities: &["code", "test", "debug", "refactor"],
        description_hint: "Writes and maintains code",
    },
    AgentRoleTemplate {
        role: "coordinator",
        default_mode: CoordinationMode::Coordinator,
        default_capabilities: &["plan", "delegate", "review", "merge"],
        description_hint: "Coordinates other agents, delegates tasks",
    },
    AgentRoleTemplate {
        role: "reviewer",
        default_mode: CoordinationMode::Supervised,
        default_capabilities: &["review", "audit", "test"],
        description_hint: "Reviews code and provides feedback",
    },
    AgentRoleTemplate {
        role: "researcher",
        default_mode: CoordinationMode::Autonomous,
        default_capabilities: &["search", "analyze", "document"],
        description_hint: "Researches topics and gathers information",
    },
    AgentRoleTemplate {
        role: "architect",
        default_mode: CoordinationMode::Coordinator,
        default_capabilities: &["design", "plan", "review", "delegate"],
        description_hint: "Designs system architecture and coordinates implementation",
    },
];

/// Lookup template by role name. Returns None for custom roles.
pub fn role_template(role: &str) -> Option<&'static AgentRoleTemplate> {
    ROLE_TEMPLATES.iter().find(|t| t.role == role)
}

// ── Task Types ──

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskPriority {
    Low,
    Normal,
    High,
    Critical,
}

impl TaskPriority {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Normal => "normal",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

impl std::fmt::Display for TaskPriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for TaskPriority {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "low" => Ok(Self::Low),
            "normal" => Ok(Self::Normal),
            "high" => Ok(Self::High),
            "critical" => Ok(Self::Critical),
            _ => Err(format!("Unknown task priority: {}", s)),
        }
    }
}

impl Default for TaskPriority {
    fn default() -> Self {
        Self::Normal
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    Blocked,
}

impl TaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::Blocked => "blocked",
        }
    }
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for TaskStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "in_progress" => Ok(Self::InProgress),
            "completed" => Ok(Self::Completed),
            "blocked" => Ok(Self::Blocked),
            _ => Err(format!("Unknown task status: {}", s)),
        }
    }
}

impl Default for TaskStatus {
    fn default() -> Self {
        Self::Pending
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTask {
    pub id: String,
    pub project_hash: String,
    pub assigned_to: String,
    pub assigned_by: String,
    pub title: String,
    pub description: String,
    pub priority: TaskPriority,
    pub status: TaskStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deadline: Option<DateTime<Utc>>,
    pub dependencies: Vec<String>,
    pub result: Option<String>,
}
