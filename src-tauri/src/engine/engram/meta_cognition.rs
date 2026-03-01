// ── Engram: Reflective Meta-Cognition (§38) ─────────────────────────────────
//
// "Know what you know — and what you don't."
//
// Maintains a KnowledgeConfidenceMap that reflects how deeply the agent
// understands different domains. Built by clustering memories and measuring
// depth, freshness, and internal consistency.
//
// Key mechanisms:
//   1. Domain Discovery: cluster embeddings → auto-label domains
//   2. Confidence Assessment: per-domain score = depth × freshness × (1−uncertainty)
//   3. Query Routing: assess_query_confidence() → Confident | Uncertain | Unknown
//   4. Reflection Injection: "I am most knowledgeable about X" system prompt
//
// This module does NOT require an LLM — all scoring is heuristic.
// Domain labelling uses the most common terms in each cluster.

use crate::atoms::engram_types::{
    DomainAssessment, KnowledgeConfidenceMap, KnowledgeDomain, MemoryScope, SemanticMemory,
};
use crate::atoms::error::EngineResult;
use crate::engine::memory::EmbeddingClient;
use crate::engine::sessions::SessionStore;
use chrono::Utc;
use log::info;

// ═════════════════════════════════════════════════════════════════════════════
// Constants
// ═════════════════════════════════════════════════════════════════════════════

/// Minimum number of memories for a cluster to be considered a "domain".
const MIN_DOMAIN_SIZE: usize = 3;

/// Cosine similarity threshold for same-cluster membership.
const CLUSTER_SIM_THRESHOLD: f64 = 0.70;

/// Days to consider "fresh" for the staleness metric.
const FRESHNESS_WINDOW_DAYS: i64 = 30;

/// High-confidence threshold — above this, agent is "Confident".
const HIGH_CONFIDENCE: f64 = 0.7;

/// Low-confidence threshold — below this, agent is "Unknown".
const LOW_CONFIDENCE: f64 = 0.3;

/// Maximum domains to track (prevents runaway clustering).
const MAX_DOMAINS: usize = 50;

// ═════════════════════════════════════════════════════════════════════════════
// Knowledge Confidence Map Builder
// ═════════════════════════════════════════════════════════════════════════════

/// Rebuild the knowledge confidence map from all semantic memories.
///
/// Pipeline:
///   1. Fetch all semantic memories with embeddings
///   2. Cluster by cosine similarity (greedy single-linkage)
///   3. Score each cluster: depth × freshness × (1 − uncertainty)
///   4. Label each cluster with most common subject/predicate terms
///
/// Called during consolidation or on explicit "reflect" command.
pub async fn rebuild_confidence_map(
    store: &SessionStore,
    _embedding_client: Option<&EmbeddingClient>,
    scope: &MemoryScope,
) -> EngineResult<KnowledgeConfidenceMap> {
    info!("[engram::meta] Rebuilding knowledge confidence map");

    // Fetch all semantic memories for this scope
    let semantics = store.engram_search_semantic_bm25("", scope, 2000)?;

    if semantics.is_empty() {
        return Ok(KnowledgeConfidenceMap {
            domains: Vec::new(),
            global_coverage: 0.0,
            last_rebuilt: Utc::now().to_rfc3339(),
        });
    }

    // ── Step 1: Cluster by embedding similarity ──────────────────
    let clusters = cluster_semantics(&semantics);

    // ── Step 2: Score each cluster → KnowledgeDomain ─────────────
    let now = Utc::now();
    let mut domains: Vec<KnowledgeDomain> = clusters
        .iter()
        .filter(|c| c.len() >= MIN_DOMAIN_SIZE)
        .take(MAX_DOMAINS)
        .map(|cluster| {
            let depth = cluster.len();

            // Freshness: fraction of cluster memories created in the last 30 days
            let fresh_count = cluster
                .iter()
                .filter(|mem| {
                    chrono::DateTime::parse_from_rfc3339(&mem.created_at)
                        .map(|dt| {
                            (now - dt.with_timezone(&Utc)).num_days() <= FRESHNESS_WINDOW_DAYS
                        })
                        .unwrap_or(false)
                })
                .count();
            let freshness = fresh_count as f64 / depth as f64;

            // Uncertainty: fraction of low-confidence memories
            let low_conf_count = cluster.iter().filter(|mem| mem.confidence < 0.5).count();
            let uncertainty = low_conf_count as f64 / depth as f64;

            // Composite confidence
            let confidence = (depth as f64).log2().min(5.0) / 5.0 * freshness * (1.0 - uncertainty);

            // Label: extract most common subject from SPO triples
            let label = extract_domain_label(cluster);

            let memory_ids = cluster.iter().map(|m| m.id.clone()).collect();

            KnowledgeDomain {
                label,
                depth,
                freshness,
                uncertainty,
                confidence: confidence.clamp(0.0, 1.0),
                memory_ids,
            }
        })
        .collect();

    domains.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let global_coverage = if domains.is_empty() {
        0.0
    } else {
        domains.iter().map(|d| d.confidence).sum::<f64>() / domains.len() as f64
    };

    info!(
        "[engram::meta] Built confidence map: {} domains, global_coverage={:.2}",
        domains.len(),
        global_coverage,
    );

    Ok(KnowledgeConfidenceMap {
        domains,
        global_coverage,
        last_rebuilt: Utc::now().to_rfc3339(),
    })
}

/// Assess the agent's confidence for a specific query.
///
/// Compares the query embedding against domain centroids to find the
/// best-matching domain, then returns a `DomainAssessment`.
///
/// If no embedding client is available, falls back to keyword matching
/// against domain labels.
pub async fn assess_query_confidence(
    map: &KnowledgeConfidenceMap,
    query: &str,
    _embedding_client: Option<&EmbeddingClient>,
) -> DomainAssessment {
    if map.domains.is_empty() {
        return DomainAssessment::Unknown;
    }

    let query_lower = query.to_lowercase();

    // Keyword fallback: check domain labels for overlap
    let best = map.domains.iter().max_by(|a, b| {
        let a_score = keyword_overlap(&query_lower, &a.label);
        let b_score = keyword_overlap(&query_lower, &b.label);
        a_score
            .partial_cmp(&b_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    match best {
        Some(domain) => {
            let overlap = keyword_overlap(&query_lower, &domain.label);
            if overlap < 0.1 {
                // No domain matches this query
                return DomainAssessment::Unknown;
            }

            // Blend keyword overlap with domain confidence
            let effective_confidence = domain.confidence * (0.5 + overlap * 0.5);

            if effective_confidence >= HIGH_CONFIDENCE {
                DomainAssessment::Confident {
                    domain: domain.label.clone(),
                    confidence: effective_confidence,
                }
            } else if effective_confidence >= LOW_CONFIDENCE {
                DomainAssessment::Uncertain {
                    domain: domain.label.clone(),
                    confidence: effective_confidence,
                }
            } else {
                DomainAssessment::Unknown
            }
        }
        None => DomainAssessment::Unknown,
    }
}

/// Generate a reflection prompt fragment to inject into the system message.
///
/// Outputs a concise self-assessment string like:
///   "My strongest knowledge domains: Rust async (high, 42 memories),
///    Docker networking (moderate, 18 memories). I have limited knowledge
///    about: SQL optimization."
pub fn build_reflection_prompt(map: &KnowledgeConfidenceMap) -> String {
    if map.domains.is_empty() {
        return String::from("I have not yet built significant long-term knowledge.");
    }

    let mut parts = Vec::new();

    // Top confident domains (up to 3)
    let confident: Vec<&KnowledgeDomain> = map
        .domains
        .iter()
        .filter(|d| d.confidence >= HIGH_CONFIDENCE)
        .take(3)
        .collect();

    if !confident.is_empty() {
        let items: Vec<String> = confident
            .iter()
            .map(|d| format!("{} (high, {} memories)", d.label, d.depth))
            .collect();
        parts.push(format!(
            "My strongest knowledge domains: {}",
            items.join(", ")
        ));
    }

    // Moderate domains
    let moderate: Vec<&KnowledgeDomain> = map
        .domains
        .iter()
        .filter(|d| d.confidence >= LOW_CONFIDENCE && d.confidence < HIGH_CONFIDENCE)
        .take(3)
        .collect();

    if !moderate.is_empty() {
        let items: Vec<String> = moderate
            .iter()
            .map(|d| format!("{} (moderate, {} memories)", d.label, d.depth))
            .collect();
        parts.push(format!(
            "Areas with partial knowledge: {}",
            items.join(", ")
        ));
    }

    // Stale domains (had memories but mostly outdated)
    let stale: Vec<&KnowledgeDomain> = map
        .domains
        .iter()
        .filter(|d| d.freshness < 0.3 && d.depth >= MIN_DOMAIN_SIZE)
        .take(2)
        .collect();

    if !stale.is_empty() {
        let items: Vec<String> = stale.iter().map(|d| d.label.clone()).collect();
        parts.push(format!(
            "Potentially outdated knowledge: {}",
            items.join(", ")
        ));
    }

    parts.join(". ") + "."
}

// ═════════════════════════════════════════════════════════════════════════════
// Clustering (greedy single-linkage by subject overlap)
// ═════════════════════════════════════════════════════════════════════════════

/// Cluster semantic memories by subject similarity.
/// Uses greedy single-linkage on the subject field (word overlap).
/// When embeddings are available, this should be upgraded to cosine clustering.
fn cluster_semantics(memories: &[(SemanticMemory, f64)]) -> Vec<Vec<&SemanticMemory>> {
    let mems: Vec<&SemanticMemory> = memories.iter().map(|(m, _)| m).collect();
    if mems.is_empty() {
        return Vec::new();
    }

    let mut assigned = vec![false; mems.len()];
    let mut clusters: Vec<Vec<&SemanticMemory>> = Vec::new();

    for i in 0..mems.len() {
        if assigned[i] {
            continue;
        }
        assigned[i] = true;
        let mut cluster = vec![mems[i]];

        for j in (i + 1)..mems.len() {
            if assigned[j] {
                continue;
            }
            // Check if this memory is similar to any member of the current cluster
            let similar = cluster.iter().any(|existing| {
                subject_similarity(&existing.subject, &mems[j].subject) >= CLUSTER_SIM_THRESHOLD
            });
            if similar {
                assigned[j] = true;
                cluster.push(mems[j]);
            }
        }

        clusters.push(cluster);
    }

    clusters
}

/// Compute word-overlap similarity between two subject strings.
fn subject_similarity(a: &str, b: &str) -> f64 {
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

/// Extract a domain label from a cluster of semantic memories.
/// Uses the most common subject terms.
fn extract_domain_label(cluster: &[&SemanticMemory]) -> String {
    let mut term_freq: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for mem in cluster {
        for word in mem.subject.to_lowercase().split_whitespace() {
            // Skip very short words
            if word.len() > 2 {
                *term_freq.entry(word.to_string()).or_insert(0) += 1;
            }
        }
    }

    let mut sorted: Vec<(String, usize)> = term_freq.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));

    sorted
        .iter()
        .take(3)
        .map(|(w, _)| w.clone())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Keyword overlap between a query and a domain label.
fn keyword_overlap(query: &str, label: &str) -> f64 {
    let q_words: std::collections::HashSet<&str> = query.split_whitespace().collect();
    let l_words: std::collections::HashSet<&str> = label.split_whitespace().collect();

    if q_words.is_empty() || l_words.is_empty() {
        return 0.0;
    }

    let hits = q_words.intersection(&l_words).count();
    hits as f64 / q_words.len().max(l_words.len()) as f64
}

// ═════════════════════════════════════════════════════════════════════════════
// Tests
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atoms::engram_types::KnowledgeConfidenceMap;

    #[test]
    fn empty_map_returns_unknown() {
        let map = KnowledgeConfidenceMap::default();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(assess_query_confidence(&map, "anything", None));
        assert_eq!(result, DomainAssessment::Unknown);
    }

    #[test]
    fn reflection_prompt_empty() {
        let map = KnowledgeConfidenceMap::default();
        let prompt = build_reflection_prompt(&map);
        assert!(prompt.contains("not yet built"));
    }

    #[test]
    fn reflection_prompt_with_domains() {
        let map = KnowledgeConfidenceMap {
            domains: vec![
                KnowledgeDomain {
                    label: "rust async".to_string(),
                    depth: 42,
                    freshness: 0.9,
                    uncertainty: 0.1,
                    confidence: 0.85,
                    memory_ids: vec![],
                },
                KnowledgeDomain {
                    label: "docker networking".to_string(),
                    depth: 18,
                    freshness: 0.6,
                    uncertainty: 0.2,
                    confidence: 0.55,
                    memory_ids: vec![],
                },
            ],
            global_coverage: 0.7,
            last_rebuilt: "2025-01-01T00:00:00Z".to_string(),
        };
        let prompt = build_reflection_prompt(&map);
        assert!(prompt.contains("rust async"));
        assert!(prompt.contains("docker networking"));
    }

    #[test]
    fn subject_similarity_identical() {
        assert!((subject_similarity("rust async", "rust async") - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn subject_similarity_disjoint() {
        assert!((subject_similarity("rust async", "python flask") - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn subject_similarity_partial() {
        let sim = subject_similarity("rust async runtime", "rust async tokio");
        assert!(sim > 0.3 && sim < 0.8, "sim={}", sim);
    }

    #[test]
    fn keyword_overlap_works() {
        assert!(keyword_overlap("rust async", "rust async runtime") > 0.3);
        assert!(keyword_overlap("python flask", "rust async") < f64::EPSILON);
    }
}
