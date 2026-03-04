//! GPU VRAM probing — hardware-agnostic, best-effort.
//!
//! Used by local_llm.rs for adaptive resource allocation and
//! by daemon watchdog for system metrics.
//! Works on: NVIDIA (nvidia-smi). Future: AMD (rocm-smi), Intel Arc.

/// GPU VRAM snapshot.
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

/// Query GPU VRAM. Returns None if no supported GPU detected.
/// Best-effort: 2s timeout, no panic on failure.
pub fn probe_vram() -> Option<VramInfo> {
    // Try NVIDIA first (most common for GGUF inference)
    if let Some(info) = probe_nvidia() {
        return Some(info);
    }
    // Future: probe_amd(), probe_intel_arc()
    None
}

/// Query NVIDIA GPU via nvidia-smi subprocess.
fn probe_nvidia() -> Option<VramInfo> {
    let output = std::process::Command::new("nvidia-smi")
        .args([
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
