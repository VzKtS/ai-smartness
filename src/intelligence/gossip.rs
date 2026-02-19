//! Gossip -- bridge discovery via similarity.
//!
//! 3-phase pipeline:
//!   1. Embedding similarity (TF-IDF cosine)
//!   2. Topic overlap complement
//!   3. Gossip propagation A-B + B-C -> A-C

use crate::{id_gen, time_utils};
use crate::bridge::{BridgeStatus, BridgeType, ThinkBridge};
use crate::config::GossipConfig;
use crate::constants::*;
use crate::thread::Thread;
use crate::AiResult;
use crate::processing::embeddings::EmbeddingManager;
use crate::storage::bridges::BridgeStorage;
use crate::storage::threads::ThreadStorage;
use rusqlite::Connection;

#[allow(dead_code)]
const MAX_PROPAGATION_DEPTH: usize = 1;
const PROPAGATION_THRESHOLD_FACTOR: f64 = 0.9;
const PROPAGATION_CONFIDENCE_FACTOR: f64 = 0.9;

pub struct Gossip {
    similarity_threshold: f64,
}

impl Gossip {
    pub fn new() -> Self {
        Self {
            similarity_threshold: GOSSIP_TFIDF_THRESHOLD,
        }
    }

    /// Main gossip cycle -- called by daemon prune loop.
    /// Config-driven bridge limits from GossipConfig.
    /// Returns number of bridges created.
    pub fn run_cycle(&self, conn: &Connection, config: &GossipConfig) -> AiResult<u32> {
        let active = ThreadStorage::list_active(conn)?;
        if active.len() < 2 {
            tracing::debug!(active_threads = active.len(), "Gossip skipped: not enough threads");
            return Ok(0);
        }

        tracing::info!(active_threads = active.len(), threshold = self.similarity_threshold, "Gossip cycle starting");

        let (max_per, _max_total) = Self::dynamic_limits(active.len(), config);
        let embeddings = EmbeddingManager::global();
        let mut created = 0u32;

        // Phase 1: Embedding similarity
        for source in &active {
            let source_emb = match &source.embedding {
                Some(e) if !e.is_empty() => e,
                _ => continue,
            };

            let existing = BridgeStorage::list_for_thread(conn, &source.id)?;
            if existing.len() >= max_per {
                continue;
            }

            for target in &active {
                if target.id == source.id {
                    continue;
                }

                // Skip if bridge already exists
                if existing
                    .iter()
                    .any(|b| b.source_id == target.id || b.target_id == target.id)
                {
                    continue;
                }

                let target_emb = match &target.embedding {
                    Some(e) if !e.is_empty() => e,
                    _ => continue,
                };

                let sim = embeddings.similarity(source_emb, target_emb);
                tracing::debug!(
                    source = %&source.id[..8.min(source.id.len())],
                    target = %&target.id[..8.min(target.id.len())],
                    similarity = format!("{:.3}", sim).as_str(),
                    "Gossip P1: embedding comparison"
                );
                if sim >= self.similarity_threshold {
                    let relation = Self::determine_relation(source, target, sim);
                    tracing::info!(
                        source = %&source.id[..8.min(source.id.len())],
                        target = %&target.id[..8.min(target.id.len())],
                        relation = ?relation,
                        weight = format!("{:.3}", sim).as_str(),
                        "Gossip P1: bridge created (embedding)"
                    );
                    let bridge = ThinkBridge {
                        id: id_gen::bridge_id(),
                        source_id: source.id.clone(),
                        target_id: target.id.clone(),
                        relation_type: relation,
                        reason: format!("gossip:embedding(sim={:.2})", sim),
                        shared_concepts: Self::shared_topics(source, target),
                        weight: sim,
                        confidence: sim,
                        status: BridgeStatus::Active,
                        propagated_from: None,
                        propagation_depth: 0,
                        created_by: "gossip".to_string(),
                        use_count: 0,
                        created_at: time_utils::now(),
                        last_reinforced: None,
                    };
                    if BridgeStorage::insert(conn, &bridge).is_ok() {
                        created += 1;
                    }
                }
            }
        }

        // Phase 2: Topic overlap complement
        for source in &active {
            if source.topics.is_empty() {
                continue;
            }

            let existing = BridgeStorage::list_for_thread(conn, &source.id)?;
            if existing.len() >= max_per {
                continue;
            }

            for target in &active {
                if target.id == source.id {
                    continue;
                }

                if existing
                    .iter()
                    .any(|b| b.source_id == target.id || b.target_id == target.id)
                {
                    continue;
                }

                let shared = Self::shared_topics(source, target);
                let shared_count = shared.len();
                tracing::debug!(
                    source = %&source.id[..8.min(source.id.len())],
                    target = %&target.id[..8.min(target.id.len())],
                    shared_count,
                    shared_topics = ?shared,
                    "Gossip P2: topic overlap check"
                );
                if shared_count >= GOSSIP_TOPIC_OVERLAP_MIN {
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
                        created_by: "gossip".to_string(),
                        use_count: 0,
                        created_at: time_utils::now(),
                        last_reinforced: None,
                    };
                    if BridgeStorage::insert(conn, &bridge).is_ok() {
                        tracing::info!(
                            source = %&source.id[..8.min(source.id.len())],
                            target = %&target.id[..8.min(target.id.len())],
                            shared_count,
                            "Gossip P2: bridge created (topic overlap)"
                        );
                        created += 1;
                    }
                }
            }
        }

        // Phase 2B: Label overlap complement
        for source in &active {
            if source.labels.is_empty() {
                continue;
            }

            let existing = BridgeStorage::list_for_thread(conn, &source.id)?;
            if existing.len() >= max_per {
                continue;
            }

            for target in &active {
                if target.id == source.id || target.labels.is_empty() {
                    continue;
                }

                if existing
                    .iter()
                    .any(|b| b.source_id == target.id || b.target_id == target.id)
                {
                    continue;
                }

                let shared = Self::shared_labels(source, target);
                let shared_count = shared.len();
                if shared_count >= GOSSIP_LABEL_OVERLAP_MIN {
                    let bridge = ThinkBridge {
                        id: id_gen::bridge_id(),
                        source_id: source.id.clone(),
                        target_id: target.id.clone(),
                        relation_type: BridgeType::Sibling,
                        reason: format!("gossip:label_overlap({})", shared_count),
                        shared_concepts: shared,
                        weight: 0.55,
                        confidence: 0.65,
                        status: BridgeStatus::Active,
                        propagated_from: None,
                        propagation_depth: 0,
                        created_by: "gossip".to_string(),
                        use_count: 0,
                        created_at: time_utils::now(),
                        last_reinforced: None,
                    };
                    if BridgeStorage::insert(conn, &bridge).is_ok() {
                        tracing::info!(
                            source = %&source.id[..8.min(source.id.len())],
                            target = %&target.id[..8.min(target.id.len())],
                            shared_count,
                            "Gossip P2B: bridge created (label overlap)"
                        );
                        created += 1;
                    }
                }
            }
        }

        // Phase 3: Propagation A-B + B-C -> A-C
        for source in &active {
            let bridges = BridgeStorage::list_for_thread(conn, &source.id)?;
            if bridges.len() >= max_per {
                continue;
            }
            for bridge in &bridges {
                if bridge.propagation_depth >= 1 {
                    continue;
                }

                let neighbor_id = if bridge.source_id == source.id {
                    &bridge.target_id
                } else {
                    &bridge.source_id
                };

                let neighbor_bridges = BridgeStorage::list_for_thread(conn, neighbor_id)?;
                for nb in &neighbor_bridges {
                    let transitive_id = if nb.source_id == *neighbor_id {
                        &nb.target_id
                    } else {
                        &nb.source_id
                    };

                    if transitive_id == &source.id {
                        continue;
                    }

                    // Skip if bridge already exists
                    if bridges
                        .iter()
                        .any(|b| b.source_id == *transitive_id || b.target_id == *transitive_id)
                    {
                        continue;
                    }

                    let prop_sim = bridge.confidence * PROPAGATION_THRESHOLD_FACTOR;
                    if prop_sim >= self.similarity_threshold * PROPAGATION_THRESHOLD_FACTOR {
                        let prop_bridge = ThinkBridge {
                            id: id_gen::bridge_id(),
                            source_id: source.id.clone(),
                            target_id: transitive_id.clone(),
                            relation_type: BridgeType::Sibling,
                            reason: format!("gossip:propagation(via={})", neighbor_id),
                            shared_concepts: vec![],
                            weight: prop_sim * PROPAGATION_CONFIDENCE_FACTOR,
                            confidence: bridge.confidence * PROPAGATION_CONFIDENCE_FACTOR,
                            status: BridgeStatus::Active,
                            propagated_from: Some(bridge.id.clone()),
                            propagation_depth: bridge.propagation_depth + 1,
                            created_by: "gossip".to_string(),
                            use_count: 0,
                            created_at: time_utils::now(),
                            last_reinforced: None,
                        };
                        if BridgeStorage::insert(conn, &prop_bridge).is_ok() {
                            tracing::info!(
                                source = %&source.id[..8.min(source.id.len())],
                                target = %&transitive_id[..8.min(transitive_id.len())],
                                via = %&neighbor_id[..8.min(neighbor_id.len())],
                                confidence = format!("{:.3}", bridge.confidence * PROPAGATION_CONFIDENCE_FACTOR).as_str(),
                                "Gossip P3: bridge created (propagation)"
                            );
                            created += 1;
                        }
                    }
                }
            }
        }

        tracing::info!(bridges_created = created, "Gossip cycle complete");

        Ok(created)
    }

    /// Dynamic bridge limits based on current thread count and config.
    fn dynamic_limits(n_threads: usize, config: &GossipConfig) -> (usize, usize) {
        let n = n_threads.max(1);
        let max_total = (n as f64 * config.target_bridge_ratio) as usize;
        let max_per = (max_total / n).clamp(config.min_bridges_per_thread, config.max_bridges_per_thread);
        (max_per, max_total)
    }

    /// Strengthen bridges between co-accessed threads.
    pub fn strengthen_used_bridges(conn: &Connection, thread_ids: &[&str]) -> AiResult<()> {
        for (i, a) in thread_ids.iter().enumerate() {
            for b in thread_ids.iter().skip(i + 1) {
                let bridges = BridgeStorage::list_for_thread(conn, a)?;
                for bridge in &bridges {
                    if bridge.source_id == *b || bridge.target_id == *b {
                        BridgeStorage::increment_use(conn, &bridge.id)?;
                        let new_weight = (bridge.weight + BRIDGE_USE_BOOST).min(1.0);
                        BridgeStorage::update_weight(conn, &bridge.id, new_weight)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Determine bridge relation type based on thread relationships.
    fn determine_relation(source: &Thread, target: &Thread, similarity: f64) -> BridgeType {
        if source.parent_id.as_deref() == Some(&*target.id) {
            BridgeType::ChildOf
        } else if target.parent_id.as_deref() == Some(&*source.id) {
            BridgeType::Extends
        } else if source.parent_id.is_some() && source.parent_id == target.parent_id {
            BridgeType::Sibling
        } else if similarity >= GOSSIP_STRONG_BRIDGE && source.created_at > target.created_at {
            BridgeType::Extends
        } else {
            BridgeType::Sibling
        }
    }

    /// Find shared topics between two threads.
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

    /// Find shared labels between two threads.
    fn shared_labels(a: &Thread, b: &Thread) -> Vec<String> {
        a.labels
            .iter()
            .filter(|l| {
                b.labels
                    .iter()
                    .any(|bl| bl.to_lowercase() == l.to_lowercase())
            })
            .cloned()
            .collect()
    }
}

impl Default for Gossip {
    fn default() -> Self {
        Self::new()
    }
}
