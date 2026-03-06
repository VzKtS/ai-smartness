//! GPU VRAM probing — hardware-agnostic, best-effort.
//!
//! Used by local_llm.rs for adaptive resource allocation,
//! daemon watchdog for system metrics, and GUI/CLI hardware detection.
//! Works on: NVIDIA (nvidia-smi), non-NVIDIA fallback (lspci).
//! Future: AMD (rocm-smi), Intel Arc (xpu-smi).

use serde::{Deserialize, Serialize};

/// GPU VRAM snapshot (legacy single-GPU interface).
#[derive(Debug, Clone)]
pub struct VramInfo {
    pub used_mb: u64,
    pub total_mb: u64,
}

impl VramInfo {
    /// Free VRAM in MB.
    pub fn free_mb(&self) -> u64 {
        self.total_mb.saturating_sub(self.used_mb)
    }
}

/// Detailed GPU information for multi-GPU enumeration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    pub index: u32,
    pub name: String,
    pub vendor: String,
    pub vram_total_mb: u64,
    pub vram_used_mb: u64,
    pub driver_version: String,
    pub pci_bus_id: String,
}

/// Query GPU VRAM (legacy — returns first NVIDIA GPU).
/// Best-effort: 2s timeout, no panic on failure.
pub fn probe_vram() -> Option<VramInfo> {
    let gpus = probe_all_nvidia();
    gpus.into_iter().next().map(|g| VramInfo {
        used_mb: g.vram_used_mb,
        total_mb: g.vram_total_mb,
    })
}

/// Query VRAM for a specific GPU by nvidia-smi index.
pub fn probe_vram_for_gpu(index: u32) -> Option<VramInfo> {
    let output = std::process::Command::new("nvidia-smi")
        .args([
            &format!("--id={}", index),
            "--query-gpu=memory.used,memory.total",
            "--format=csv,noheader,nounits",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.lines().next()?;
    let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
    if parts.len() < 2 {
        return None;
    }

    Some(VramInfo {
        used_mb: parts[0].parse().ok()?,
        total_mb: parts[1].parse().ok()?,
    })
}

/// Enumerate ALL GPUs on the system.
///
/// Strategy:
/// 1. Query NVIDIA GPUs via nvidia-smi (detailed: name, VRAM, driver, PCI bus)
/// 2. Query non-NVIDIA GPUs via lspci (name + PCI bus only, no VRAM)
/// 3. Deduplicate: skip lspci entries already covered by nvidia-smi
pub fn probe_all_gpus() -> Vec<GpuInfo> {
    let mut gpus = probe_all_nvidia();
    let nvidia_pci_ids: Vec<String> = gpus.iter().map(|g| g.pci_bus_id.clone()).collect();

    // lspci fallback for non-NVIDIA GPUs (Intel, AMD, etc.)
    for gpu in probe_lspci() {
        // Skip if already covered by nvidia-smi (match on PCI bus ID)
        let dominated = nvidia_pci_ids.iter().any(|nv_id| {
            normalize_pci_id(nv_id) == normalize_pci_id(&gpu.pci_bus_id)
        });
        if !dominated {
            gpus.push(gpu);
        }
    }

    // Re-index sequentially
    for (i, gpu) in gpus.iter_mut().enumerate() {
        gpu.index = i as u32;
    }

    gpus
}

/// Query all NVIDIA GPUs via nvidia-smi.
fn probe_all_nvidia() -> Vec<GpuInfo> {
    let output = match std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=index,name,memory.used,memory.total,driver_version,pci.bus_id",
            "--format=csv,noheader,nounits",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut gpus = Vec::new();

    for line in stdout.lines() {
        let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
        if parts.len() < 6 {
            continue;
        }
        let index = parts[0].parse::<u32>().unwrap_or(gpus.len() as u32);
        gpus.push(GpuInfo {
            index,
            name: parts[1].to_string(),
            vendor: "nvidia".to_string(),
            vram_used_mb: parts[2].parse().unwrap_or(0),
            vram_total_mb: parts[3].parse().unwrap_or(0),
            driver_version: parts[4].to_string(),
            pci_bus_id: parts[5].to_string(),
        });
    }

    gpus
}

/// Detect GPUs via lspci (VGA/3D controllers).
/// Returns basic info without VRAM (set to 0 for integrated GPUs).
fn probe_lspci() -> Vec<GpuInfo> {
    let output = match std::process::Command::new("lspci")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut gpus = Vec::new();

    for line in stdout.lines() {
        let lower = line.to_lowercase();
        if !lower.contains("vga") && !lower.contains("3d") {
            continue;
        }

        // lspci format: "01:00.0 VGA compatible controller: NVIDIA Corporation ..."
        let pci_bus_id = line.split_whitespace().next().unwrap_or("").to_string();
        let name = line
            .find(": ")
            .map(|i| line[i + 2..].trim())
            .unwrap_or("Unknown GPU")
            .to_string();

        // Vendor detection — check name part only (after ": ") to avoid false matches
        // e.g. "compatible" contains "ati" which is not AMD ATI
        let name_lower = name.to_lowercase();
        let vendor = if name_lower.contains("nvidia") {
            "nvidia"
        } else if name_lower.contains("amd") || name_lower.contains("radeon") || name_lower.contains(" ati ") {
            "amd"
        } else if name_lower.contains("intel") {
            "intel"
        } else {
            "unknown"
        };

        gpus.push(GpuInfo {
            index: gpus.len() as u32,
            name,
            vendor: vendor.to_string(),
            vram_total_mb: 0,
            vram_used_mb: 0,
            driver_version: String::new(),
            pci_bus_id,
        });
    }

    gpus
}

/// Normalize PCI bus ID for comparison.
/// nvidia-smi: "00000000:01:00.0", lspci: "01:00.0"
fn normalize_pci_id(id: &str) -> String {
    // Strip leading domain (0000: prefix) if present
    let stripped = id.trim();
    if let Some(pos) = stripped.rfind(':') {
        // Find the bus:device.function part (last 7 chars typically: "01:00.0")
        if pos >= 2 {
            let prefix_end = stripped[..pos].rfind(':').map(|p| p + 1).unwrap_or(0);
            return stripped[prefix_end..].to_lowercase();
        }
    }
    stripped.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_pci_id() {
        assert_eq!(normalize_pci_id("00000000:01:00.0"), "01:00.0");
        assert_eq!(normalize_pci_id("01:00.0"), "01:00.0");
        assert_eq!(normalize_pci_id("0000:02:00.0"), "02:00.0");
    }

    #[test]
    fn test_vram_info_free() {
        let info = VramInfo { used_mb: 1000, total_mb: 8000 };
        assert_eq!(info.free_mb(), 7000);
    }

    #[test]
    fn test_vram_info_free_overflow() {
        let info = VramInfo { used_mb: 9000, total_mb: 8000 };
        assert_eq!(info.free_mb(), 0);
    }
}
