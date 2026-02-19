use anyhow::{Context, Result};
use ai_smartness::processing::daemon_ipc_client;
use ai_smartness::storage::path_utils;

/// Start the global daemon (no project/agent args — serves all).
pub fn start() -> Result<()> {
    // Check if already running
    let data_dir = path_utils::data_dir();
    let pid_file = data_dir.join("daemon.pid");
    if pid_file.exists() {
        if let Ok(pid_str) = std::fs::read_to_string(&pid_file) {
            let pid_str = pid_str.trim();
            if is_process_alive(pid_str) {
                println!("Daemon already running (PID {})", pid_str);
                return Ok(());
            }
            // Stale PID file
            let _ = std::fs::remove_file(&pid_file);
        }
    }

    // Spawn global daemon as a child process using the same binary
    let self_bin = std::env::current_exe()
        .context("Failed to get current exe")?;

    let mut child = std::process::Command::new(&self_bin)
        .args(["daemon", "run-foreground"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .context("Failed to start daemon")?;

    println!("Global daemon started (PID {})", child.id());

    // Reaper thread: wait for the child to prevent zombie processes.
    // When the parent is long-lived (GUI, MCP server), without this
    // the daemon child becomes a zombie on exit.
    std::thread::spawn(move || {
        let _ = child.wait();
    });

    Ok(())
}

pub fn stop() -> Result<()> {
    let data_dir = path_utils::data_dir();

    // Try IPC shutdown first
    match daemon_ipc_client::shutdown() {
        Ok(_) => {
            println!("Daemon shutdown requested via IPC");
            // Wait briefly for PID file to disappear
            let pid_file = data_dir.join("daemon.pid");
            for _ in 0..10 {
                if !pid_file.exists() {
                    println!("Daemon stopped");
                    return Ok(());
                }
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
            println!("Daemon may still be shutting down");
            return Ok(());
        }
        Err(_) => {
            // IPC failed, try PID file
            let pid_file = data_dir.join("daemon.pid");
            if pid_file.exists() {
                if let Ok(pid_str) = std::fs::read_to_string(&pid_file) {
                    let pid_str = pid_str.trim();
                    kill_process(pid_str);
                    let _ = std::fs::remove_file(&pid_file);
                    // Clean up socket in case the killed daemon didn't get to
                    let socket_file = data_dir.join("processor.sock");
                    let _ = std::fs::remove_file(&socket_file);
                    println!("Daemon stopped (killed PID {})", pid_str);
                    return Ok(());
                }
            }
            println!("Daemon is not running");
        }
    }

    Ok(())
}

pub fn status() -> Result<()> {
    let data_dir = path_utils::data_dir();

    // Check PID file
    let pid_file = data_dir.join("daemon.pid");
    if !pid_file.exists() {
        println!("Daemon: not running");
        return Ok(());
    }

    let pid_str = std::fs::read_to_string(&pid_file)
        .context("Failed to read PID file")?;
    let pid_str = pid_str.trim();

    if !is_process_alive(pid_str) {
        println!("Daemon: stale PID file (process {} not found)", pid_str);
        return Ok(());
    }

    // Try IPC status (global)
    match daemon_ipc_client::daemon_status() {
        Ok(resp) => {
            println!("Daemon: running (PID {}) [global mode]", pid_str);
            if let Some(uptime) = resp.get("uptime_secs") {
                println!("Uptime: {}s", uptime);
            }
            if let Some(agents) = resp.get("active_agents") {
                println!("Active agents: {}", agents);
            }
            if let Some(pool) = resp.get("pool") {
                if let Some(active) = pool.get("active") {
                    println!("Pool: {} active connections", active);
                }
            }
        }
        Err(_) => {
            println!("Daemon: running (PID {}) but IPC unavailable", pid_str);
        }
    }

    Ok(())
}

fn is_process_alive(pid_str: &str) -> bool {
    #[cfg(unix)]
    {
        if let Ok(pid) = pid_str.parse::<i32>() {
            extern "C" {
                fn kill(pid: i32, sig: i32) -> i32;
            }
            unsafe { kill(pid, 0) == 0 }
        } else {
            false
        }
    }
    #[cfg(not(unix))]
    {
        let _ = pid_str;
        true
    }
}

fn kill_process(pid_str: &str) {
    #[cfg(unix)]
    {
        if let Ok(pid) = pid_str.parse::<i32>() {
            extern "C" {
                fn kill(pid: i32, sig: i32) -> i32;
            }
            unsafe {
                kill(pid, 15); // SIGTERM
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", pid_str, "/F"])
            .output();
    }
}
