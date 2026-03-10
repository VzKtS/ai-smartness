// Tauri commands — sera implemente avec les #[tauri::command] macros.
// Stubs pour l'instant.

use crate::AiResult;

/// Get dashboard data (daemon status, thread counts, health)
pub fn cmd_get_dashboard() -> AiResult<serde_json::Value> { todo!() }

/// Get settings (GuardianConfig, quotas)
pub fn cmd_get_settings() -> AiResult<serde_json::Value> { todo!() }

/// Update guardian settings (per-system: embedding mode, thresholds)
pub fn cmd_update_guardian() -> AiResult<serde_json::Value> { todo!() }

/// Get thread list with filters
pub fn cmd_get_threads() -> AiResult<serde_json::Value> { todo!() }
