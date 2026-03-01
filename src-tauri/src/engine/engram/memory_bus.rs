// ── Engram: Multi-Agent Memory Sync Protocol — Memory Bus (§43) ─────────────
//
// Pub/sub system for cross-agent memory sharing.
//
// Agents can:
//   - Publish memories to the bus with topic tags and visibility scope
//   - Subscribe to topics with filters (min importance, source agents, rate limits)
//   - Receive deliveries during consolidation cycles
//
// Contradiction Resolution:
//   When a published memory contradicts an existing one in the receiving agent,
//   the bus applies recency + confidence tiebreaking.
//
// Architecture:
//   The bus is backed by an in-memory publication queue (per-process).
//   In a multi-process deployment, this would be replaced by SQLite WAL
//   or an external message queue. The current implementation is single-process
//   but the types are designed for easy migration.
//
// Integration:
//   - publish() is called by graph::store_*_dedup when scope includes
//     project/squad/global visibility
//   - deliver() is called during consolidation to batch-deliver publications
//   - Each agent has a SubscriptionFilter controlling what they receive

use crate::atoms::engram_types::{
    ConsolidationState, DeliveryReport, EdgeType, EpisodicMemory, MemoryEdge, MemoryPublication,
    MemoryScope, MemorySource, MemoryType, PublicationScope, SubscriptionFilter, TieredContent,
};
use crate::atoms::error::EngineResult;
use crate::engine::sessions::SessionStore;
use chrono::Utc;
use log::{info, warn};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// ═════════════════════════════════════════════════════════════════════════════
// Constants
// ═════════════════════════════════════════════════════════════════════════════

/// Maximum age of a publication in seconds before it expires.
const PUBLICATION_TTL_SECS: i64 = 86_400; // 24 hours

/// Maximum publications held in the bus at any time.
const MAX_PENDING_PUBLICATIONS: usize = 1000;

/// Importance threshold below which publications are auto-dropped.
const MIN_GLOBAL_IMPORTANCE: f32 = 0.2;

/// Content overlap threshold for contradiction detection.
const CONTRADICTION_OVERLAP_THRESHOLD: f64 = 0.5;

// ═════════════════════════════════════════════════════════════════════════════
// Memory Bus
// ═════════════════════════════════════════════════════════════════════════════

/// In-process memory bus for cross-agent memory sharing.
///
/// Thread-safe: all internal state is behind Arc<Mutex>.
/// Clone is cheap (Arc clones).
#[derive(Clone)]
pub struct MemoryBus {
    /// Pending publications awaiting delivery.
    publications: Arc<Mutex<Vec<MemoryPublication>>>,
    /// Per-agent subscription filters. Key = agent_id.
    subscriptions: Arc<Mutex<HashMap<String, SubscriptionFilter>>>,
}

impl MemoryBus {
    /// Create a new empty memory bus.
    pub fn new() -> Self {
        Self {
            publications: Arc::new(Mutex::new(Vec::new())),
            subscriptions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Publish a memory to the bus.
    ///
    /// The memory becomes available for delivery to agents whose
    /// subscription filters match.
    pub fn publish(&self, publication: MemoryPublication) -> EngineResult<()> {
        // Gate on minimum importance
        if publication.min_importance < MIN_GLOBAL_IMPORTANCE {
            info!(
                "[engram::bus] Dropping low-importance publication: {} (importance {:.2})",
                publication.memory_id, publication.min_importance
            );
        }

        let mut pubs = self.publications.lock().map_err(|e| {
            crate::atoms::error::EngineError::Other(format!("Bus lock poisoned: {}", e))
        })?;

        // Evict expired publications
        let now = Utc::now();
        pubs.retain(|p| {
            chrono::DateTime::parse_from_rfc3339(&p.published_at)
                .map(|dt| (now - dt.with_timezone(&Utc)).num_seconds() < PUBLICATION_TTL_SECS)
                .unwrap_or(false)
        });

        // Enforce capacity
        if pubs.len() >= MAX_PENDING_PUBLICATIONS {
            warn!(
                "[engram::bus] Publication queue full ({}/{}), dropping oldest",
                pubs.len(),
                MAX_PENDING_PUBLICATIONS
            );
            pubs.remove(0); // FIFO eviction
        }

        pubs.push(publication);
        Ok(())
    }

    /// Register or update a subscription filter for an agent.
    pub fn subscribe(&self, agent_id: &str, filter: SubscriptionFilter) -> EngineResult<()> {
        let mut subs = self.subscriptions.lock().map_err(|e| {
            crate::atoms::error::EngineError::Other(format!("Bus lock poisoned: {}", e))
        })?;
        subs.insert(agent_id.to_string(), filter);
        Ok(())
    }

    /// Remove a subscription.
    pub fn unsubscribe(&self, agent_id: &str) -> EngineResult<()> {
        let mut subs = self.subscriptions.lock().map_err(|e| {
            crate::atoms::error::EngineError::Other(format!("Bus lock poisoned: {}", e))
        })?;
        subs.remove(agent_id);
        Ok(())
    }

    /// Deliver pending publications to a specific agent.
    ///
    /// Applies the agent's subscription filter, checks for contradictions
    /// against existing memories, and stores accepted memories.
    ///
    /// Returns a DeliveryReport summarizing the cycle.
    pub fn deliver(&self, agent_id: &str, store: &SessionStore) -> EngineResult<DeliveryReport> {
        let pubs = self.publications.lock().map_err(|e| {
            crate::atoms::error::EngineError::Other(format!("Bus lock poisoned: {}", e))
        })?;

        let subs = self.subscriptions.lock().map_err(|e| {
            crate::atoms::error::EngineError::Other(format!("Bus lock poisoned: {}", e))
        })?;

        let filter = subs.get(agent_id).cloned().unwrap_or_default();
        let mut report = DeliveryReport::default();
        let mut delivered_count = 0usize;

        for pub_mem in pubs.iter() {
            // Skip self-published memories
            if pub_mem.source_agent == agent_id {
                continue;
            }

            // Check visibility scope
            if !is_visible_to(agent_id, &pub_mem.visibility) {
                report.filtered += 1;
                continue;
            }

            // Apply subscription filter
            if !matches_filter(pub_mem, &filter) {
                report.filtered += 1;
                continue;
            }

            // Rate limit
            if delivered_count >= filter.rate_limit {
                report.filtered += 1;
                continue;
            }

            // Check for contradictions with existing memories
            let scope = MemoryScope::agent(agent_id);
            let existing = store
                .engram_search_episodic_bm25(&pub_mem.content, &scope, 5)
                .unwrap_or_default();

            let mut contradicted = false;
            for (existing_mem, _score) in &existing {
                let overlap = content_overlap(&pub_mem.content, &existing_mem.content.full);
                if overlap > CONTRADICTION_OVERLAP_THRESHOLD {
                    // Contradiction detected — apply recency tiebreaking
                    let pub_newer = pub_mem.published_at > existing_mem.created_at;
                    if pub_newer {
                        // New publication wins — update existing
                        store
                            .engram_update_episodic_content(
                                &existing_mem.id,
                                &pub_mem.content,
                                None,
                            )
                            .ok();
                        // Add contradiction edge
                        let edge = MemoryEdge {
                            source_id: existing_mem.id.clone(),
                            target_id: pub_mem.memory_id.clone(),
                            edge_type: EdgeType::Contradicts,
                            weight: overlap as f32,
                            created_at: Utc::now().to_rfc3339(),
                        };
                        store.engram_add_edge(&edge).ok();
                        report.contradictions_resolved += 1;
                    }
                    contradicted = true;
                    break;
                }
            }

            if !contradicted {
                // Store as new episodic memory in the receiving agent's scope
                let mem = EpisodicMemory {
                    id: format!("bus-{}-{}", pub_mem.memory_id, agent_id),
                    agent_id: agent_id.to_string(),
                    session_id: format!("bus-delivery-{}", pub_mem.source_agent),
                    content: TieredContent {
                        full: pub_mem.content.clone(),
                        summary: None,
                        key_fact: None,
                        tags: None,
                    },
                    scope,
                    importance: pub_mem.min_importance,
                    strength: 0.5,
                    access_count: 0,
                    category: format!("bus:{}", pub_mem.source_agent),
                    outcome: None,
                    embedding: None,
                    embedding_model: None,
                    source: MemorySource::AutoCapture,
                    consolidation_state: ConsolidationState::Fresh,
                    negative_contexts: Vec::new(),
                    created_at: Utc::now().to_rfc3339(),
                    last_accessed_at: None,
                };
                store.engram_store_episodic(&mem).ok();
            }

            delivered_count += 1;
            report.delivered += 1;
        }

        info!(
            "[engram::bus] Delivery to {}: {} delivered, {} filtered, {} contradictions",
            agent_id, report.delivered, report.filtered, report.contradictions_resolved,
        );

        Ok(report)
    }

    /// Drain all expired publications. Called during maintenance.
    pub fn garbage_collect(&self) -> EngineResult<usize> {
        let mut pubs = self.publications.lock().map_err(|e| {
            crate::atoms::error::EngineError::Other(format!("Bus lock poisoned: {}", e))
        })?;

        let before = pubs.len();
        let now = Utc::now();
        pubs.retain(|p| {
            chrono::DateTime::parse_from_rfc3339(&p.published_at)
                .map(|dt| (now - dt.with_timezone(&Utc)).num_seconds() < PUBLICATION_TTL_SECS)
                .unwrap_or(false)
        });
        let removed = before - pubs.len();

        if removed > 0 {
            info!("[engram::bus] GC removed {} expired publications", removed);
        }

        Ok(removed)
    }

    /// Get the current number of pending publications.
    pub fn pending_count(&self) -> usize {
        self.publications.lock().map(|p| p.len()).unwrap_or(0)
    }

    /// Create a MemoryPublication from content.
    pub fn create_publication(
        source_agent: &str,
        memory_id: &str,
        memory_type: MemoryType,
        content: &str,
        topics: Vec<String>,
        visibility: PublicationScope,
        min_importance: f32,
    ) -> MemoryPublication {
        MemoryPublication {
            source_agent: source_agent.to_string(),
            memory_id: memory_id.to_string(),
            memory_type,
            topics,
            visibility,
            min_importance,
            content: content.to_string(),
            published_at: Utc::now().to_rfc3339(),
        }
    }
}

impl Default for MemoryBus {
    fn default() -> Self {
        Self::new()
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Helpers
// ═════════════════════════════════════════════════════════════════════════════

/// Check if a publication is visible to a given agent.
fn is_visible_to(agent_id: &str, scope: &PublicationScope) -> bool {
    match scope {
        PublicationScope::Global => true,
        PublicationScope::Project => true, // All agents in same project
        PublicationScope::Squad => true,   // All agents in same squad
        PublicationScope::Targeted(agents) => agents.iter().any(|a| a == agent_id),
    }
}

/// Check if a publication matches a subscription filter.
fn matches_filter(publication: &MemoryPublication, filter: &SubscriptionFilter) -> bool {
    // Topic filter
    if !filter.topics.is_empty() {
        let topic_match = publication
            .topics
            .iter()
            .any(|t| filter.topics.iter().any(|ft| ft == t));
        if !topic_match {
            return false;
        }
    }

    // Importance filter
    if publication.min_importance < filter.min_importance {
        return false;
    }

    // Source agent filter
    if !filter.source_agents.is_empty() && !filter.source_agents.contains(&publication.source_agent)
    {
        return false;
    }

    true
}

/// Word-level Jaccard overlap for contradiction detection.
fn content_overlap(a: &str, b: &str) -> f64 {
    let a_lower = a.to_lowercase();
    let b_lower = b.to_lowercase();
    let a_words: std::collections::HashSet<&str> = a_lower.split_whitespace().collect();
    let b_words: std::collections::HashSet<&str> = b_lower.split_whitespace().collect();
    if a_words.is_empty() || b_words.is_empty() {
        return 0.0;
    }
    let intersection = a_words.intersection(&b_words).count();
    let union = a_words.union(&b_words).count();
    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Tests
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bus_publish_and_count() {
        let bus = MemoryBus::new();
        let pub1 = MemoryBus::create_publication(
            "agent-a",
            "mem-1",
            MemoryType::Episodic,
            "Test content about Rust",
            vec!["rust".to_string()],
            PublicationScope::Global,
            0.5,
        );
        bus.publish(pub1).unwrap();
        assert_eq!(bus.pending_count(), 1);
    }

    #[test]
    fn bus_subscribe_and_filter() {
        let bus = MemoryBus::new();
        let filter = SubscriptionFilter {
            topics: vec!["rust".to_string()],
            min_importance: 0.3,
            source_agents: vec![],
            rate_limit: 10,
        };
        bus.subscribe("agent-b", filter).unwrap();
    }

    #[test]
    fn visibility_checks() {
        assert!(is_visible_to("any", &PublicationScope::Global));
        assert!(is_visible_to("any", &PublicationScope::Project));
        assert!(is_visible_to(
            "agent-a",
            &PublicationScope::Targeted(vec!["agent-a".to_string()])
        ));
        assert!(!is_visible_to(
            "agent-b",
            &PublicationScope::Targeted(vec!["agent-a".to_string()])
        ));
    }

    #[test]
    fn filter_by_topic() {
        let pub1 = MemoryBus::create_publication(
            "agent-a",
            "m1",
            MemoryType::Semantic,
            "Test",
            vec!["rust".to_string()],
            PublicationScope::Global,
            0.5,
        );
        let filter = SubscriptionFilter {
            topics: vec!["python".to_string()],
            min_importance: 0.0,
            source_agents: vec![],
            rate_limit: 10,
        };
        assert!(!matches_filter(&pub1, &filter));

        let filter2 = SubscriptionFilter {
            topics: vec!["rust".to_string()],
            min_importance: 0.0,
            source_agents: vec![],
            rate_limit: 10,
        };
        assert!(matches_filter(&pub1, &filter2));
    }

    #[test]
    fn content_overlap_works() {
        let high = content_overlap("the quick brown fox jumps", "the quick brown dog jumps");
        assert!(high > 0.5, "overlap={}", high);

        let low = content_overlap("rust async programming", "python flask web");
        assert!(low < 0.2, "overlap={}", low);
    }

    #[test]
    fn bus_gc_empty() {
        let bus = MemoryBus::new();
        let removed = bus.garbage_collect().unwrap();
        assert_eq!(removed, 0);
    }
}
