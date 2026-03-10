//! System resource watchdog — collects CPU, RAM, GPU metrics for beat telemetry.
//!
//! Called once per prune cycle (~5 min). Metrics are written to beat.json
//! so agents can read system state via MCP tools and make proactive decisions
//! (sleep on beat when GPU busy, wake when resources free).
//!
//! CPU/RAM: via sysinfo crate (cross-platform).
//! GPU VRAM: via nvidia-smi subprocess (best-effort, 2s timeout).
//! FDs/Threads: via /proc/self/status (Linux only).

pub use ai_smartness::storage::beat::SystemMetrics;

/// Collect system metrics. Non-blocking, best-effort — never panics.
pub fn collect() -> SystemMetrics {
    let (cpu, ram_used, ram_total, ram_available) = collect_cpu_ram();
    let (gpu_used, gpu_total) = collect_gpu_vram();
    let (fds, threads) = collect_proc_stats();

    SystemMetrics {
        cpu_usage_percent: cpu,
        ram_used_mb: ram_used,
        ram_total_mb: ram_total,
        ram_available_mb: ram_available,
        gpu_vram_used_mb: gpu_used,
        gpu_vram_total_mb: gpu_total,
        open_fds: fds,
        thread_count: threads,
    }
}

/// Collect CPU usage and RAM via sysinfo crate.
fn collect_cpu_ram() -> (f64, u64, u64, u64) {
    use sysinfo::{Pid, ProcessesToUpdate, System};

    let mut sys = System::new();
    sys.refresh_memory();

    let ram_total = sys.total_memory() / (1024 * 1024);
    let ram_available = sys.available_memory() / (1024 * 1024);

    // Process-level metrics — refresh only our own PID
    let pid = Pid::from_u32(std::process::id());
    sys.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);

    let (cpu, ram_used) = sys
        .process(pid)
        .map(|p| {
            (
                p.cpu_usage() as f64,
                p.memory() / (1024 * 1024),
            )
        })
        .unwrap_or((0.0, 0));

    (cpu, ram_used, ram_total, ram_available)
}

/// Collect GPU VRAM via shared vram_probe module.
/// Returns (used_mb, total_mb). Both None if no GPU detected.
fn collect_gpu_vram() -> (Option<u64>, Option<u64>) {
    match ai_smartness::processing::vram_probe::probe_vram() {
        Some(info) => (Some(info.used_mb), Some(info.total_mb)),
        None => (None, None),
    }
}

/// Collect open FDs and thread count from /proc/self (Linux only).
fn collect_proc_stats() -> (Option<u64>, Option<u64>) {
    #[cfg(target_os = "linux")]
    {
        let fds = std::fs::read_dir("/proc/self/fd")
            .ok()
            .map(|entries| entries.count() as u64);

        let threads = std::fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|status| {
                status.lines()
                    .find(|l| l.starts_with("Threads:"))
                    .and_then(|l| l.split_whitespace().nth(1))
                    .and_then(|n| n.parse::<u64>().ok())
            });

        (fds, threads)
    }

    #[cfg(not(target_os = "linux"))]
    {
        (None, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collect_returns_non_panic() {
        // Smoke test: collect() should never panic regardless of environment
        let metrics = collect();
        // RAM total should be non-zero on any system
        assert!(metrics.ram_total_mb > 0, "RAM total should be > 0");
    }

    #[test]
    fn test_system_metrics_serde_roundtrip() {
        let metrics = SystemMetrics {
            cpu_usage_percent: 42.5,
            ram_used_mb: 256,
            ram_total_mb: 16384,
            ram_available_mb: 8192,
            gpu_vram_used_mb: Some(2371),
            gpu_vram_total_mb: Some(3911),
            open_fds: Some(42),
            thread_count: Some(8),
        };

        let json = serde_json::to_string(&metrics).unwrap();
        let parsed: SystemMetrics = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.ram_total_mb, 16384);
        assert_eq!(parsed.gpu_vram_used_mb, Some(2371));
    }
}
