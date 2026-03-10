//! Hardware detection — shared between GUI, CLI, and MCP.
//!
//! Enumerates CPU, RAM, and all GPUs in a single call.

use serde::{Deserialize, Serialize};
use super::vram_probe;

/// CPU information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuInfo {
    pub model: String,
    pub cores: u32,
    pub threads: u32,
}

/// RAM information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RamInfo {
    pub total_mb: u64,
    pub available_mb: u64,
}

/// Complete hardware snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareInfo {
    pub cpu: CpuInfo,
    pub ram: RamInfo,
    pub gpus: Vec<vram_probe::GpuInfo>,
}

/// Detect all hardware. Best-effort, never panics.
pub fn detect() -> HardwareInfo {
    use sysinfo::System;

    let mut sys = System::new();
    sys.refresh_cpu_all();
    sys.refresh_memory();

    let cpus = sys.cpus();
    let cpu = CpuInfo {
        model: cpus.first().map(|c| c.brand().to_string()).unwrap_or_else(|| "Unknown".to_string()),
        cores: sys.physical_core_count().unwrap_or(0) as u32,
        threads: cpus.len() as u32,
    };

    let ram = RamInfo {
        total_mb: sys.total_memory() / (1024 * 1024),
        available_mb: sys.available_memory() / (1024 * 1024),
    };

    let gpus = vram_probe::probe_all_gpus();

    HardwareInfo { cpu, ram, gpus }
}
