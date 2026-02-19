//! Memory Retriever -- high-level retrieval via text search.
//!
//! For the full Engram pipeline, use EngramRetriever directly.

use crate::thread::Thread;
use crate::AiResult;
use crate::storage::threads::ThreadStorage;
use rusqlite::Connection;

pub struct MemoryRetriever;

impl MemoryRetriever {
    /// Simple text-search recall.
    pub fn recall(conn: &Connection, query: &str) -> AiResult<Vec<Thread>> {
        tracing::info!(query = %query, "Memory recall");
        let results = ThreadStorage::search(conn, query)?;
        tracing::debug!(results = results.len(), "Recall complete");
        Ok(results)
    }
}
