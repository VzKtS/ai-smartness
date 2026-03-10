use anyhow::{Context, Result};
use ai_smartness::storage::path_utils;

/// Start the CLI controller (integrated with daemon).
/// The controller runs as a thread within the global daemon.
pub fn start(interval: Option<u64>) -> Result<()> {
    if let Some(ms) = interval {
        if ms < 100 || ms > 60_000 {
            anyhow::bail!(
                "Poll interval must be between 100ms and 60s (requested: {}ms)",
                ms
            );
        }
    }

    // Check if daemon is already running
    let data_dir = path_utils::data_dir();
    let pid_file = data_dir.join("daemon.pid");
    if pid_file.exists() {
        if let Ok(pid_str) = std::fs::read_to_string(&pid_file) {
            let pid_str = pid_str.trim();
            if is_process_alive(pid_str) {
                println!("Controller active (daemon PID {})", pid_str);
                if interval.is_some() {
                    println!("Note: --interval is ignored when daemon is already running.");
                    println!("To change interval, stop and restart the daemon.");
                }
                return Ok(());
            }
            // Stale PID file
            let _ = std::fs::remove_file(&pid_file);
        }
    }

    // Start global daemon (which includes controller loop)
    let self_bin = std::env::current_exe()
        .context("Failed to get current exe")?;

    let mut child = std::process::Command::new(&self_bin)
        .args(["daemon", "run-foreground"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .context("Failed to start daemon (which includes controller)")?;

    println!("Controller started (daemon PID {})", child.id());

    // Reaper thread: wait for the child to prevent zombie processes
    std::thread::spawn(move || {
        let _ = child.wait();
    });

    Ok(())
}

/// Stop the CLI controller (which stops the daemon).
pub fn stop() -> Result<()> {
    let data_dir = path_utils::data_dir();

    // Try IPC shutdown first (preferred method)
    match ai_smartness::processing::daemon_ipc_client::shutdown() {
        Ok(_) => {
            println!("Controller shutdown requested via IPC");
            // Wait briefly for PID file to disappear
            let pid_file = data_dir.join("daemon.pid");
            for _ in 0..10 {
                if !pid_file.exists() {
                    println!("Controller stopped");
                    return Ok(());
                }
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
            println!("Controller may still be shutting down");
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
                    // Clean up socket
                    let socket_file = data_dir.join("processor.sock");
                    let _ = std::fs::remove_file(&socket_file);
                    println!("Controller stopped (killed PID {})", pid_str);
                    return Ok(());
                }
            }
            println!("Controller is not running");
        }
    }

    Ok(())
}

/// Show controller status.
pub fn status() -> Result<()> {
    let data_dir = path_utils::data_dir();

    // Check PID file
    let pid_file = data_dir.join("daemon.pid");
    if !pid_file.exists() {
        println!("Controller: not running");
        return Ok(());
    }

    let pid_str = std::fs::read_to_string(&pid_file)
        .context("Failed to read PID file")?;
    let pid_str = pid_str.trim();

    if !is_process_alive(pid_str) {
        println!("Controller: stale PID file (process {} not found)", pid_str);
        return Ok(());
    }

    // Try IPC status
    match ai_smartness::processing::daemon_ipc_client::daemon_status() {
        Ok(resp) => {
            println!("Controller: active (daemon PID {})", pid_str);
            if let Some(uptime) = resp.get("uptime_secs") {
                println!("Uptime: {}s", uptime);
            }
            if let Some(agents) = resp.get("active_agents") {
                println!("Monitoring agents: {}", agents);
            }
        }
        Err(_) => {
            println!("Controller: active (daemon PID {}) but IPC unavailable", pid_str);
        }
    }

    Ok(())
}

/// Check if a process is alive via kill(pid, 0).
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

/// Kill a process by PID.
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
