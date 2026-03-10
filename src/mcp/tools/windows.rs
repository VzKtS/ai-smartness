use ai_smartness::AiResult;
use rusqlite::params;

use super::ToolContext;

/// Open a new VSCode window on the current project.
pub fn handle_windows(
    _params: &serde_json::Value,
    ctx: &ToolContext,
) -> AiResult<serde_json::Value> {
    // Resolve project path from registry
    let project_path: String = ctx
        .registry_conn
        .query_row(
            "SELECT path FROM projects WHERE hash = ?1",
            params![ctx.project_hash],
            |row| row.get(0),
        )
        .map_err(|e| {
            ai_smartness::AiError::Storage(format!(
                "Could not resolve project path for hash {}: {}",
                ctx.project_hash, e
            ))
        })?;

    // Spawn `code --new-window <path>` so a second window opens
    // even if one is already open on this project.
    match std::process::Command::new("code")
        .arg("--new-window")
        .arg(&project_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(_) => Ok(serde_json::json!({
            "opened": true,
            "path": project_path,
            "note": "New VSCode window opened"
        })),
        Err(e) => Err(ai_smartness::AiError::InvalidInput(format!(
            "Failed to launch `code {}`: {}",
            project_path, e
        ))),
    }
}
