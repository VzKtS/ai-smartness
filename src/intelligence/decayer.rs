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
            } else if new_weight < crate::constants::BRIDGE_WEAK_THRESHOLD {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::*;

    fn default_cfg() -> DecayConfig {
        DecayConfig::default()
    }

    // ── Pure function tests ──

    #[test]
    fn test_effective_half_life_zero_importance() {
        let cfg = default_cfg();
        let hl = effective_half_life(0.0, &cfg);
        assert!((hl - 0.75).abs() < 0.001); // min_half_life
    }

    #[test]
    fn test_effective_half_life_max_importance() {
        let cfg = default_cfg();
        let hl = effective_half_life(1.0, &cfg);
        assert!((hl - 7.0).abs() < 0.001); // max_half_life
    }

    #[test]
    fn test_effective_half_life_mid() {
        let cfg = default_cfg();
        let hl = effective_half_life(0.5, &cfg);
        // 0.75 + 0.5 * (7.0 - 0.75) = 0.75 + 3.125 = 3.875
        assert!((hl - 3.875).abs() < 0.001);
    }

    #[test]
    fn test_effective_half_life_clamped() {
        let cfg = default_cfg();
        let hl = effective_half_life(1.5, &cfg);
        // clamped to 1.0 -> max_half_life = 7.0
        assert!((hl - 7.0).abs() < 0.001);
    }

    // ── DB-level decay tests ──

    #[test]
    fn test_decay_active_empty_db() {
        let conn = setup_agent_db();
        let cfg = default_cfg();
        let affected = Decayer::decay_active(&conn, &cfg).unwrap();
        assert_eq!(affected, 0);
    }

    #[test]
    fn test_decay_reduces_weight() {
        let conn = setup_agent_db();
        let cfg = default_cfg();
        // Thread last active 12 hours ago, importance=0.5 -> base half-life=3.875 days
        // Orphan acceleration: 0.5^(12/6) = 0.25, clamped to 0.25 (> min 0.1)
        // Effective half-life = 3.875 * 0.25 ≈ 0.97 days
        // decay_factor = 0.5^(0.5/0.97) ≈ 0.70 -> weight ~ 0.70, well above 0.1
        let t = ThreadBuilder::new()
            .id("t1")
            .weight(1.0)
            .importance(0.5)
            .last_active(hours_ago(12))
            .build();
        ThreadStorage::insert(&conn, &t).unwrap();

        let affected = Decayer::decay_active(&conn, &cfg).unwrap();
        assert_eq!(affected, 1);

        let got = ThreadStorage::get(&conn, "t1").unwrap().unwrap();
        assert!(got.weight < 1.0, "weight should decrease from decay");
        assert!(got.weight > 0.1, "should not be suspended at 12 hours");
        assert_eq!(got.status, ThreadStatus::Active);
    }

    #[test]
    fn test_decay_suspends_below_threshold() {
        let conn = setup_agent_db();
        let cfg = default_cfg();
        // Thread with low importance, last active long ago -> should decay below 0.1
        // importance=0.0 -> half-life=0.75 days, 10 days ago -> extreme decay
        let t = ThreadBuilder::new()
            .id("t1")
            .weight(0.5)
            .importance(0.0)
            .last_active(days_ago(10))
            .build();
        ThreadStorage::insert(&conn, &t).unwrap();

        Decayer::decay_active(&conn, &cfg).unwrap();

        let got = ThreadStorage::get(&conn, "t1").unwrap().unwrap();
        assert!(got.weight < 0.1, "weight should be below suspend threshold");
        assert_eq!(got.status, ThreadStatus::Suspended);
    }

    #[test]
    fn test_decay_bridge_death() {
        let conn = setup_agent_db();
        let cfg = default_cfg();
        // Two threads + one bridge. Bridge created 30 days ago, half-life=2 days -> should die.
        let t1 = ThreadBuilder::new().id("t1").build();
        let t2 = ThreadBuilder::new().id("t2").build();
        ThreadStorage::insert(&conn, &t1).unwrap();
        ThreadStorage::insert(&conn, &t2).unwrap();

        let b = BridgeBuilder::new()
            .id("b1")
            .source_id("t1")
            .target_id("t2")
            .weight(0.5)
            .created_at(days_ago(30))
            .build();
        BridgeStorage::insert(&conn, &b).unwrap();

        Decayer::decay_active(&conn, &cfg).unwrap();

        let got = BridgeStorage::get(&conn, "b1").unwrap().unwrap();
        assert_eq!(got.status, BridgeStatus::Invalid, "bridge should be marked invalid after heavy decay");
    }

    #[test]
    fn test_decay_cleans_orphan_bridges() {
        // Use no-FK db so we can create orphan bridges
        let conn = setup_agent_db_no_fk();
        let cfg = default_cfg();
        let t1 = ThreadBuilder::new().id("t1").build();
        let t2 = ThreadBuilder::new().id("t2").build();
        ThreadStorage::insert(&conn, &t1).unwrap();
        ThreadStorage::insert(&conn, &t2).unwrap();
        let b = BridgeBuilder::new()
            .id("b-orphan")
            .source_id("t1")
            .target_id("t2")
            .build();
        BridgeStorage::insert(&conn, &b).unwrap();
        // Delete t2 to orphan the bridge (no FK cascade)
        conn.execute("DELETE FROM threads WHERE id = 't2'", []).unwrap();

        Decayer::decay_active(&conn, &cfg).unwrap();

        // Orphan bridge should be deleted
        assert!(BridgeStorage::get(&conn, "b-orphan").unwrap().is_none());
    }
}
