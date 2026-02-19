use serde::{Deserialize, Serialize};

/// Vue dashboard pour le frontend Tauri.
#[derive(Serialize, Deserialize)]
pub struct DashboardView {
    pub daemon_status: DaemonStatusView,
    pub thread_counts: ThreadCountsView,
    pub bridge_count: usize,
    pub health: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
pub struct DaemonStatusView {
    pub running: bool,
    pub pid: Option<u32>,
    pub uptime_secs: u64,
    pub version: String,
}

#[derive(Serialize, Deserialize)]
pub struct ThreadCountsView {
    pub active: usize,
    pub suspended: usize,
    pub archived: usize,
    pub quota: usize,
    pub usage_percent: f64,
}

/// Vue settings Guardian avec tous les sous-systemes.
#[derive(Serialize, Deserialize)]
pub struct GuardianSettingsView {
    pub enabled: bool,
    pub tasks: Vec<GuardianTaskView>,
    pub gossip: GossipSettingsView,
    pub recall: RecallSettingsView,
    pub thread_matching: ThreadMatchingSettingsView,
    pub cache_enabled: bool,
    pub pattern_learning: bool,
    pub usage_tracking: bool,
    pub fallback_on_failure: bool,
    pub active_alerts: Vec<serde_json::Value>,
    pub alert_thresholds: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
pub struct GuardianTaskView {
    pub name: String,
    pub model: String,
    pub timeout_secs: u64,
    pub enabled: bool,
    pub failure_mode: String,
    pub call_count: u64,
    pub success_rate: f64,
}

#[derive(Serialize, Deserialize)]
pub struct EmbeddingSettingsView {
    pub mode: String,
    pub onnx_threshold: f64,
    pub tfidf_threshold: f64,
    pub onnx_available: bool,
}

#[derive(Serialize, Deserialize)]
pub struct GossipSettingsView {
    pub embedding: EmbeddingSettingsView,
    pub topic_overlap_enabled: bool,
    pub topic_overlap_min_shared: usize,
    pub batch_size: usize,
}

#[derive(Serialize, Deserialize)]
pub struct RecallSettingsView {
    pub embedding: EmbeddingSettingsView,
    pub max_results: usize,
    pub focus_boost: f64,
}

#[derive(Serialize, Deserialize)]
pub struct ThreadMatchingSettingsView {
    pub mode: String,
    pub embedding: EmbeddingSettingsView,
}
