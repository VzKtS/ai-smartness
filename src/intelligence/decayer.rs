//! Decayer -- passive weight decay for threads and bridges.
//!
//! Does NOT delete or merge anything. Only reduces weights.
//! Suspends threads below THREAD_SUSPEND_THRESHOLD.
//! Cleans orphan bridges (both endpoints missing).

use crate::bridge::BridgeStatus;
use crate::constants::*;
use crate::thread::ThreadStatus;
use crate::AiResult;
use crate::storage::bridges::BridgeStorage;
use crate::storage::threads::ThreadStorage;
use chrono::Utc;
use rusqlite::Connection;

pub struct Decayer;

impl Decayer {
    /// Decay active thread/bridge weights. Returns count of affected threads.
    pub fn decay_active(conn: &Connection) -> AiResult<u32> {
        let now = Utc::now();
        let mut affected = 0u32;
        let mut suspended_count = 0u32;

        // 1. Decay thread weights
        let active = ThreadStorage::list_active(conn)?;
        for thread in &active {
            let age_days = (now - thread.last_active).num_hours() as f64 / 24.0;
            if age_days <= 0.0 {
                continue;
            }

            let half_life = effective_half_life(thread.importance);
            let decay_factor = 0.5f64.powf(age_days / half_life);
            let new_weight = (thread.weight * decay_factor).max(0.0);

            if (new_weight - thread.weight).abs() < 0.001 {
                continue;
            }

            ThreadStorage::update_weight(conn, &thread.id, new_weight)?;
            affected += 1;

            // Auto-suspend if below threshold
            if new_weight < THREAD_SUSPEND_THRESHOLD {
                tracing::warn!(thread_id = %thread.id, weight = new_weight, "Thread auto-suspended by decay");
                ThreadStorage::update_status(conn, &thread.id, ThreadStatus::Suspended)?;
                suspended_count += 1;
            }
        }

        // 2. Decay bridge weights
        let bridges = BridgeStorage::list_active(conn)?;
        for bridge in &bridges {
            let reference_time = bridge.last_reinforced.unwrap_or(bridge.created_at);
            let age_days = (now - reference_time).num_hours() as f64 / 24.0;
            if age_days <= 0.0 {
                continue;
            }

            let decay_factor = 0.5f64.powf(age_days / BRIDGE_HALF_LIFE);
            let new_weight = bridge.weight * decay_factor;

            if new_weight < BRIDGE_DEATH_THRESHOLD {
                BridgeStorage::update_status(conn, &bridge.id, BridgeStatus::Invalid)?;
            } else if new_weight < 0.3 {
                BridgeStorage::update_status(conn, &bridge.id, BridgeStatus::Weak)?;
                BridgeStorage::update_weight(conn, &bridge.id, new_weight)?;
            } else {
                BridgeStorage::update_weight(conn, &bridge.id, new_weight)?;
            }
        }

        // 3. Clean orphan bridges
        let orphans = BridgeStorage::scan_orphans(conn)?;
        if !orphans.is_empty() {
            tracing::debug!(orphan_count = orphans.len(), "Cleaning orphan bridges");
            let ids: Vec<String> = orphans.iter().map(|b| b.id.clone()).collect();
            BridgeStorage::delete_batch(conn, &ids)?;
        }

        tracing::info!(threads_affected = affected, threads_suspended = suspended_count, orphans_cleaned = orphans.len(), "Decay cycle complete");

        Ok(affected)
    }
}

/// Compute effective half-life based on importance.
/// Range: 0.75 days (disposable, importance=0) to 7.0 days (critical, importance=1).
fn effective_half_life(importance: f64) -> f64 {
    THREAD_MIN_HALF_LIFE
        + (importance.clamp(0.0, 1.0) * (THREAD_MAX_HALF_LIFE - THREAD_MIN_HALF_LIFE))
}
