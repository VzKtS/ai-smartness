use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::processing::extractor::ExtractionMode;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ThreadStatus {
    Active,
    Suspended,
    Archived,
}

impl ThreadStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Suspended => "suspended",
            Self::Archived => "archived",
        }
    }
}

impl std::fmt::Display for ThreadStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for ThreadStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active" => Ok(Self::Active),
            "suspended" => Ok(Self::Suspended),
            "archived" => Ok(Self::Archived),
            _ => Err(format!("Unknown thread status: {}", s)),
        }
    }
}

impl Default for ThreadStatus {
    fn default() -> Self {
        Self::Active
    }
}

/// Type d'origine du thread — mappe sur les 7 templates d'extraction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OriginType {
    /// Contenu de prompt/conversation
    Prompt,
    /// Lecture de fichier
    FileRead,
    /// Ecriture de fichier
    FileWrite,
    /// Execution de tache
    Task,
    /// Fetch web/API
    Fetch,
    /// Reponse systeme
    Response,
    /// Commande CLI
    Command,
    /// Issue d'un split de thread
    Split,
    /// Reactivation d'un thread archive
    Reactivation,
}

impl OriginType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Prompt => "prompt",
            Self::FileRead => "file_read",
            Self::FileWrite => "file_write",
            Self::Task => "task",
            Self::Fetch => "fetch",
            Self::Response => "response",
            Self::Command => "command",
            Self::Split => "split",
            Self::Reactivation => "reactivation",
        }
    }
}

impl std::str::FromStr for OriginType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "prompt" | "Prompt" => Ok(Self::Prompt),
            "file_read" | "Read" => Ok(Self::FileRead),
            "file_write" | "Write" | "Edit" => Ok(Self::FileWrite),
            "task" | "Task" => Ok(Self::Task),
            "fetch" | "Fetch" | "WebFetch" | "WebSearch" => Ok(Self::Fetch),
            "response" | "Response" => Ok(Self::Response),
            "command" | "Command" | "Bash" => Ok(Self::Command),
            "split" | "Split" => Ok(Self::Split),
            "reactivation" | "Reactivation" => Ok(Self::Reactivation),
            _ => Err(format!("Unknown origin type: {}", s)),
        }
    }
}

impl Default for OriginType {
    fn default() -> Self {
        Self::Prompt
    }
}

/// Contexte de travail actif — fichiers, actions, goal en cours.
/// Decay automatique: freshness_factor() base sur l'age.
/// Nettoye par le daemon prune loop quand expired (> 24h).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkContext {
    /// Fichiers manipules dans cette session de travail.
    pub files: Vec<String>,
    /// Actions effectuees (read, write, exec, etc.).
    pub actions: Vec<String>,
    /// Objectif en cours (optionnel, extrait par le LLM).
    pub goal: Option<String>,
    /// Timestamp de derniere mise a jour.
    pub updated_at: DateTime<Utc>,
}

impl WorkContext {
    /// Facteur de fraicheur: < 2h = 1.0, 2-8h = 0.5, 8-24h = 0.1, > 24h = 0.0
    pub fn freshness_factor(&self) -> f64 {
        let age_hours = (Utc::now() - self.updated_at).num_minutes() as f64 / 60.0;
        if age_hours < 2.0 { 1.0 }
        else if age_hours < 8.0 { 0.5 }
        else if age_hours < 24.0 { 0.1 }
        else { 0.0 }
    }

    pub fn is_expired(&self) -> bool {
        self.freshness_factor() == 0.0
    }

    pub fn importance_boost(&self) -> f64 {
        let file_factor = if !self.files.is_empty() { 0.15 } else { 0.0 };
        let action_factor = if !self.actions.is_empty() { 0.10 } else { 0.0 };
        (file_factor + action_factor) * self.freshness_factor()
    }
}

/// Statistiques d'injection — tracking "ce thread a ete injecte X fois sans etre utilise".
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InjectionStats {
    pub injection_count: u32,
    pub used_count: u32,
    pub last_injected_at: Option<String>,
    pub last_used_at: Option<String>,
}

impl InjectionStats {
    pub fn usage_ratio(&self) -> f64 {
        if self.injection_count == 0 { return 1.0; }
        self.used_count as f64 / self.injection_count as f64
    }

    pub fn should_decay(&self) -> bool {
        self.injection_count >= 5 && self.usage_ratio() < 0.2
    }

    pub fn compute_relevance_penalty(&self) -> f64 {
        if !self.should_decay() { return 0.0; }
        ((1.0 - self.usage_ratio()) * 0.3).min(0.3)
    }

    pub fn record_injection(&mut self) {
        self.injection_count += 1;
        self.last_injected_at = Some(Utc::now().to_rfc3339());
    }

    pub fn record_usage(&mut self) {
        self.used_count += 1;
        self.last_used_at = Some(Utc::now().to_rfc3339());
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thread {
    pub id: String,
    pub title: String,
    pub status: ThreadStatus,
    pub weight: f64,
    pub importance: f64,
    pub importance_manually_set: bool,
    pub created_at: DateTime<Utc>,
    pub last_active: DateTime<Utc>,
    pub activation_count: u32,
    pub split_locked: bool,
    pub split_locked_until: Option<DateTime<Utc>>,
    pub origin_type: OriginType,
    pub drift_history: Vec<String>,
    pub parent_id: Option<String>,
    pub child_ids: Vec<String>,
    pub summary: Option<String>,
    pub topics: Vec<String>,
    pub tags: Vec<String>,
    pub labels: Vec<String>,
    /// Semantic explosion concepts (LLM-generated synonyms, hypernyms, related domains).
    pub concepts: Vec<String>,
    /// Embedding vector (f32) — ONNX all-MiniLM-L6-v2 or TF-IDF hash vector.
    pub embedding: Option<Vec<f32>>,
    pub relevance_score: f64,
    /// Ratings history — JSON array of {useful: bool, timestamp: String}.
    pub ratings: Vec<serde_json::Value>,
    /// Structured work context with staleness decay.
    pub work_context: Option<WorkContext>,
    /// Injection tracking stats.
    pub injection_stats: Option<InjectionStats>,
    /// How this thread was extracted: verbatim or full LLM extract.
    #[serde(default)]
    pub extraction_mode: ExtractionMode,
    /// True if any message was created from truncated input (sticky flag).
    #[serde(default)]
    pub has_truncated_origin: bool,
    /// Previous thread ID in the logical reasoning chain (continuity edge).
    #[serde(default)]
    pub continuity_parent_id: Option<String>,
    /// Subject coherence with continuity parent: 0.0 (new topic) to 1.0 (same topic).
    #[serde(default)]
    pub subject_coherence: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadMessage {
    pub thread_id: String,
    pub msg_id: String,
    pub content: String,
    pub source: String,
    pub source_type: String,
    pub timestamp: DateTime<Utc>,
    pub metadata: serde_json::Value,
    /// True if the original content was truncated (> 2000 chars).
    #[serde(default)]
    pub is_truncated: bool,
    /// Thread ID entering this changelog waypoint (continuity chain).
    #[serde(default)]
    pub continuity_from: Option<String>,
    /// Thread ID leaving this changelog waypoint (backfilled).
    #[serde(default)]
    pub continuity_to: Option<String>,
}
