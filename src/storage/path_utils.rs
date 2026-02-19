use std::path::PathBuf;

/// Retourne le repertoire de donnees centralise cross-platform.
/// Linux: ~/.config/ai-smartness/
/// macOS: ~/Library/Application Support/ai-smartness/
/// Windows: %APPDATA%/ai-smartness/
pub fn data_dir() -> PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".config")
    });
    base.join("ai-smartness")
}

/// Retourne le repertoire des projets: {data_dir}/projects/
pub fn projects_dir() -> PathBuf {
    data_dir().join("projects")
}

/// Retourne le repertoire d'un projet specifique: {data_dir}/projects/{hash}/
pub fn project_dir(project_hash: &str) -> PathBuf {
    projects_dir().join(project_hash)
}

/// Retourne le chemin de la DB d'un agent: {data_dir}/projects/{hash}/agents/{agent_id}.db
pub fn agent_db_path(project_hash: &str, agent_id: &str) -> PathBuf {
    project_dir(project_hash)
        .join("agents")
        .join(format!("{}.db", agent_id))
}

/// Retourne le repertoire de donnees d'un agent: {data_dir}/projects/{hash}/agents/{agent_id}/
/// Pour les fichiers non-DB: beat.json, session_state.json, user_profile.json, pins.json, etc.
pub fn agent_data_dir(project_hash: &str, agent_id: &str) -> PathBuf {
    project_dir(project_hash)
        .join("agents")
        .join(agent_id)
}

/// Retourne le chemin de shared.db: {data_dir}/projects/{hash}/shared.db
pub fn shared_db_path(project_hash: &str) -> PathBuf {
    project_dir(project_hash).join("shared.db")
}

/// Retourne le chemin de registry.db: {data_dir}/registry.db
pub fn registry_db_path() -> PathBuf {
    data_dir().join("registry.db")
}

/// Retourne le repertoire des wake signals: {data_dir}/wake_signals/
pub fn wake_signals_dir() -> PathBuf {
    data_dir().join("wake_signals")
}

/// Retourne le chemin du wake signal d'un agent: {data_dir}/wake_signals/{agent_id}.signal
pub fn wake_signal_path(agent_id: &str) -> PathBuf {
    wake_signals_dir().join(format!("{}.signal", agent_id))
}

/// Retourne le chemin du fichier de session agent: {data_dir}/projects/{hash}/session_agent
/// Contient l'ID de l'agent selectionne pour la session courante.
pub fn agent_session_path(project_hash: &str) -> PathBuf {
    project_dir(project_hash).join("session_agent")
}

/// Retourne le repertoire des fichiers agent per-session: {data_dir}/projects/{hash}/session_agents/
pub fn session_agents_dir(project_hash: &str) -> PathBuf {
    project_dir(project_hash).join("session_agents")
}

/// Retourne le chemin d'un fichier agent per-session: {data_dir}/projects/{hash}/session_agents/{session_id}
/// Chaque session Claude Code a un session_id unique, ce qui permet l'isolation multi-panel.
pub fn per_session_agent_path(project_hash: &str, session_id: &str) -> PathBuf {
    session_agents_dir(project_hash).join(session_id)
}

/// Wrapper avec canonicalize() pour project_hash (I/O, pas WASM-safe).
/// 1. canonicalize() le chemin (resoudre symlinks)
/// 2. Appelle crate::id_gen::hash_path_string() (pur)
pub fn project_hash(path: &std::path::Path) -> Result<String, std::io::Error> {
    let canonical = path.canonicalize()?;
    let canonical_str = canonical.to_string_lossy();
    Ok(crate::id_gen::hash_path_string(&canonical_str))
}

/// Expand ~ to home directory in paths.
pub fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/") || path == "~" {
        if let Some(home) = dirs::home_dir() {
            return path.replacen('~', &home.to_string_lossy(), 1);
        }
    }
    path.to_string()
}
