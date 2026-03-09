// ── Engram: Bridge to Engine ─────────────────────────────────────────────────
//
// Bridge functions that connect the Engram memory system to the existing
// engine tools and commands. Provides the same interface as the old
// `engine::memory` module but routes through Engram's three-tier store.
//
// This bridge allows incremental migration:
//   - Agent tools (`memory_store`, `memory_search`) route through Engram
//   - Auto-capture stores to Engram's episodic tier
//   - Search uses Engram's BM25+vector+graph fusion
//   - The old `engine::memory` module remains for backward compatibility

use crate::atoms::engram_types::{
    ConsolidationState, EpisodicMemory, MemoryScope, MemorySearchConfig, MemorySource,
    TieredContent,
};
use crate::atoms::error::EngineResult;
use crate::engine::engram::encryption;
use crate::engine::memory::EmbeddingClient;
use crate::engine::sessions::SessionStore;
use log::{info, warn};

// ═════════════════════════════════════════════════════════════════════════════
// Store (bridge for memory_store tool)
// ═════════════════════════════════════════════════════════════════════════════

/// Store a memory through Engram. Equivalent to the old `memory::store_memory`.
///
/// Creates an episodic memory with deduplication. The `source` is marked as
/// `Explicit` for user/agent-initiated stores.
#[allow(clippy::too_many_arguments)]
pub async fn store(
    store: &SessionStore,
    content: &str,
    category: &str,
    importance: f32,
    embedding_client: Option<&EmbeddingClient>,
    agent_id: Option<&str>,
    session_id: Option<&str>,
    hnsw_index: Option<&super::hnsw::SharedHnswIndex>,
) -> EngineResult<Option<String>> {
    // §10.17 Input validation
    encryption::validate_memory_input(content, category)?;

    // §10.5 PII detection + field-level encryption
    let enc_key = encryption::get_memory_encryption_key().ok();
    let stored_content = if let Some(ref key) = enc_key {
        let prepared = encryption::prepare_for_storage(content, key)?;
        if prepared.tier != encryption::MemorySecurityTier::Cleartext {
            info!(
                "[engram] Memory classified as {:?} — encrypting ({} PII types detected)",
                prepared.tier,
                prepared.pii_types.len()
            );
        }
        prepared.content
    } else {
        warn!("[engram] No encryption key available — storing cleartext");
        content.to_string()
    };

    let id = uuid::Uuid::new_v4().to_string();

    let mem = EpisodicMemory {
        id: id.clone(),
        content: TieredContent::from_text(&stored_content),
        outcome: None,
        category: category.to_string(),
        importance: importance.clamp(0.0, 1.0),
        agent_id: agent_id.unwrap_or("default").to_string(),
        session_id: session_id.unwrap_or("unknown").to_string(),
        source: MemorySource::Explicit,
        consolidation_state: ConsolidationState::Fresh,
        strength: 1.0,
        scope: MemoryScope {
            global: agent_id.is_none(),
            agent_id: agent_id.map(|s| s.to_string()),
            ..Default::default()
        },
        embedding: None,
        embedding_model: None,
        negative_contexts: vec![],
        created_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        last_accessed_at: None,
        access_count: 0,
    };

    // Use dedup to avoid storing duplicates
    let result =
        super::graph::store_episodic_dedup(store, mem, embedding_client, None, hnsw_index).await?;

    Ok(result)
}

/// Store an auto-captured memory (from fact extraction or session summary).
///
/// Uses `AutoCapture` source and lower default importance.
#[allow(clippy::too_many_arguments)]
pub async fn store_auto_capture(
    store: &SessionStore,
    content: &str,
    category: &str,
    embedding_client: Option<&EmbeddingClient>,
    agent_id: Option<&str>,
    session_id: Option<&str>,
    channel: Option<&str>,
    channel_user_id: Option<&str>,
    hnsw_index: Option<&super::hnsw::SharedHnswIndex>,
) -> EngineResult<Option<String>> {
    // §10.17 Input validation (lenient for auto-capture — skip empty check)
    if content.len() > encryption::MAX_MEMORY_CONTENT_BYTES {
        warn!(
            "[engram] Auto-capture content too large ({} bytes), skipping",
            content.len()
        );
        return Ok(None);
    }

    // §10.5 PII-aware encryption
    let enc_key = encryption::get_memory_encryption_key().ok();
    let stored_content = if let Some(ref key) = enc_key {
        let prepared = encryption::prepare_for_storage(content, key)?;
        prepared.content
    } else {
        content.to_string()
    };

    let id = uuid::Uuid::new_v4().to_string();

    let mem = EpisodicMemory {
        id,
        content: TieredContent::from_text(&stored_content),
        outcome: None,
        category: category.to_string(),
        importance: 0.4, // auto-captured starts lower
        agent_id: agent_id.unwrap_or("default").to_string(),
        session_id: session_id.unwrap_or("unknown").to_string(),
        source: MemorySource::AutoCapture,
        consolidation_state: ConsolidationState::Fresh,
        strength: 1.0,
        scope: MemoryScope {
            global: false,
            agent_id: agent_id.map(|s| s.to_string()),
            channel: channel.map(|s| s.to_string()),
            channel_user_id: channel_user_id.map(|s| s.to_string()),
            ..Default::default()
        },
        embedding: None,
        embedding_model: None,
        negative_contexts: vec![],
        created_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        last_accessed_at: None,
        access_count: 0,
    };

    super::graph::store_episodic_dedup(store, mem, embedding_client, None, hnsw_index).await
}

// ═════════════════════════════════════════════════════════════════════════════
// Search (bridge for memory_search tool)
// ═════════════════════════════════════════════════════════════════════════════

/// A simplified search result for tool output and backward compatibility.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub id: String,
    pub content: String,
    pub category: String,
    pub memory_type: String,
    pub score: f64,
}

/// Search Engram memories. Equivalent to the old `memory::search_memories`
/// but uses BM25+vector+graph fusion.
pub async fn search(
    store: &SessionStore,
    query: &str,
    limit: usize,
    _threshold: f64,
    embedding_client: Option<&EmbeddingClient>,
    agent_id: Option<&str>,
) -> EngineResult<Vec<SearchResult>> {
    // §10.15 FTS5 query sanitization
    let sanitized_query = encryption::sanitize_fts5_query(query);
    if sanitized_query.is_empty() {
        return Ok(vec![]);
    }

    let scope = MemoryScope {
        global: agent_id.is_none(),
        agent_id: agent_id.map(|s| s.to_string()),
        ..Default::default()
    };

    let config = MemorySearchConfig::default();

    let recall_result = super::graph::search(
        store,
        &sanitized_query,
        &scope,
        &config,
        embedding_client,
        limit * 100,
        None, // No momentum blending from bridge (callers with WorkingMemory can pass it)
        None, // No HNSW index from bridge (callers with EngineState can pass it)
    )
    .await?;

    // §10.5 Decrypt encrypted content on retrieval (per-agent HKDF key)
    let mapped: Vec<SearchResult> = recall_result
        .memories
        .into_iter()
        .take(limit)
        .map(|r| {
            // Decrypt with per-agent derived key (HKDF isolation)
            let content = if let Ok(key) = encryption::get_agent_encryption_key(&r.agent_id) {
                encryption::decrypt_memory_content(&r.content, &key)
                    .unwrap_or_else(|_| r.content.clone())
            } else {
                r.content
            };
            // §10.14 Prompt injection scanning on recalled content
            let content = encryption::sanitize_recalled_memory(&content);

            SearchResult {
                id: r.memory_id,
                content,
                category: r.category,
                memory_type: r.memory_type.to_string(),
                score: r.trust_score.composite() as f64,
            }
        })
        .collect();

    // §10.12 Log redaction
    info!(
        "[engram:bridge] Search '{}' → {} results",
        encryption::safe_log_preview(query, 50),
        mapped.len()
    );

    Ok(mapped)
}

// ═════════════════════════════════════════════════════════════════════════════
// Stats
// ═════════════════════════════════════════════════════════════════════════════

/// Get memory statistics from Engram.
pub fn stats(store: &SessionStore) -> EngineResult<super::graph::EngramStats> {
    super::graph::memory_stats(store)
}

// ═════════════════════════════════════════════════════════════════════════════
// Maintenance (for background timer)
// ═════════════════════════════════════════════════════════════════════════════

/// Run all background maintenance tasks.
/// Call this periodically (e.g. every 5 minutes).
pub async fn run_maintenance(
    store: &SessionStore,
    embedding_client: Option<&EmbeddingClient>,
    half_life_days: f32,
    gc_importance_threshold: i32,
    hnsw_index: Option<&super::hnsw::SharedHnswIndex>,
) -> EngineResult<MaintenanceReport> {
    // 0. Key rotation check — rekey legacy-encrypted memories if overdue
    let mut rekey_count = 0usize;
    if super::encryption::should_rotate_keys(store) {
        match super::encryption::rekey_all_memories(store) {
            Ok(report) => {
                rekey_count = report.rekeyed;
                if rekey_count > 0 {
                    info!(
                        "[engram:maintenance] Key rotation: {} memories rekeyed",
                        rekey_count
                    );
                }
            }
            Err(e) => {
                warn!(
                    "[engram:maintenance] Key rotation failed (non-fatal): {}",
                    e
                );
            }
        }
    }

    // 1. Consolidation
    let consolidation =
        super::consolidation::run_consolidation(store, embedding_client, None).await?;

    // 1.5. Community detection (GraphRAG Louvain clustering)
    let mut communities_found = 0usize;
    match super::community_detection::detect_communities(store) {
        Ok((_communities, cd_report)) => {
            communities_found = cd_report.communities_found;
            if communities_found > 0 {
                info!(
                    "[engram:maintenance] Community detection: {} communities (Q={:.3})",
                    communities_found, cd_report.modularity
                );
            }
        }
        Err(e) => {
            warn!(
                "[engram:maintenance] Community detection failed (non-fatal): {}",
                e
            );
        }
    }

    // 2. Decay
    let decayed = super::graph::apply_decay(store, half_life_days)?;

    // 3. Garbage collection
    let gc_count = super::graph::garbage_collect(store, gc_importance_threshold, 100, hnsw_index)?;

    // 4. Self-healing gap injection (§4.5)
    // Convert detected knowledge gaps into natural-language prompts
    // that can be injected into working memory as clarifying questions.
    let gap_prompts: Vec<String> = consolidation
        .gaps
        .iter()
        .map(|gap| {
            use super::consolidation::GapKind;
            match &gap.kind {
                GapKind::StaleHighUse => {
                    format!("[Memory Gap] Frequently used knowledge may be outdated: {}. Consider verifying.", gap.description)
                }
                GapKind::UnresolvedContradiction => {
                    format!("[Memory Conflict] Contradictory facts detected: {}. Clarification needed.", gap.description)
                }
                GapKind::IncompleteSchema => {
                    format!("[Incomplete Knowledge] Only partial information exists: {}. More context would help.", gap.description)
                }
            }
        })
        .collect();

    // 5. Dream replay — hippocampal-inspired memory consolidation (§44)
    // Strengthens at-risk memories, re-embeds stale vectors, discovers latent connections.
    let global_scope = crate::atoms::engram_types::MemoryScope::global();
    let replay = super::dream_replay::run_replay(store, embedding_client, &global_scope).await;
    let (replay_strengthened, replay_reembedded, replay_connections) = match replay {
        Ok(r) => {
            if r.strengthened > 0 || r.re_embedded > 0 || r.new_connections > 0 {
                info!(
                    "[engram:maintenance] Dream replay: {} strengthened, {} re-embedded, {} connections ({}ms)",
                    r.strengthened, r.re_embedded, r.new_connections, r.duration_ms
                );
            }
            (r.strengthened, r.re_embedded, r.new_connections)
        }
        Err(e) => {
            warn!(
                "[engram:maintenance] Dream replay failed (non-fatal): {}",
                e
            );
            (0, 0, 0)
        }
    };

    // 6. Memory fusion — near-duplicate detection and merging (FadeMem §Fusion)
    // Highest-impact component per FadeMem ablation (-53.7% F1 without it).
    let (fused_count, fusion_tombstoned) = match super::memory_fusion::run_fusion(store) {
        Ok(fr) => {
            let total = fr.fused_compatible + fr.fused_contradictory + fr.fused_subsumed;
            if total > 0 {
                info!(
                    "[engram:maintenance] Fusion: {} fused, {} tombstoned, {} edges redirected",
                    total, fr.tombstoned, fr.edges_redirected
                );
            }
            (total, fr.tombstoned)
        }
        Err(e) => {
            warn!(
                "[engram:maintenance] Memory fusion failed (non-fatal): {}",
                e
            );
            (0, 0)
        }
    };

    let report = MaintenanceReport {
        consolidation,
        memories_decayed: decayed,
        memories_gc: gc_count,
        memories_rekeyed: rekey_count,
        communities_detected: communities_found,
        replay_strengthened,
        replay_reembedded,
        replay_connections,
        fused: fused_count,
        fusion_tombstoned,
        gap_prompts,
    };

    info!(
        "[engram:maintenance] consolidation: {} triples, decay: {}, gc: {}, rekey: {}, communities: {}, replay: {}/{}/{}, fusion: {}/{}",
        report.consolidation.triples_created, report.memories_decayed, report.memories_gc,
        report.memories_rekeyed, report.communities_detected,
        report.replay_strengthened, report.replay_reembedded, report.replay_connections,
        report.fused, report.fusion_tombstoned,
    );

    Ok(report)
}

/// Summary of a maintenance cycle.
#[derive(Debug, Clone)]
pub struct MaintenanceReport {
    pub consolidation: super::consolidation::ConsolidationReport,
    pub memories_decayed: usize,
    pub memories_gc: usize,
    /// Number of memories re-encrypted during key rotation (0 if no rotation needed).
    pub memories_rekeyed: usize,
    /// Number of communities discovered by Louvain detection.
    pub communities_detected: usize,
    /// Dream replay: memories strengthened above GC threshold.
    pub replay_strengthened: usize,
    /// Dream replay: memories re-embedded with current model.
    pub replay_reembedded: usize,
    /// Dream replay: new latent connections discovered.
    pub replay_connections: usize,
    /// Memory fusion: near-duplicate pairs fused.
    pub fused: usize,
    /// Memory fusion: memories tombstoned after fusion.
    pub fusion_tombstoned: usize,
    /// Knowledge gaps detected during consolidation, formatted for working memory injection.
    pub gap_prompts: Vec<String>,
}

// ═════════════════════════════════════════════════════════════════════════════
// Tests
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_result_format() {
        let result = SearchResult {
            id: "test-123".to_string(),
            content: "User prefers dark mode".to_string(),
            category: "preference".to_string(),
            memory_type: "Episodic".to_string(),
            score: 0.85,
        };
        assert_eq!(result.memory_type, "Episodic");
        assert!(result.score > 0.5);
    }
}
