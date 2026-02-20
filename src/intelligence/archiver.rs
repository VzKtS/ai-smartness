//! Archiver -- move stale suspended threads to archived.

use crate::config::DecayConfig;
use crate::thread::ThreadStatus;
use crate::AiResult;
use crate::storage::threads::ThreadStorage;
use chrono::Utc;
use rusqlite::Connection;

pub struct Archiver;

impl Archiver {
    /// Archive suspended threads inactive for > archive_after_hours.
    /// Returns count of archived threads.
    pub fn archive_stale(conn: &Connection, cfg: &DecayConfig) -> AiResult<u32> {
        let now = Utc::now();
        let suspended = ThreadStorage::list_by_status(conn, &ThreadStatus::Suspended)?;
        let mut count = 0u32;
        let threshold_hours = cfg.archive_after_hours as i64;

        for thread in &suspended {
            let hours_inactive = (now - thread.last_active).num_hours();
            if hours_inactive >= threshold_hours {
                tracing::debug!(thread_id = %thread.id, hours_inactive = hours_inactive, "Archiving stale thread");
                ThreadStorage::update_status(conn, &thread.id, ThreadStatus::Archived)?;
                count += 1;
            }
        }

        if count > 0 {
            tracing::info!(archived_count = count, "Archive cycle complete");
        }

        Ok(count)
    }
}
