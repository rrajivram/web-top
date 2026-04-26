use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct MetricsSnapshot {
    pub timestamp: u64,
    pub cpu_cores: Vec<CpuCore>,
    pub gpu_cores: Vec<GpuCore>,
    pub memory: MemStats,
    pub processes: Vec<ProcessInfo>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CpuCore {
    pub id: usize,
    pub usage: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GpuCore {
    pub name: String,
    pub usage: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct MemStats {
    pub total: u64,
    pub used: u64,
    pub available: u64,
    pub swap_total: u64,
    pub swap_used: u64,
    /// macOS "wired" pages (kernel + pinned)
    pub wired: u64,
    /// macOS file-backed cached pages
    pub cached: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub user: String,
    pub cpu_usage: f32,
    pub memory_bytes: u64,
    pub gpu_usage: Option<f32>,
    /// Has an active GPU context (IOAccelerator UserClient)
    pub gpu_active: bool,
}
