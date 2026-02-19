#[cfg(feature = "gui")]
mod commands;

/// Launch the Tauri GUI dashboard.
///
/// Requires the `gui` feature flag:
/// `cargo build --features gui`
///
/// System dependencies (Linux):
/// `sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev`
pub fn launch() {
    #[cfg(feature = "gui")]
    {
        tauri::Builder::default()
            .invoke_handler(tauri::generate_handler![
                commands::get_dashboard,
                commands::get_project_overview,
                commands::get_threads,
                commands::get_settings,
                commands::save_settings,
                commands::daemon_status,
                commands::daemon_start,
                commands::daemon_stop,
                commands::search_threads,
                commands::get_bridges,
                commands::list_projects,
                commands::add_project,
                commands::update_project,
                commands::remove_project,
                commands::list_agents,
                commands::add_agent,
                commands::update_agent,
                commands::remove_agent,
                commands::get_hierarchy,
                commands::open_debug_window,
                commands::get_debug_logs,
                commands::get_daemon_settings,
                commands::save_daemon_settings,
                commands::get_global_debug_logs,
                commands::get_system_resources,
                commands::get_thread_detail,
                commands::delete_thread,
                commands::purge_agent_db,
                commands::get_user_profile,
                commands::save_user_profile,
                commands::get_backup_settings,
                commands::save_backup_settings,
                commands::trigger_backup,
                commands::list_backups,
                commands::restore_backup,
                commands::delete_backup,
                commands::reindex_agent,
                commands::check_update,
            ])
            .run(tauri::generate_context!())
            .expect("Failed to launch AI Smartness GUI");
    }

    #[cfg(not(feature = "gui"))]
    {
        eprintln!("GUI not available. Rebuild with: cargo build --features gui");
        eprintln!("System deps (Linux): sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev");
        std::process::exit(1);
    }
}
