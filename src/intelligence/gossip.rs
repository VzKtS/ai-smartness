//! Gossip v2 — concept-based bridge discovery.
//!
//! Pipeline:
//!   1. ConceptIndex inverted index → find_overlaps(min_shared)
//!   2. Score: weight = overlap_ratio × 0.5 + richness × 0.5
//!   3. Create bridges, collect merge candidates (weight >= 0.60)
//!   4. Legacy topic overlap fallback for threads without concepts
//!
//! Replaces v1 (TF-IDF embedding + topic/label overlap + propagation).

use crate::{id_gen, time_utils};
use crate::bridge::{BridgeStatus, BridgeType, ThinkBridge};
use crate::config::GossipConfig;
use crate::thread::Thread;
use crate::AiResult;
use crate::storage::bridges::BridgeStorage;
use crate::storage::concept_index::ConceptIndex;
use crate::storage::threads::ThreadStorage;
use rusqlite::Connection;

/// Merge candidate produced by gossip when overlap score >= threshold.
#[derive(Debug, Clone)]
pub struct MergeCandidate {
    pub thread_a: String,
    pub thread_b: String,
    pub overlap_score: f64,
    pub shared_concepts: Vec<String>,
    pub bridge_id: String,
}

pub struct Gossip {
    concept_index: ConceptIndex,
}

impl Gossip {
    /// Build gossip engine with ConceptIndex loaded from DB.
    pub fn new(conn: &Connection) -> AiResult<Self> {
        let concept_index = ConceptIndex::build_from_db(conn)?;
        Ok(Self { concept_index })
    }

    /// Main gossip cycle — concept-based bridge discovery.
    /// Returns (bridges_created, merge_candidates).
    pub fn run_cycle(
        &self,
        conn: &Connection,
        config: &GossipConfig,
    ) -> AiResult<(u32, Vec<MergeCandidate>)> {
        // One-time v1 bridge migration (idempotent — 0 rows after first run)
        Self::migrate_v1_bridges(conn)?;

        // Load ALL threads (active + suspended + archived) for inclusive gossip
        let all_threads = ThreadStorage::list_all(conn)?;
        if all_threads.len() < 2 {
            tracing::debug!(threads = all_threads.len(), "Gossip v2 skipped: not enough threads");
            return Ok((0, vec![]));
        }

        let thread_count = all_threads.len();
        let concept_count = self.concept_index.concept_count();
        let indexed_count = self.concept_index.thread_count();
        tracing::info!(
            total_threads = thread_count,
            indexed_threads = indexed_count,
            concepts = concept_count,
            "Gossip v2 cycle starting"
        );

        let (max_per, _max_total) = Self::dynamic_limits(thread_count, config);
        let mut created = 0u32;
        let mut merge_candidates = Vec::new();

        // Phase 1: Concept overlap discovery via inverted index
        let min_shared = config.concept_overlap_min_shared;
        let min_weight = config.concept_min_bridge_weight;
        let merge_threshold = config.merge_evaluation_threshold;

        if indexed_count >= 2 && config.concept_gossip_enabled {
            let overlaps = self.concept_index.find_overlaps(min_shared);
            tracing::info!(
                pairs = overlaps.len(),
                min_shared = min_shared,
                "Gossip v2 P1: concept overlap pairs found"
            );

            for (thread_a, thread_b, shared_count, shared_concepts) in &overlaps {
                // Check bridge limits
                let existing_a = BridgeStorage::list_for_thread(conn, thread_a)?;
                if existing_a.len() >= max_per {
                    continue;
                }

                // Skip if bridge already exists — reinforce if stronger
                let existing_bridge = existing_a.iter().find(|b| {
                    b.source_id == *thread_b || b.target_id == *thread_b
                });

                if let Some(existing) = existing_bridge {
                    // Reinforce if gossip bridge and new overlap is stronger
                    if existing.created_by.starts_with("gossip") {
                        let (_, overlap_ratio, _) =
                            self.concept_index.overlap_score(thread_a, thread_b);
                        let new_weight = Self::compute_weight(*shared_count, overlap_ratio);
                        if new_weight > existing.weight {
                            BridgeStorage::update_weight(conn, &existing.id, new_weight)?;
                            tracing::debug!(
                                bridge = %&existing.id[..8.min(existing.id.len())],
                                old_weight = format!("{:.3}", existing.weight).as_str(),
                                new_weight = format!("{:.3}", new_weight).as_str(),
                                "Gossip v2 P1: reinforced existing bridge"
                            );
                        }
                    }
                    continue;
                }

                let existing_b = BridgeStorage::list_for_thread(conn, thread_b)?;
                if existing_b.len() >= max_per {
                    continue;
                }

                // Compute weight
                let (_, overlap_ratio, _) =
                    self.concept_index.overlap_score(thread_a, thread_b);
                let weight = Self::compute_weight(*shared_count, overlap_ratio);

                if weight < min_weight {
                    continue;
                }

                // Determine relation type
                let relation = all_threads
                    .iter()
                    .find(|t| t.id == *thread_a)
                    .and_then(|ta| {
                        all_threads.iter().find(|t| t.id == *thread_b).map(|tb| {
                            Self::determine_relation(ta, tb, weight)
                        })
                    })
                    .unwrap_or(BridgeType::Sibling);

                let bridge_id = id_gen::bridge_id();
                let bridge = ThinkBridge {
                    id: bridge_id.clone(),
                    source_id: thread_a.clone(),
                    target_id: thread_b.clone(),
                    relation_type: relation,
                    reason: format!(
                        "gossip:concept_overlap({},ratio={:.2})",
                        shared_count, overlap_ratio
                    ),
                    shared_concepts: shared_concepts.clone(),
                    weight,
                    confidence: weight,
                    status: BridgeStatus::Active,
                    propagated_from: None,
                    propagation_depth: 0,
                    created_by: "gossip_v2".to_string(),
                    use_count: 0,
                    created_at: time_utils::now(),
                    last_reinforced: None,
                };

                if BridgeStorage::insert(conn, &bridge).is_ok() {
                    tracing::info!(
                        source = %&thread_a[..8.min(thread_a.len())],
                        target = %&thread_b[..8.min(thread_b.len())],
                        shared = *shared_count,
                        weight = format!("{:.3}", weight).as_str(),
                        "Gossip v2 P1: bridge created (concept overlap)"
                    );
                    created += 1;

                    // Collect merge candidate if above threshold
                    if weight >= merge_threshold {
                        merge_candidates.push(MergeCandidate {
                            thread_a: thread_a.clone(),
                            thread_b: thread_b.clone(),
                            overlap_score: weight,
                            shared_concepts: shared_concepts.clone(),
                            bridge_id,
                        });
                    }
                }
            }
        }

        // Phase 2: Legacy topic overlap for threads WITHOUT concepts
        if config.topic_overlap_enabled {
            let legacy_created =
                Self::run_legacy_topic_overlap(conn, &all_threads, config, max_per)?;
            created += legacy_created;
        }

        tracing::info!(
            bridges_created = created,
            merge_candidates = merge_candidates.len(),
            "Gossip v2 cycle complete"
        );

        Ok((created, merge_candidates))
    }

    /// Compute bridge weight from concept overlap metrics.
    /// weight = overlap_ratio × 0.5 + richness × 0.5
    fn compute_weight(shared_count: usize, overlap_ratio: f64) -> f64 {
        let richness = (shared_count as f64 / 5.0).min(1.0);
        overlap_ratio * 0.5 + richness * 0.5
    }

    /// Legacy topic overlap for threads without concepts (backward compat).
    /// Only processes threads that have topics but NO concepts.
    fn run_legacy_topic_overlap(
        conn: &Connection,
        active: &[Thread],
        config: &GossipConfig,
        max_per: usize,
    ) -> AiResult<u32> {
        let mut created = 0u32;

        for source in active {
            // Only legacy threads: has topics but no concepts
            if source.topics.is_empty() || !source.concepts.is_empty() {
                continue;
            }

            let existing = BridgeStorage::list_for_thread(conn, &source.id)?;
            if existing.len() >= max_per {
                continue;
            }

            for target in active {
                if target.id == source.id {
                    continue;
                }

                // Skip if target has concepts (handled by Phase 1)
                if !target.concepts.is_empty() {
                    continue;
                }

                if existing
                    .iter()
                    .any(|b| b.source_id == target.id || b.target_id == target.id)
                {
                    continue;
                }

                let shared = Self::shared_topics(source, target);
                if shared.len() >= config.topic_overlap_min_shared {
                    let bridge = ThinkBridge {
                        id: id_gen::bridge_id(),
                        source_id: source.id.clone(),
                        target_id: target.id.clone(),
                        relation_type: BridgeType::Sibling,
                        reason: format!("gossip:topic_overlap({})", shared.len()),
                        shared_concepts: shared,
                        weight: 0.5,
                        confidence: 0.6,
                        status: BridgeStatus::Active,
                        propagated_from: None,
                        propagation_depth: 0,
                        created_by: "gossip_v2".to_string(),
                        use_count: 0,
                        created_at: time_utils::now(),
                        last_reinforced: None,
                    };
                    if BridgeStorage::insert(conn, &bridge).is_ok() {
                        tracing::info!(
                            source = %&source.id[..8.min(source.id.len())],
                            target = %&target.id[..8.min(target.id.len())],
                            shared = bridge.shared_concepts.len(),
                            "Gossip v2 P2: bridge created (legacy topic overlap)"
                        );
                        created += 1;
                    }
                }
            }
        }

        Ok(created)
    }

    /// Dynamic bridge limits based on thread count and config.
    fn dynamic_limits(n_threads: usize, config: &GossipConfig) -> (usize, usize) {
        let n = n_threads.max(1);
        let max_total = (n as f64 * config.target_bridge_ratio) as usize;
        let max_per =
            (max_total / n).clamp(config.min_bridges_per_thread, config.max_bridges_per_thread);
        (max_per, max_total)
    }

    /// Determine bridge relation type based on thread relationships.
    fn determine_relation(source: &Thread, target: &Thread, weight: f64) -> BridgeType {
        if source.parent_id.as_deref() == Some(&*target.id) {
            BridgeType::ChildOf
        } else if target.parent_id.as_deref() == Some(&*source.id) {
            BridgeType::Extends
        } else if source.parent_id.is_some() && source.parent_id == target.parent_id {
            BridgeType::Sibling
        } else if weight >= 0.80 && source.created_at > target.created_at {
            BridgeType::Extends
        } else {
            BridgeType::Sibling
        }
    }

    /// Shared topics between two threads.
    fn shared_topics(a: &Thread, b: &Thread) -> Vec<String> {
        a.topics
            .iter()
            .filter(|t| {
                b.topics
                    .iter()
                    .any(|bt| bt.to_lowercase() == t.to_lowercase())
            })
            .cloned()
            .collect()
    }

    /// One-time migration: invalidate v1 propagation bridges.
    /// Idempotent — affects 0 rows after first run.
    pub fn migrate_v1_bridges(conn: &Connection) -> AiResult<usize> {
        let affected = conn
            .execute(
                "UPDATE bridges SET status = 'invalid' WHERE reason LIKE 'gossip:propagation%' AND status != 'invalid'",
                [],
            )
            .map_err(|e| crate::AiError::Storage(format!("V1 bridge migration failed: {}", e)))?;
        if affected > 0 {
            tracing::info!(
                invalidated = affected,
                "Gossip v2: migrated v1 propagation bridges to invalid"
            );
        }
        Ok(affected)
    }
}
