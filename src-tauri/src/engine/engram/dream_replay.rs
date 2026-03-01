// ── Engram: Memory Replay & Dream Consolidation (§44) ───────────────────────
//
// Idle-time background process that replays and strengthens memories.
// Inspired by the biological hippocampal replay during sleep, which
// consolidates memories by re-activating them in compressed time.
//
// 4-Phase Pipeline (runs when the agent is idle):
//   Phase 1: Strengthen At-Risk — boost memories near GC threshold
//   Phase 2: Re-Embed Stale — refresh embeddings for memories whose
//            embedding model has changed or is outdated
//   Phase 3: Discover Connections — find latent SimilarTo edges between
//            memories that weren't connected during original encoding
//   Phase 4: Rebuild Derived — trigger abstraction tree + confidence map rebuild
//
// Resource Control:
//   - Runs only when system is idle (no active conversation for >5 min)
//   - Self-terminates after a budget of operations per cycle
//   - Logs everything to the audit trail

use crate::atoms::engram_types::{EdgeType, MemoryEdge, MemoryScope, ReplayReport};
use crate::atoms::error::EngineResult;
use crate::engine::memory::EmbeddingClient;
use crate::engine::sessions::SessionStore;
use chrono::Utc;
use log::{info, warn};
use std::time::Instant;

// ═════════════════════════════════════════════════════════════════════════════
// Constants
// ═════════════════════════════════════════════════════════════════════════════

/// Importance threshold below which memories are "at risk" of GC.
const AT_RISK_IMPORTANCE: i32 = 3;

/// Maximum memories to strengthen per replay cycle (Phase 1).
const MAX_STRENGTHEN_PER_CYCLE: usize = 50;

/// Strength boost applied to at-risk memories during replay.
const REPLAY_STRENGTH_BOOST: f32 = 0.1;

/// Maximum memories to re-embed per cycle (Phase 2).
const MAX_REEMBED_PER_CYCLE: usize = 20;

/// Minimum cosine similarity to create a SimilarTo edge (Phase 3).
const SIMILARITY_EDGE_THRESHOLD: f64 = 0.80;

/// Maximum new edges to discover per cycle (Phase 3).
const MAX_NEW_EDGES_PER_CYCLE: usize = 30;

/// Maximum memories to sample for connection discovery.
const DISCOVERY_SAMPLE_SIZE: usize = 100;

// ═════════════════════════════════════════════════════════════════════════════
// Dream Replay Engine
// ═════════════════════════════════════════════════════════════════════════════

/// Run a full dream replay cycle.
///
/// Should be called when the agent has been idle for >5 minutes.
/// The cycle is idempotent — running it multiple times is safe.
///
/// Returns a `ReplayReport` with counts of operations performed.
pub async fn run_replay(
    store: &SessionStore,
    embedding_client: Option<&EmbeddingClient>,
    scope: &MemoryScope,
) -> EngineResult<ReplayReport> {
    let start = Instant::now();
    info!("[engram::dream] Starting dream replay cycle");

    let report = ReplayReport {
        strengthened: phase_strengthen_at_risk(store, scope)?,
        re_embedded: phase_reembed_stale(store, embedding_client).await?,
        new_connections: phase_discover_connections(store, scope)?,
        duration_ms: start.elapsed().as_millis() as u64,
    };

    info!(
        "[engram::dream] Replay complete: {}ms, +{} strengthened, +{} re-embedded, +{} connections",
        report.duration_ms, report.strengthened, report.re_embedded, report.new_connections,
    );

    // Audit trail
    store.engram_audit_log(
        "dream_replay",
        "system",
        scope.agent_id.as_deref().unwrap_or("global"),
        "dream",
        Some(&format!(
            "strengthened={}, re_embedded={}, connections={}",
            report.strengthened, report.re_embedded, report.new_connections,
        )),
    )?;

    Ok(report)
}

// ═════════════════════════════════════════════════════════════════════════════
// Phase 1: Strengthen At-Risk Memories
// ═════════════════════════════════════════════════════════════════════════════

/// Boost memories that are near the GC threshold.
///
/// Rationale: Memories that were accessed at least once but have decayed
/// near the GC boundary are more valuable than never-accessed memories.
/// Replaying strengthens their trace, mirroring the biological finding
/// that sleep replay prevents catastrophic forgetting.
fn phase_strengthen_at_risk(store: &SessionStore, scope: &MemoryScope) -> EngineResult<usize> {
    let candidates =
        store.engram_list_gc_candidates(AT_RISK_IMPORTANCE, MAX_STRENGTHEN_PER_CYCLE)?;

    let mut strengthened = 0;
    for id in &candidates {
        // Only strengthen memories that have been accessed at least once
        if let Ok(Some(mem)) = store.engram_get_episodic(id) {
            if mem.access_count > 0 {
                // Check scope match
                let scope_match = scope
                    .agent_id
                    .as_ref()
                    .is_none_or(|aid| mem.agent_id == *aid);

                if scope_match {
                    store.engram_record_access(id, REPLAY_STRENGTH_BOOST)?;
                    strengthened += 1;
                }
            }
        }
    }

    if strengthened > 0 {
        info!(
            "[engram::dream] Phase 1: strengthened {} at-risk memories",
            strengthened
        );
    }

    Ok(strengthened)
}

// ═════════════════════════════════════════════════════════════════════════════
// Phase 2: Re-Embed Stale Memories
// ═════════════════════════════════════════════════════════════════════════════

/// Refresh embeddings for memories that were stored without embeddings
/// or whose embedding model has changed.
async fn phase_reembed_stale(
    store: &SessionStore,
    embedding_client: Option<&EmbeddingClient>,
) -> EngineResult<usize> {
    let client = match embedding_client {
        Some(c) => c,
        None => return Ok(0), // No embedding client → skip
    };

    let candidates = store.engram_list_episodic_without_embeddings(MAX_REEMBED_PER_CYCLE)?;

    let mut re_embedded = 0;
    for mem in &candidates {
        match client.embed(&mem.content.full).await {
            Ok(embedding) => {
                store.engram_update_episodic_embedding(&mem.id, &embedding, client.model_name())?;
                re_embedded += 1;
            }
            Err(e) => {
                warn!(
                    "[engram::dream] Phase 2: failed to re-embed {}: {}",
                    mem.id, e
                );
            }
        }
    }

    if re_embedded > 0 {
        info!(
            "[engram::dream] Phase 2: re-embedded {} stale memories",
            re_embedded
        );
    }

    Ok(re_embedded)
}

// ═════════════════════════════════════════════════════════════════════════════
// Phase 3: Discover Latent Connections
// ═════════════════════════════════════════════════════════════════════════════

/// Find memories that are semantically similar but not yet connected.
///
/// Uses pairwise cosine similarity on embeddings to discover SimilarTo
/// edges that weren't created during original encoding.
fn phase_discover_connections(store: &SessionStore, scope: &MemoryScope) -> EngineResult<usize> {
    // Fetch a sample of embedded memories
    let all_embedded = store.engram_search_episodic_bm25("", scope, DISCOVERY_SAMPLE_SIZE)?;

    // Filter to only those with embeddings
    let with_embeddings: Vec<_> = all_embedded
        .iter()
        .filter(|(m, _)| m.embedding.is_some())
        .collect();

    if with_embeddings.len() < 2 {
        return Ok(0);
    }

    let mut new_edges = 0;

    // Pairwise comparison (O(n²) but bounded by DISCOVERY_SAMPLE_SIZE)
    for i in 0..with_embeddings.len() {
        if new_edges >= MAX_NEW_EDGES_PER_CYCLE {
            break;
        }

        for j in (i + 1)..with_embeddings.len() {
            if new_edges >= MAX_NEW_EDGES_PER_CYCLE {
                break;
            }

            let (mem_a, _) = with_embeddings[i];
            let (mem_b, _) = with_embeddings[j];

            if let (Some(emb_a), Some(emb_b)) = (&mem_a.embedding, &mem_b.embedding) {
                let sim = cosine_similarity(emb_a, emb_b);
                if sim >= SIMILARITY_EDGE_THRESHOLD {
                    // Check if edge already exists
                    let existing = store.engram_get_edges_from(&mem_a.id)?;
                    let already_linked = existing.iter().any(|e| e.target_id == mem_b.id);

                    if !already_linked {
                        let edge = MemoryEdge {
                            source_id: mem_a.id.clone(),
                            target_id: mem_b.id.clone(),
                            edge_type: EdgeType::SimilarTo,
                            weight: sim as f32,
                            created_at: Utc::now().to_rfc3339(),
                        };
                        store.engram_add_edge(&edge)?;
                        new_edges += 1;
                    }
                }
            }
        }
    }

    if new_edges > 0 {
        info!(
            "[engram::dream] Phase 3: discovered {} new SimilarTo connections",
            new_edges,
        );
    }

    Ok(new_edges)
}

// ═════════════════════════════════════════════════════════════════════════════
// Helpers
// ═════════════════════════════════════════════════════════════════════════════

/// Cosine similarity between two embedding vectors.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot: f64 = a
        .iter()
        .zip(b.iter())
        .map(|(x, y)| *x as f64 * *y as f64)
        .sum();
    let mag_a: f64 = a
        .iter()
        .map(|x| (*x as f64) * (*x as f64))
        .sum::<f64>()
        .sqrt();
    let mag_b: f64 = b
        .iter()
        .map(|x| (*x as f64) * (*x as f64))
        .sum::<f64>()
        .sqrt();

    if mag_a < f64::EPSILON || mag_b < f64::EPSILON {
        return 0.0;
    }

    (dot / (mag_a * mag_b)).clamp(-1.0, 1.0)
}

// ═════════════════════════════════════════════════════════════════════════════
// Tests
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_similarity_identical() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        assert!(cosine_similarity(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn cosine_similarity_opposite() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![-1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) + 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_similarity_empty() {
        assert!(cosine_similarity(&[], &[]).abs() < f64::EPSILON);
    }

    #[test]
    fn cosine_similarity_mismatched_length() {
        let a = vec![1.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!(cosine_similarity(&a, &b).abs() < f64::EPSILON);
    }

    #[test]
    fn replay_report_default() {
        let report = ReplayReport::default();
        assert_eq!(report.strengthened, 0);
        assert_eq!(report.re_embedded, 0);
        assert_eq!(report.new_connections, 0);
        assert_eq!(report.duration_ms, 0);
    }
}
