//! AI Smartness — Persistent cognitive memory for AI agents.
//!
//! Single-crate library providing storage, processing, intelligence,
//! agent registry, and guardcode for autonomous AI memory management.

// Foundation types (from ai-common)
pub mod id_gen;
pub mod time_utils;
pub mod project_registry;

// Core types (from ai-core)
pub mod agent;
pub mod bridge;
pub mod config;
pub mod constants;
pub mod error;
pub mod message;
pub mod provider;
pub mod session;
pub mod shared;
pub mod thread;
pub mod user_profile;

// Sub-systems
pub mod storage;
pub mod processing;
pub mod intelligence;
pub mod guardcode;
pub mod registry;
pub mod admin;
pub mod network;
pub mod tracing_init;
pub mod hook_setup;
pub mod config_sync;

// Re-exports for convenience
pub use error::{AiError, AiResult};

use serde::{Deserialize, Serialize};

/// Health status injecte par ai-hook
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub sqlite_ok: bool,
    pub daemon_alive: bool,
    pub daemon_pid: Option<u32>,
    pub wake_signals_ok: bool,
    pub overall: HealthLevel,
    pub repairs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum HealthLevel {
    Ok,
    Healed,
    Degraded,
    Critical,
}
