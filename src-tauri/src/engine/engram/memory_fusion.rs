// ── Engram: Memory Fusion Pipeline (§ ENGRAM.md — Memory Fusion) ────────────
//
// Near-duplicate detection and merging for episodic memories.
// FadeMem ablation shows removing fusion causes -53.7% F1 drop —
// the highest-impact single component in the system.
//
// Pipeline:
//   1. Candidate detection — cosine similarity ≥ θ_fusion (0.75)
//   2. Relation classification — Compatible / Contradictory / Subsumes / Subsumed
//   3. Merge — create unified entry, transfer strength
//   4. Edge redirection — re-point all edges to the merged entry
//   5. Tombstoning — mark originals as archived (recoverable)
//   6. Audit — full provenance trail

use crate::atoms::engram_types::{
    ConsolidationState, EdgeType, EpisodicMemory, MemoryEdge, MemoryScope,
};
use crate::atoms::error::EngineResult;
use crate::engine::sessions::SessionStore;
use chrono::Utc;
use log::info;

// ═════════════════════════════════════════════════════════════════════════════
// Configuration
// ═════════════════════════════════════════════════════════════════════════════

/// Cosine similarity threshold for fusion candidates (from FadeMem paper).
const FUSION_SIMILARITY_THRESHOLD: f64 = 0.75;

/// Maximum fusion operations per cycle (prevents unbounded work).
const MAX_FUSIONS_PER_CYCLE: usize = 25;

/// Maximum memories to sample for fusion candidate detection.
const FUSION_SAMPLE_SIZE: usize = 200;

// ═════════════════════════════════════════════════════════════════════════════
// Relation Classification (FadeMem 4-type model)
// ═════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, PartialEq)]
enum FusionRelation {
    /// Both memories are true simultaneously → merge into one
    Compatible,
    /// Mutually exclusive claims → newer wins
    Contradictory,
    /// Memory A is a superset of B → absorb B into A
    Subsumes,
    /// Memory B is a superset of A → absorb A into B
    Subsumed,
}

/// Classify the relationship between two similar memories.
///
/// Uses word overlap asymmetry to detect subsumption:
///   - If A's words are a near-superset of B's → A subsumes B
///   - If B's words are a near-superset of A's → A is subsumed by B
///   - If overlap is symmetric and high → Compatible
///   - Otherwise check for contradiction signals
fn classify_relation(a: &EpisodicMemory, b: &EpisodicMemory) -> FusionRelation {
    let words_a: std::collections::HashSet<&str> = a
        .content
        .full
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| w.len() > 2)
        .collect();
    let words_b: std::collections::HashSet<&str> = b
        .content
        .full
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| w.len() > 2)
        .collect();

    if words_a.is_empty() || words_b.is_empty() {
        return FusionRelation::Compatible;
    }

    let intersection = words_a.intersection(&words_b).count();
    let coverage_a = intersection as f64 / words_a.len() as f64; // how much of A is in B
    let coverage_b = intersection as f64 / words_b.len() as f64; // how much of B is in A

    // Check for contradiction signals (negation words near overlap)
    let contradiction_signals = [
        "not",
        "no",
        "never",
        "incorrect",
        "wrong",
        "false",
        "isn't",
        "doesn't",
        "wasn't",
    ];
    let a_has_negation = words_a
        .iter()
        .any(|w| contradiction_signals.contains(&w.to_lowercase().as_str()));
    let b_has_negation = words_b
        .iter()
        .any(|w| contradiction_signals.contains(&w.to_lowercase().as_str()));

    if a_has_negation != b_has_negation && coverage_a > 0.5 {
        return FusionRelation::Contradictory;
    }

    // Subsumption: one memory contains nearly all of the other's information
    if coverage_b > 0.85 && coverage_a < 0.6 {
        // B's words are mostly in A (A is longer / more complete) → A subsumes B
        return FusionRelation::Subsumes;
    }
    if coverage_a > 0.85 && coverage_b < 0.6 {
        // A's words are mostly in B (B is longer / more complete) → A subsumed by B
        return FusionRelation::Subsumed;
    }

    // High symmetric overlap → compatible (near-duplicates)
    FusionRelation::Compatible
}

// ═════════════════════════════════════════════════════════════════════════════
// Public API
// ═════════════════════════════════════════════════════════════════════════════

/// Report from a single fusion cycle.
#[derive(Debug, Default, Clone)]
pub struct FusionReport {
    pub pairs_evaluated: usize,
    pub fused_compatible: usize,
    pub fused_contradictory: usize,
    pub fused_subsumed: usize,
    pub edges_redirected: usize,
    pub tombstoned: usize,
}

/// Run one fusion cycle. Should be called after consolidation within the
/// maintenance pipeline.
///
/// Scans embedded episodic memories for near-duplicate pairs (cosine ≥ 0.75),
/// classifies their relationship, merges or resolves, redirects edges,
/// and tombstones the originals.
pub fn run_fusion(store: &SessionStore) -> EngineResult<FusionReport> {
    let mut report = FusionReport::default();

    // ── 1. Fetch embedded memories for pairwise comparison ───────────
    let scope = MemoryScope::global();
    let candidates = store.engram_list_episodic(&scope, None, FUSION_SAMPLE_SIZE)?;

    let with_embeddings: Vec<&EpisodicMemory> = candidates
        .iter()
        .filter(|m| m.embedding.is_some())
        .filter(|m| m.consolidation_state != ConsolidationState::Archived)
        .collect();

    if with_embeddings.len() < 2 {
        return Ok(report);
    }

    // Track which IDs have already been fused this cycle to avoid double-fusing
    let mut fused_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    // ── 2. Pairwise cosine scan (O(n²) but bounded by FUSION_SAMPLE_SIZE) ──
    for i in 0..with_embeddings.len() {
        if report.fused_compatible + report.fused_contradictory + report.fused_subsumed
            >= MAX_FUSIONS_PER_CYCLE
        {
            break;
        }

        let a = with_embeddings[i];
        if fused_ids.contains(&a.id) {
            continue;
        }

        for &b in with_embeddings.iter().skip(i + 1) {
            if report.fused_compatible + report.fused_contradictory + report.fused_subsumed
                >= MAX_FUSIONS_PER_CYCLE
            {
                break;
            }

            if fused_ids.contains(&b.id) {
                continue;
            }

            // Scope compatibility: only fuse memories from same agent/scope
            if a.agent_id != b.agent_id {
                continue;
            }

            let (emb_a, emb_b) = match (&a.embedding, &b.embedding) {
                (Some(ea), Some(eb)) => (ea, eb),
                _ => continue,
            };

            let sim = cosine_sim(emb_a, emb_b);
            report.pairs_evaluated += 1;

            if sim < FUSION_SIMILARITY_THRESHOLD {
                continue;
            }

            let relation = classify_relation(a, b);
            let redirected = match relation {
                FusionRelation::Compatible => {
                    // Merge: keep the higher-importance one, append content, tombstone the other
                    let (keeper, loser) = if a.importance >= b.importance {
                        (a, b)
                    } else {
                        (b, a)
                    };
                    let merged_content =
                        format!("{}\n\n{}", keeper.content.full, loser.content.full);
                    store.engram_update_episodic_content(&keeper.id, &merged_content, None)?;
                    // Boost the keeper's strength
                    super::graph::boost_slow_strength(store, &keeper.id).ok();
                    let r = redirect_edges(store, &loser.id, &keeper.id)?;
                    tombstone(store, &loser.id)?;
                    report.fused_compatible += 1;
                    audit_fusion(store, &keeper.id, &loser.id, "compatible")?;
                    fused_ids.insert(loser.id.to_string());
                    r
                }
                FusionRelation::Contradictory => {
                    // Newer wins, old gets tombstoned, Contradicts edge created
                    let (newer, older) = if a.created_at >= b.created_at {
                        (a, b)
                    } else {
                        (b, a)
                    };
                    store.engram_add_edge(&MemoryEdge {
                        source_id: newer.id.clone(),
                        target_id: older.id.clone(),
                        edge_type: EdgeType::Contradicts,
                        weight: sim as f32,
                        created_at: Utc::now().to_rfc3339(),
                    })?;
                    let r = redirect_edges(store, &older.id, &newer.id)?;
                    tombstone(store, &older.id)?;
                    report.fused_contradictory += 1;
                    audit_fusion(store, &newer.id, &older.id, "contradictory")?;
                    fused_ids.insert(older.id.to_string());
                    r
                }
                FusionRelation::Subsumes => {
                    // A subsumes B → keep A, tombstone B
                    super::graph::boost_slow_strength(store, &a.id).ok();
                    let r = redirect_edges(store, &b.id, &a.id)?;
                    tombstone(store, &b.id)?;
                    report.fused_subsumed += 1;
                    audit_fusion(store, &a.id, &b.id, "subsumes")?;
                    fused_ids.insert(b.id.to_string());
                    r
                }
                FusionRelation::Subsumed => {
                    // B subsumes A → keep B, tombstone A
                    super::graph::boost_slow_strength(store, &b.id).ok();
                    let r = redirect_edges(store, &a.id, &b.id)?;
                    tombstone(store, &a.id)?;
                    report.fused_subsumed += 1;
                    audit_fusion(store, &b.id, &a.id, "subsumed")?;
                    fused_ids.insert(a.id.to_string());
                    r
                }
            };
            report.edges_redirected += redirected;
        }
    }

    report.tombstoned = fused_ids.len();

    if report.tombstoned > 0 {
        info!(
            "[engram::fusion] Fused {} pairs ({}C/{}X/{}S), redirected {} edges, tombstoned {}",
            report.fused_compatible + report.fused_contradictory + report.fused_subsumed,
            report.fused_compatible,
            report.fused_contradictory,
            report.fused_subsumed,
            report.edges_redirected,
            report.tombstoned,
        );
    }

    Ok(report)
}

// ═════════════════════════════════════════════════════════════════════════════
// Internal Helpers
// ═════════════════════════════════════════════════════════════════════════════

/// Redirect all edges pointing to `old_id` to point to `new_id` instead.
/// Returns the number of edges redirected.
fn redirect_edges(store: &SessionStore, old_id: &str, new_id: &str) -> EngineResult<usize> {
    let mut count = 0;

    // Redirect outgoing edges (old_id as source)
    let outgoing = store.engram_get_edges_from(old_id)?;
    for edge in &outgoing {
        if edge.target_id == new_id {
            // Self-loop after redirect — just remove
            store.engram_remove_edge(old_id, &edge.target_id, &edge.edge_type)?;
        } else {
            store.engram_add_edge(&MemoryEdge {
                source_id: new_id.to_string(),
                target_id: edge.target_id.clone(),
                edge_type: edge.edge_type.clone(),
                weight: edge.weight,
                created_at: edge.created_at.clone(),
            })?;
            store.engram_remove_edge(old_id, &edge.target_id, &edge.edge_type)?;
        }
        count += 1;
    }

    // Redirect incoming edges (old_id as target)
    let incoming = store.engram_get_edges_to(old_id)?;
    for edge in &incoming {
        if edge.source_id == new_id {
            store.engram_remove_edge(&edge.source_id, old_id, &edge.edge_type)?;
        } else {
            store.engram_add_edge(&MemoryEdge {
                source_id: edge.source_id.clone(),
                target_id: new_id.to_string(),
                edge_type: edge.edge_type.clone(),
                weight: edge.weight,
                created_at: edge.created_at.clone(),
            })?;
            store.engram_remove_edge(&edge.source_id, old_id, &edge.edge_type)?;
        }
        count += 1;
    }

    Ok(count)
}

/// Mark a memory as archived (tombstoned). Recoverable — content is preserved.
fn tombstone(store: &SessionStore, id: &str) -> EngineResult<()> {
    store.engram_set_consolidation_state(id, ConsolidationState::Archived)
}

/// Audit a fusion event with full provenance.
fn audit_fusion(
    store: &SessionStore,
    keeper_id: &str,
    loser_id: &str,
    relation: &str,
) -> EngineResult<()> {
    store.engram_audit_log(
        "memory_fusion",
        keeper_id,
        "system",
        "fusion",
        Some(&format!("relation={} absorbed={}", relation, loser_id)),
    )
}

/// Cosine similarity between two f32 vectors.
fn cosine_sim(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let (mut dot, mut na, mut nb) = (0.0f64, 0.0f64, 0.0f64);
    for (x, y) in a.iter().zip(b.iter()) {
        let (fx, fy) = (*x as f64, *y as f64);
        dot += fx * fy;
        na += fx * fx;
        nb += fy * fy;
    }
    let denom = na.sqrt() * nb.sqrt();
    if denom < 1e-12 {
        0.0
    } else {
        dot / denom
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Tests
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atoms::engram_types::{MemorySource, TieredContent};

    fn make_memory(id: &str, content: &str, importance: f32) -> EpisodicMemory {
        EpisodicMemory {
            id: id.to_string(),
            content: TieredContent {
                full: content.to_string(),
                summary: None,
                key_fact: None,
                tags: None,
            },
            category: "general".to_string(),
            importance,
            agent_id: "default".to_string(),
            session_id: "test".to_string(),
            source: MemorySource::Explicit,
            consolidation_state: ConsolidationState::Fresh,
            scope: MemoryScope::global(),
            ..EpisodicMemory::default()
        }
    }

    #[test]
    fn test_classify_compatible() {
        let a = make_memory("a", "The user prefers dark mode for coding", 5.0);
        let b = make_memory("b", "The user prefers dark mode in editors", 5.0);
        let rel = classify_relation(&a, &b);
        assert_eq!(rel, FusionRelation::Compatible);
    }

    #[test]
    fn test_classify_contradictory() {
        let a = make_memory("a", "The server runs on port 8080 for production", 5.0);
        let b = make_memory(
            "b",
            "The server does not run on port 8080 for production",
            5.0,
        );
        let rel = classify_relation(&a, &b);
        assert_eq!(rel, FusionRelation::Contradictory);
    }

    #[test]
    fn test_classify_subsumes() {
        let a = make_memory("a", "The deployment pipeline uses Docker containers with nginx reverse proxy and automated health checks on port 443 with SSL termination", 5.0);
        let b = make_memory("b", "deployment uses Docker", 5.0);
        let rel = classify_relation(&a, &b);
        assert_eq!(rel, FusionRelation::Subsumes);
    }

    #[test]
    fn test_fusion_report_default() {
        let report = FusionReport::default();
        assert_eq!(report.tombstoned, 0);
        assert_eq!(report.pairs_evaluated, 0);
    }
}
