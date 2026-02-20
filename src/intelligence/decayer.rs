//! Decayer -- passive weight decay for threads and bridges.
//!
//! Does NOT delete or merge anything. Only reduces weights.
//! Suspends threads below DecayConfig.thread_suspend_threshold.
//! Cleans orphan bridges (both endpoints missing).

use crate::bridge::BridgeStatus;
use crate::config::DecayConfig;
use crate::thread::ThreadStatus;
use crate::AiResult;
use crate::storage::bridges::BridgeStorage;
use crate::storage::threads::ThreadStorage;
use chrono::Utc;
use rusqlite::Connection;

pub struct Decayer;

impl Decayer {
    /// Decay active thread/bridge weights. Returns count of affected threads.
    pub fn decay_active(conn: &Connection, cfg: &DecayConfig) -> AiResult<u32> {
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

            let base_half_life = effective_half_life(thread.importance, cfg);
            // Orphan acceleration: threads not re-injected decay faster.
            // Every orphan_halving_hours without contact halves the effective half-life.
            let orphan_hours = age_days * 24.0;
            let orphan_factor = 0.5f64
                .powf(orphan_hours / cfg.orphan_halving_hours)
                .max(cfg.orphan_min_half_life_factor);
            let half_life = base_half_life * orphan_factor;
            let decay_factor = 0.5f64.powf(age_days / half_life);
            let new_weight = (thread.weight * decay_factor).max(0.0);

            if (new_weight - thread.weight).abs() < 0.001 {
                continue;
            }

            ThreadStorage::update_weight(conn, &thread.id, new_weight)?;
            affected += 1;

            // Auto-suspend if below threshold
            if new_weight < cfg.thread_suspend_threshold {
                tracing::warn!(thread_id = %thread.id, weight = new_weight, "Thread auto-suspended by decay");
                ThreadStorage::update_status(conn, &thread.id, ThreadStatus::Suspended)?;
                suspended_count += 1;
            }
        }

        // 2. Decay bridge weights (Active + Weak — so Weak bridges can still die)
        let mut bridges = BridgeStorage::list_active(conn)?;
        bridges.extend(BridgeStorage::list_by_status(conn, BridgeStatus::Weak)?);
        for bridge in &bridges {
            let reference_time = bridge.last_reinforced.unwrap_or(bridge.created_at);
            let age_days = (now - reference_time).num_hours() as f64 / 24.0;
            if age_days <= 0.0 {
                continue;
            }

            let decay_factor = 0.5f64.powf(age_days / cfg.bridge_half_life);
            let new_weight = bridge.weight * decay_factor;

            if new_weight < cfg.bridge_death_threshold {
                BridgeStorage::update_status(conn, &bridge.id, BridgeStatus::Invalid)?;
            } else if new_weight < 0.15 {
                // Weak threshold lowered from 0.30 to 0.15 to give bridges
                // more time to be reinforced via Hebbian usage before degrading.
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
/// Range: min_half_life (disposable, importance=0) to max_half_life (critical, importance=1).
fn effective_half_life(importance: f64, cfg: &DecayConfig) -> f64 {
    cfg.thread_min_half_life
        + (importance.clamp(0.0, 1.0) * (cfg.thread_max_half_life - cfg.thread_min_half_life))
}
