// ── Engram: Memory Graph (Business Logic Layer) ─────────────────────────────
//
// High-level operations on the three-tier memory system.
// Delegates DB I/O to SessionStore methods in sessions/engram.rs.
//
// Responsibilities:
//   - Store with deduplication (Jaccard + embedding cosine)
//   - Unified search across all three tiers (BM25 + vector + graph)
//   - Reciprocal Rank Fusion (RRF) for result merging
//   - Trust-score computation on retrieval
//   - Relationship creation between memories
//   - Strength decay (FadeMem dual-layer: fast activation + slow consolidation)
//   - Audit trail

use crate::atoms::engram_types::{
    CompressionLevel, EdgeType, EpisodicMemory, MemoryEdge, MemoryScope, MemorySearchConfig,
    MemoryType, ProceduralMemory, RecallResult, RetrievedMemory, SemanticMemory, TieredContent,
    TrustScore,
};
use crate::atoms::error::EngineResult;
use crate::engine::engram::encryption::{
    get_agent_encryption_key, prepare_for_storage, MemorySecurityTier,
};
use crate::engine::engram::hybrid_search::resolve_hybrid_weight;
use crate::engine::engram::reranking::{cross_type_dedup, rerank_results};
use crate::engine::engram::retrieval_quality::build_recall_result;
use crate::engine::engram::tokenizer::Tokenizer;
use crate::engine::memory::EmbeddingClient;
use crate::engine::sessions::SessionStore;
use log::{info, warn};

// ═════════════════════════════════════════════════════════════════════════════
// Constants
// ═════════════════════════════════════════════════════════════════════════════

/// Default Jaccard overlap threshold for dedup (word-level).
const DEDUP_JACCARD_THRESHOLD: f64 = 0.6;

/// Default number of recent memories to check for dedup.
const DEDUP_WINDOW: usize = 100;

/// RRF constant (k=60 is standard in information retrieval).
const RRF_K: f64 = 60.0;

/// Strength boost on each retrieval (spacing effect).
const RETRIEVAL_STRENGTH_BOOST: f32 = 0.05;

/// Temporal decay lambda for Ebbinghaus curve (used in FadeMem slow layer).
/// Memory half-life of 30 days: lambda = ln(2) / 30.
#[allow(dead_code)]
const DECAY_LAMBDA: f64 = 0.0231;

// ═════════════════════════════════════════════════════════════════════════════
// Store with Dedup
// ═════════════════════════════════════════════════════════════════════════════

/// Store an episodic memory with word-level deduplication.
/// Returns `Some(id)` if stored, `None` if deduplicated (too similar to existing).
pub async fn store_episodic_dedup(
    store: &SessionStore,
    mut mem: EpisodicMemory,
    embedding_client: Option<&EmbeddingClient>,
    dedup_threshold: Option<f64>,
    hnsw_index: Option<&super::hnsw::SharedHnswIndex>,
) -> EngineResult<Option<String>> {
    let threshold = dedup_threshold.unwrap_or(DEDUP_JACCARD_THRESHOLD);

    // Optionally compute embedding
    if let Some(client) = embedding_client {
        match client.embed(&mem.content.full).await {
            Ok(emb) => {
                mem.embedding_model = Some(client.model_name().to_string());
                mem.embedding = Some(emb);
            }
            Err(e) => {
                warn!("[engram] Failed to embed episodic memory: {}", e);
            }
        }
    }

    // Check for content overlap with recent episodic memories
    // We use a simple BM25 search on the content to find candidates, then Jaccard check
    let scope = mem.scope.clone();
    let candidates = store.engram_search_episodic_bm25(&mem.content.full, &scope, DEDUP_WINDOW)?;

    for (existing, _score) in &candidates {
        let overlap = content_overlap(&mem.content.full, &existing.content.full);
        if overlap > threshold {
            info!(
                "[engram] Dedup: skipping episodic memory (overlap {:.2} > {:.2} with {})",
                overlap, threshold, existing.id
            );
            // Boost the existing memory's strength instead
            store.engram_record_access(&existing.id, RETRIEVAL_STRENGTH_BOOST)?;
            return Ok(None);
        }
    }

    let id = mem.id.clone();

    // ── Encrypt memory content at rest ─────────────────────────────────
    // Uses HKDF-derived per-agent key so each agent's memories are
    // cryptographically isolated. A compromised derived key for agent A
    // cannot decrypt agent B's memories.
    match get_agent_encryption_key(&mem.agent_id) {
        Ok(key) => match prepare_for_storage(&mem.content.full, &key) {
            Ok(encrypted) => {
                mem.content.full = encrypted.content;
                // For Sensitive tier, preserve a safe summary for FTS search
                if encrypted.tier == MemorySecurityTier::Sensitive {
                    if let Some(summary) = encrypted.cleartext_summary {
                        if mem.content.summary.is_none() {
                            mem.content.summary = Some(summary);
                        }
                    }
                }
                if !encrypted.pii_types.is_empty() {
                    info!(
                        "[engram] Encrypted episodic memory {} (tier={:?}, pii={:?})",
                        id, encrypted.tier, encrypted.pii_types
                    );
                }
            }
            Err(e) => {
                warn!(
                    "[engram] Failed to encrypt memory {}: {} — storing cleartext",
                    id, e
                );
            }
        },
        Err(e) => {
            warn!(
                "[engram] No encryption key available: {} — storing cleartext",
                e
            );
        }
    }

    store.engram_store_episodic(&mem)?;

    // Incremental HNSW update — keep the in-memory index current
    if let (Some(idx), Some(ref embedding)) = (hnsw_index, &mem.embedding) {
        super::hnsw::insert_shared(idx, &id, embedding.clone());
    }

    // Audit
    store.engram_audit_log("store", &id, &mem.agent_id, &mem.session_id, None)?;

    info!("[engram] ✓ Stored episodic memory {}", id);
    Ok(Some(id))
}

/// Store a semantic memory (SPO triple). If a triple with the same subject+predicate
/// already exists, update the object and bump version instead of duplicating.
pub async fn store_semantic_dedup(
    store: &SessionStore,
    mut mem: SemanticMemory,
    embedding_client: Option<&EmbeddingClient>,
) -> EngineResult<String> {
    // Check for existing triple with same subject+predicate
    let existing = store.engram_lookup_by_subject(&mem.subject, &mem.scope)?;
    for existing_mem in &existing {
        if existing_mem.predicate == mem.predicate {
            if existing_mem.object == mem.object {
                // Exact duplicate — skip
                info!(
                    "[engram] Dedup: semantic triple already exists ({})",
                    existing_mem.id
                );
                return Ok(existing_mem.id.clone());
            }
            // Same subject+predicate, different object → update (reconsolidation)
            let mut updated = existing_mem.clone();
            updated.object = mem.object.clone();
            updated.full_text = format!("{} {} {}", mem.subject, mem.predicate, mem.object);
            updated.version += 1;
            updated.updated_at = Some(chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string());

            // If the new value contradicts, note it
            if mem.contradiction_of.is_some() {
                updated.contradiction_of = mem.contradiction_of.clone();
            }

            // Optionally re-embed
            if let Some(client) = embedding_client {
                match client.embed(&updated.full_text).await {
                    Ok(emb) => {
                        updated.embedding_model = Some(client.model_name().to_string());
                        updated.embedding = Some(emb);
                    }
                    Err(e) => warn!("[engram] Failed to re-embed semantic memory: {}", e),
                }
            }

            store.engram_store_semantic(&updated)?;
            info!(
                "[engram] ✓ Updated semantic triple {} (v{})",
                updated.id, updated.version
            );
            return Ok(updated.id.clone());
        }
    }

    // New triple
    if let Some(client) = embedding_client {
        match client.embed(&mem.full_text).await {
            Ok(emb) => {
                mem.embedding_model = Some(client.model_name().to_string());
                mem.embedding = Some(emb);
            }
            Err(e) => warn!("[engram] Failed to embed semantic memory: {}", e),
        }
    }

    let id = mem.id.clone();

    // ── Encrypt semantic memory content at rest ────────────────────────
    // Derive per-agent key for the owning agent.
    let agent_id_for_key = mem.scope.agent_id.as_deref().unwrap_or("");
    match get_agent_encryption_key(agent_id_for_key) {
        Ok(key) => match prepare_for_storage(&mem.full_text, &key) {
            Ok(encrypted) => {
                if !encrypted.pii_types.is_empty() {
                    mem.full_text = encrypted.content;
                    // Also encrypt the object field which may contain PII
                    if let Ok(obj_enc) = prepare_for_storage(&mem.object, &key) {
                        mem.object = obj_enc.content;
                    }
                    info!(
                        "[engram] Encrypted semantic memory {} (tier={:?}, pii={:?})",
                        id, encrypted.tier, encrypted.pii_types
                    );
                }
            }
            Err(e) => {
                warn!("[engram] Failed to encrypt semantic memory {}: {}", id, e);
            }
        },
        Err(e) => {
            warn!("[engram] No encryption key for semantic memory: {}", e);
        }
    }

    store.engram_store_semantic(&mem)?;
    info!("[engram] ✓ Stored semantic triple {}", id);
    Ok(id)
}

/// Store a procedural memory.
pub fn store_procedural(store: &SessionStore, mem: &ProceduralMemory) -> EngineResult<String> {
    let id = mem.id.clone();
    store.engram_store_procedural(mem)?;
    info!("[engram] ✓ Stored procedural memory {}", id);
    Ok(id)
}

// ═════════════════════════════════════════════════════════════════════════════
// Unified Search (Reciprocal Rank Fusion)
// ═════════════════════════════════════════════════════════════════════════════

/// Unified search across episodic + semantic + procedural memories.
/// Uses BM25 + vector similarity, fused with Reciprocal Rank Fusion (RRF).
/// Results are scored, trust-weighted, reranked, deduped, and sorted by composite score.
///
/// `momentum_embeddings` — optional recent query embeddings from WorkingMemory.
/// When provided, the current query embedding is blended with the weighted average
/// of momentum vectors (§8.6 trajectory-aware recall). Blend ratio: 0.7 current, 0.3 momentum.
///
/// Returns a `RecallResult` containing both the memories and retrieval quality metrics.
#[allow(clippy::too_many_arguments)]
pub async fn search(
    store: &SessionStore,
    query: &str,
    scope: &MemoryScope,
    config: &MemorySearchConfig,
    embedding_client: Option<&EmbeddingClient>,
    budget_tokens: usize,
    momentum_embeddings: Option<&[Vec<f32>]>,
    hnsw_index: Option<&super::hnsw::SharedHnswIndex>,
) -> EngineResult<RecallResult> {
    let search_start = std::time::Instant::now();
    let mut all_results: Vec<RetrievedMemory> = Vec::new();
    let search_limit = 50; // Retrieve more candidates, then budget-trim

    // ── BM25 search (episodic + semantic) ────────────────────────────
    let bm25_episodic = store.engram_search_episodic_bm25(query, scope, search_limit)?;
    let bm25_semantic = store.engram_search_semantic_bm25(query, scope, search_limit)?;

    // ── Vector search (if embedding client available) ────────────────
    // §8.6 Momentum blending: when momentum embeddings are available,
    // blend the current query embedding with the weighted average of
    // recent momentum vectors. This biases recall toward conversation trajectory.
    let mut vec_episodic: Vec<(EpisodicMemory, f64)> = Vec::new();
    // §8.6 Capture the raw query embedding before momentum blending
    // so callers can push it into the momentum vector for future queries.
    let mut raw_query_embedding: Option<Vec<f32>> = None;
    if let Some(client) = embedding_client {
        match client.embed(query).await {
            Ok(query_emb) => {
                // Capture the raw embedding before any blending
                raw_query_embedding = Some(query_emb.clone());

                // Apply momentum blending if we have trajectory history
                let search_emb = if let Some(mom) = momentum_embeddings {
                    if !mom.is_empty() && !query_emb.is_empty() {
                        blend_momentum(&query_emb, mom, 0.7)
                    } else {
                        query_emb
                    }
                } else {
                    query_emb
                };

                vec_episodic = match hnsw_index {
                    Some(idx) if !super::hnsw::is_empty_shared(idx) => {
                        // HNSW O(log n) approximate nearest-neighbor search.
                        // Over-fetch to compensate for post-hoc scope + model filtering.
                        // Adaptive: if initial fetch yields < 50% after filtering, widen or fall back.
                        let model_name = client.model_name();
                        // §5 Self-tuning: use the recall tuner's adapted threshold
                        // instead of the static config value. Falls back to config
                        // on first run before the tuner has enough samples.
                        let adapted = super::recall_tuner::current_threshold();
                        let threshold = if adapted != config.similarity_threshold as f64 {
                            adapted
                        } else {
                            config.similarity_threshold as f64
                        };
                        let initial_k = search_limit * 3;

                        let hnsw_results =
                            super::hnsw::search_shared(idx, &search_emb, initial_k, threshold);

                        // Batch-fetch all candidate memories in a single SQL query
                        let candidate_ids: Vec<String> =
                            hnsw_results.iter().map(|hr| hr.memory_id.clone()).collect();
                        let batch = store
                            .engram_get_episodic_batch(&candidate_ids)
                            .unwrap_or_default();

                        // Apply scope + model filters
                        let mut filtered: Vec<(EpisodicMemory, f64)> = hnsw_results
                            .into_iter()
                            .filter_map(|hr| {
                                batch.get(&hr.memory_id).and_then(|mem| {
                                    if !crate::engine::sessions::engram::scope_matches(
                                        scope, &mem.scope,
                                    ) {
                                        return None;
                                    }
                                    if !model_name.is_empty()
                                        && mem.embedding_model.as_deref() != Some(model_name)
                                    {
                                        return None;
                                    }
                                    Some((mem.clone(), hr.similarity))
                                })
                            })
                            .collect();

                        // Adaptive fallback: if HNSW+scope filtering yields too few results,
                        // fall back to brute-force which searches within scope natively.
                        // This prevents quality regression for minority-scope agents.
                        if filtered.len() < search_limit / 2 {
                            store.engram_search_episodic_vector(
                                &search_emb,
                                model_name,
                                scope,
                                search_limit,
                                threshold,
                            )?
                        } else {
                            filtered.truncate(search_limit);
                            filtered
                        }
                    }
                    _ => {
                        // Brute-force O(n) fallback when HNSW is unavailable or empty
                        let adapted = super::recall_tuner::current_threshold();
                        store.engram_search_episodic_vector(
                            &search_emb,
                            client.model_name(),
                            scope,
                            search_limit,
                            adapted,
                        )?
                    }
                };
            }
            Err(e) => {
                warn!("[engram] Vector search skipped (embedding failed): {}", e);
            }
        }
    }

    // ── Procedural search ────────────────────────────────────────────
    let procedural = store.engram_search_procedural(query, scope, 10)?;

    // ── RRF Fusion (with hybrid text-boost weighting) ──────────────
    // Resolve the optimal text/vector balance for this specific query.
    let hybrid_text_weight = resolve_hybrid_weight(query, &config.hybrid);
    let vector_weight = 1.0 - hybrid_text_weight;

    // Merge BM25 and vector results for episodic memories using weighted RRF.
    let fused_episodic = rrf_fuse_episodic(
        &bm25_episodic,
        &vec_episodic,
        hybrid_text_weight as f32,
        vector_weight as f32,
    );

    // Convert fused episodic to RetrievedMemory
    for (mem, score) in &fused_episodic {
        let trust = TrustScore {
            relevance: *score as f32,
            accuracy: 0.5,
            freshness: temporal_freshness(&mem.created_at),
            utility: mem.importance,
        };

        // Choose compression level based on budget pressure
        let (content, level) = choose_compression(&mem.content, budget_tokens, all_results.len());
        let tok = Tokenizer::heuristic();
        let token_cost = tok.count_tokens(&content);

        all_results.push(RetrievedMemory {
            content,
            compression_level: level,
            memory_id: mem.id.clone(),
            memory_type: MemoryType::Episodic,
            trust_score: trust,
            token_cost,
            category: mem.category.clone(),
            created_at: mem.created_at.clone(),
            agent_id: mem.agent_id.clone(),
        });

        // Record access for spacing effect
        if let Err(e) = store.engram_record_access(&mem.id, RETRIEVAL_STRENGTH_BOOST) {
            log::debug!("[engram] Failed to record access for {}: {}", mem.id, e);
        }
    }

    // Convert semantic BM25 results
    for (mem, score) in &bm25_semantic {
        let trust = TrustScore {
            relevance: *score as f32,
            accuracy: mem.confidence,
            freshness: temporal_freshness(&mem.created_at),
            utility: if mem.is_user_explicit { 0.9 } else { 0.6 },
        };

        all_results.push(RetrievedMemory {
            content: mem.full_text.clone(),
            compression_level: CompressionLevel::Full,
            memory_id: mem.id.clone(),
            memory_type: MemoryType::Semantic,
            trust_score: trust,
            token_cost: Tokenizer::heuristic().count_tokens(&mem.full_text),
            category: mem.category.clone(),
            created_at: mem.created_at.clone(),
            agent_id: mem.scope.agent_id.clone().unwrap_or_default(),
        });
    }

    // Convert procedural results
    for mem in &procedural {
        let content = format!(
            "Procedure: {}\nSteps: {}",
            mem.trigger,
            mem.steps
                .iter()
                .enumerate()
                .map(|(i, s)| format!("{}. {}", i + 1, s.description))
                .collect::<Vec<_>>()
                .join("\n")
        );

        let trust = TrustScore {
            relevance: 0.5,
            accuracy: mem.success_rate,
            freshness: temporal_freshness(&mem.created_at),
            utility: 0.7,
        };

        all_results.push(RetrievedMemory {
            content: content.clone(),
            compression_level: CompressionLevel::Full,
            memory_id: mem.id.clone(),
            memory_type: MemoryType::Procedural,
            trust_score: trust,
            token_cost: Tokenizer::heuristic().count_tokens(&content),
            category: "procedure".into(),
            created_at: mem.created_at.clone(),
            agent_id: String::new(), // Procedural memories are agent-agnostic
        });
    }

    // ── Spreading Activation ─────────────────────────────────────────
    // Boost scores of memories connected to high-scoring results.
    let top_ids: Vec<String> = all_results
        .iter()
        .take(5)
        .map(|r| r.memory_id.clone())
        .collect();

    if !top_ids.is_empty() {
        if let Ok(activated) = store.engram_spreading_activation(&top_ids, 0.3) {
            for (neighbor_id, activation) in activated.iter().take(10) {
                // Check if this neighbor is already in results
                if let Some(existing) = all_results.iter_mut().find(|r| r.memory_id == *neighbor_id)
                {
                    // Boost existing result
                    existing.trust_score.relevance += activation * 0.2;
                } else if *activation > 0.4 {
                    // Fetch high-activation neighbors not already in results (§5 2-hop)
                    if let Ok(Some(mem)) = store.engram_get_episodic(neighbor_id) {
                        let trust = TrustScore {
                            relevance: activation * 0.15,
                            accuracy: 0.5,
                            freshness: temporal_freshness(&mem.created_at),
                            utility: 0.4,
                        };
                        let content = mem.content.full.clone();
                        all_results.push(RetrievedMemory {
                            token_cost: Tokenizer::heuristic().count_tokens(&content),
                            content,
                            compression_level: CompressionLevel::Full,
                            memory_id: mem.id.clone(),
                            memory_type: MemoryType::Episodic,
                            trust_score: trust,
                            category: mem.category.clone(),
                            created_at: mem.created_at.clone(),
                            agent_id: mem.agent_id.clone(),
                        });
                        if let Err(e) =
                            store.engram_record_access(&mem.id, RETRIEVAL_STRENGTH_BOOST * 0.5)
                        {
                            log::debug!("[engram] Failed to record access for {}: {}", mem.id, e);
                        }
                    }
                }
            }
        }
    }

    // ── Cross-type deduplication (§34.3) ───────────────────────────
    // Remove near-duplicate results that span episodic ↔ semantic types.
    cross_type_dedup(&mut all_results, DEDUP_JACCARD_THRESHOLD);
    let candidates_after_filter = all_results.len();

    // ── Sort by composite trust score ────────────────────────────────
    all_results.sort_by(|a, b| {
        let sa = a.trust_score.composite();
        let sb = b.trust_score.composite();
        sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
    });

    // ── Reranking (§35.1) ────────────────────────────────────────────
    let rerank_applied = if config.rerank_enabled {
        all_results = rerank_results(
            &all_results,
            query,
            raw_query_embedding.as_deref(),
            config.rerank_strategy,
            config.mmr_lambda,
        );
        Some(config.rerank_strategy)
    } else {
        None
    };

    // ── Budget-aware trimming ────────────────────────────────────────
    let trimmed = budget_trim(all_results, budget_tokens);

    // ── Retrieval quality metrics (§5.3 / §35) ──────────────────────
    let search_latency_ms = search_start.elapsed().as_millis() as u64;
    let result = build_recall_result(
        trimmed,
        candidates_after_filter,
        search_latency_ms,
        rerank_applied,
        hybrid_text_weight,
    );

    info!(
        "[engram] Search completed: {} memories packed, NDCG={:.2}, avg_relevancy={:.2}, {}ms",
        result.quality.memories_packed,
        result.quality.ndcg,
        result.quality.average_relevancy,
        result.quality.search_latency_ms,
    );

    // §5 Self-tuning: feed NDCG into the recall tuner so the similarity
    // threshold adapts over time. Low NDCG → loosen threshold (wider net);
    // high NDCG → tighten threshold (reduce noise).
    if result.quality.memories_packed > 0 {
        let adapted = super::recall_tuner::observe_and_tune(result.quality.ndcg);
        if (adapted - config.similarity_threshold as f64).abs() > 0.02 {
            info!(
                "[engram] Recall tuner: threshold adapted to {:.3} (EMA NDCG={:.3})",
                adapted,
                super::recall_tuner::current_ema_ndcg(),
            );
        }
    }

    // §8.6 Attach the raw query embedding for trajectory tracking
    let mut result = result;
    result.query_embedding = raw_query_embedding;

    Ok(result)
}

// ═════════════════════════════════════════════════════════════════════════════
// Relationship Management
// ═════════════════════════════════════════════════════════════════════════════

/// Create a relationship between two memories.
pub fn relate(
    store: &SessionStore,
    source_id: &str,
    target_id: &str,
    edge_type: EdgeType,
    weight: f32,
) -> EngineResult<()> {
    let edge = MemoryEdge {
        source_id: source_id.to_string(),
        target_id: target_id.to_string(),
        edge_type,
        weight,
        created_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
    };
    store.engram_add_edge(&edge)?;
    info!(
        "[engram] ✓ Edge: {} --[{:.2}]--> {}",
        source_id, weight, target_id
    );
    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// FadeMem Dual-Layer Decay (§54)
// ═════════════════════════════════════════════════════════════════════════════
//
// Two-layer decay model inspired by FadeMem:
//
//   fast_strength — activation layer (hours half-life).
//     Represents how "active" a memory is in the current context.
//     Decays rapidly (half-life = 4 hours). Boosted on every retrieval.
//     Used for working memory promotion and recency scoring.
//
//   slow_strength — consolidation layer (days/weeks half-life).
//     Represents how well-established a memory is through repetition.
//     Decays slowly (half-life = configurable, default 7 days).
//     Boosted when a memory is successfully consolidated (consolidation pipeline).
//     GC uses slow_strength — well-consolidated memories survive even if
//     temporarily inactive (no fast_strength).
//
//   importance — derived composite: max(fast, slow) * base_importance / 10.
//     This preserves backward compatibility with GC thresholds and search scoring.

/// Fast-layer half-life in hours.
const FAST_HALF_LIFE_HOURS: f64 = 4.0;

/// Slow-layer consolidation boost on each consolidation cycle.
const CONSOLIDATION_SLOW_BOOST: f64 = 0.15;

/// Fast-layer retrieval boost on each access.
const RETRIEVAL_FAST_BOOST: f64 = 0.3;

/// FadeMem β-exponent: SML (Short Memory Layer) — super-linear decay.
/// half-life ≈ 5.02 days. Transient, low-importance memories fade rapidly.
const BETA_SML: f64 = 1.2;

/// FadeMem β-exponent: LML (Long Memory Layer) — sub-linear decay.
/// half-life ≈ 11.25 days. Important, frequently-accessed memories persist.
const BETA_LML: f64 = 0.8;

/// Hysteresis: promote SML → LML when access frequency exceeds this threshold.
/// Access frequency = access_count / days_since_creation.
const PROMOTE_THRESHOLD: f64 = 0.7;

/// Hysteresis: demote LML → SML when access frequency drops below this.
/// Gap between promote/demote prevents oscillation.
const DEMOTE_THRESHOLD: f64 = 0.3;

/// Apply FadeMem dual-layer decay with β-exponent model to episodic memories.
///
/// Formula: strength(t) = S₀ · exp(-λ_base · t^β)
///   - β = 0.8 (LML) for well-accessed memories → sub-linear, slow decay
///   - β = 1.2 (SML) for low-access memories → super-linear, fast decay
///
/// Layer assignment uses hysteresis (promote/demote) to prevent oscillation.
/// Per-type decay modulation: procedural ×0.5, semantic ×0.7, episodic ×1.0.
///
/// Returns the number of memories updated.
pub fn apply_decay(store: &SessionStore, _half_life_days: f32) -> EngineResult<usize> {
    let lambda_base: f64 = 0.1; // base decay rate (from ENGRAM.md)
    let lambda_fast = (2.0_f64.ln()) / FAST_HALF_LIFE_HOURS;
    let now = chrono::Utc::now();

    let updates: Vec<(String, f64, f64, i32)> = {
        let conn = store.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, importance, last_accessed_at, created_at,
                    COALESCE(fast_strength, 1.0), COALESCE(slow_strength, 0.0),
                    access_count, category
             FROM episodic_memories
             WHERE consolidation_state != 'archived'",
        )?;
        let rows = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let base_importance: i32 = row.get(1)?;
            let last_access: Option<String> = row.get(2)?;
            let created: String = row.get(3)?;
            let fast: f64 = row.get(4)?;
            let slow: f64 = row.get(5)?;
            let access_count: i32 = row.get(6)?;
            let category: String = row.get::<_, String>(7).unwrap_or_default();

            let reference_time = last_access.as_deref().unwrap_or(&created);
            let days_elapsed = parse_days_since(reference_time, &now);
            let hours_elapsed = days_elapsed * 24.0;
            let days_since_creation = parse_days_since(&created, &now).max(0.01);

            // Fast-layer: standard exponential (activation, not knowledge)
            let new_fast = (fast * (-lambda_fast * hours_elapsed).exp()).max(0.0);

            // Slow-layer: β-exponent FadeMem model
            // 1. Determine memory layer (LML vs SML) via hysteresis
            let access_freq = access_count as f64 / days_since_creation;
            let beta = if access_freq >= PROMOTE_THRESHOLD {
                BETA_LML // sub-linear — decays slowly
            } else if access_freq <= DEMOTE_THRESHOLD {
                BETA_SML // super-linear — decays rapidly
            } else {
                // In hysteresis gap: use current slow_strength as proxy
                // High slow_strength → was LML, keep LML; low → SML
                if slow >= 0.5 {
                    BETA_LML
                } else {
                    BETA_SML
                }
            };

            // 2. Per-type decay modulation (§ ENGRAM.md: Adaptive Forgetting)
            let type_modifier = match category.as_str() {
                "procedure" | "skill" | "workflow" => 0.5, // skills persist longer
                "knowledge" | "preference" | "fact" => 0.7, // knowledge decays slower
                _ => 1.0,                                  // episodic at base rate
            };

            // 3. Frequently accessed memories persist (>5 accesses)
            let access_modifier = if access_count > 5 { 0.7 } else { 1.0 };

            // 4. Effective lambda with all modifiers
            let lambda_eff = lambda_base * type_modifier * access_modifier;

            // 5. β-exponent decay: S₀ · exp(-λ_eff · t^β)
            let t_days = days_elapsed.max(0.0);
            let new_slow = (slow * (-lambda_eff * t_days.powf(beta)).exp()).max(0.0);

            // Composite importance: the effective strength is the max of both layers
            // scaled by the original base importance (0-10).
            let effective = new_fast.max(new_slow);
            let new_importance = ((base_importance as f64 * effective).round() as i32).max(0);

            Ok((id, new_fast, new_slow, new_importance))
        })?;
        rows.filter_map(|r| r.ok())
            .filter(|(_, fast, slow, imp)| *fast < 1.0 || *slow < 1.0 || *imp < 10)
            .collect()
    };

    let count = updates.len();
    {
        let conn = store.conn.lock();
        for (id, fast, slow, importance) in &updates {
            conn.execute(
                "UPDATE episodic_memories
                 SET fast_strength = ?2, slow_strength = ?3, importance = ?4
                 WHERE id = ?1",
                rusqlite::params![id, fast, slow, importance],
            )?;
        }
    }

    if count > 0 {
        info!(
            "[engram] FadeMem dual-layer decay applied to {} episodic memories",
            count
        );
    }

    Ok(count)
}

/// Boost fast_strength on retrieval (spacing effect).
/// Called when a memory is accessed/retrieved during search.
pub fn boost_fast_strength(store: &SessionStore, memory_id: &str) -> EngineResult<()> {
    let conn = store.conn.lock();
    conn.execute(
        "UPDATE episodic_memories
         SET fast_strength = MIN(COALESCE(fast_strength, 0.0) + ?2, 1.0)
         WHERE id = ?1",
        rusqlite::params![memory_id, RETRIEVAL_FAST_BOOST],
    )?;
    Ok(())
}

/// Boost slow_strength during consolidation.
/// Called when a memory is successfully consolidated into a semantic triple.
pub fn boost_slow_strength(store: &SessionStore, memory_id: &str) -> EngineResult<()> {
    let conn = store.conn.lock();
    conn.execute(
        "UPDATE episodic_memories
         SET slow_strength = MIN(COALESCE(slow_strength, 0.0) + ?2, 1.0)
         WHERE id = ?1",
        rusqlite::params![memory_id, CONSOLIDATION_SLOW_BOOST],
    )?;
    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// Garbage Collection
// ═════════════════════════════════════════════════════════════════════════════

/// Garbage collect memories with strength below threshold.
/// Archived/consolidated memories are preserved. Only low-strength Fresh memories are GC'd.
/// FadeMem: uses slow_strength as additional protection — well-consolidated memories
/// (slow_strength >= 0.3) are shielded from GC even if importance is low.
/// Uses secure erasure (zero-before-delete) and re-pads the DB to the next
/// bucket boundary to prevent file-size side-channel leakage.
pub fn garbage_collect(
    store: &SessionStore,
    importance_threshold: i32,
    batch_size: usize,
    hnsw_index: Option<&super::hnsw::SharedHnswIndex>,
) -> EngineResult<usize> {
    let candidates = store.engram_list_gc_candidates(importance_threshold, batch_size)?;

    // FadeMem filter: protect well-consolidated memories even if importance dropped
    let filtered: Vec<String> = {
        let conn = store.conn.lock();
        candidates
            .into_iter()
            .filter(|id| {
                let slow: f64 = conn
                    .query_row(
                        "SELECT COALESCE(slow_strength, 0.0) FROM episodic_memories WHERE id = ?1",
                        rusqlite::params![id],
                        |row| row.get(0),
                    )
                    .unwrap_or(0.0);
                // Only GC if slow_strength is also below protection threshold
                slow < 0.3
            })
            .collect()
    };

    let count = filtered.len();

    for id in &filtered {
        // Secure erase: zero content fields then delete (anti-forensic)
        store.engram_secure_erase_episodic(id)?;
        store.engram_audit_log("secure_erase", id, "system", "gc", Some("strength_gc"))?;

        // Remove from HNSW index so deleted memories don't pollute search
        if let Some(idx) = hnsw_index {
            super::hnsw::remove_shared(idx, id);
        }
    }

    if count > 0 {
        // Re-pad to next bucket boundary so file size doesn't reveal
        // how many memories were just deleted (KDBX-equivalent mitigation).
        store.engram_repad()?;
        info!(
            "[engram] GC: securely erased {} decayed episodic memories",
            count
        );
    }

    Ok(count)
}

// ═════════════════════════════════════════════════════════════════════════════
// Quick Stats
// ═════════════════════════════════════════════════════════════════════════════

/// Get memory counts across all tiers.
pub fn memory_stats(store: &SessionStore) -> EngineResult<EngramStats> {
    let episodic = store.engram_count_episodic(None)?;
    let semantic = store.engram_count_semantic()?;
    let procedural = store.engram_count_procedural()?;
    let edges = store.engram_count_edges()?;

    Ok(EngramStats {
        episodic,
        semantic,
        procedural,
        edges,
    })
}

/// Summary statistics for the Engram system.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EngramStats {
    pub episodic: usize,
    pub semantic: usize,
    pub procedural: usize,
    pub edges: usize,
}

// ═════════════════════════════════════════════════════════════════════════════
// Private Helpers
// ═════════════════════════════════════════════════════════════════════════════

/// Word-level Jaccard similarity (same as existing memory::content_overlap).
fn content_overlap(a: &str, b: &str) -> f64 {
    let a_words: std::collections::HashSet<&str> = a.split_whitespace().collect();
    let b_words: std::collections::HashSet<&str> = b.split_whitespace().collect();
    if a_words.is_empty() && b_words.is_empty() {
        return 1.0;
    }
    let intersection = a_words.intersection(&b_words).count();
    let union = a_words.union(&b_words).count();
    if union == 0 {
        return 0.0;
    }
    intersection as f64 / union as f64
}

/// Reciprocal Rank Fusion for episodic memories.
/// Merges BM25 and vector results, deduplicating by ID.
fn rrf_fuse_episodic(
    bm25: &[(EpisodicMemory, f64)],
    vector: &[(EpisodicMemory, f64)],
    bm25_weight: f32,
    vector_weight: f32,
) -> Vec<(EpisodicMemory, f64)> {
    let mut scores: std::collections::HashMap<String, (EpisodicMemory, f64)> =
        std::collections::HashMap::new();

    // BM25 contributions
    for (rank, (mem, _score)) in bm25.iter().enumerate() {
        let rrf_score = bm25_weight as f64 / (RRF_K + rank as f64 + 1.0);
        scores
            .entry(mem.id.clone())
            .and_modify(|(_, s)| *s += rrf_score)
            .or_insert((mem.clone(), rrf_score));
    }

    // Vector contributions
    for (rank, (mem, _score)) in vector.iter().enumerate() {
        let rrf_score = vector_weight as f64 / (RRF_K + rank as f64 + 1.0);
        scores
            .entry(mem.id.clone())
            .and_modify(|(_, s)| *s += rrf_score)
            .or_insert((mem.clone(), rrf_score));
    }

    let mut results: Vec<(EpisodicMemory, f64)> = scores.into_values().collect();
    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    results
}

/// Compute temporal freshness from a timestamp string (0.0 = ancient, 1.0 = just now).
fn temporal_freshness(created_at: &str) -> f32 {
    let now = chrono::Utc::now();
    let days = parse_days_since(created_at, &now);
    // Exponential decay: freshness = e^(-days/30)
    ((-days / 30.0).exp() as f32).clamp(0.0, 1.0)
}

/// Parse days elapsed since a timestamp string.
fn parse_days_since(timestamp: &str, now: &chrono::DateTime<chrono::Utc>) -> f64 {
    if let Ok(parsed) = chrono::NaiveDateTime::parse_from_str(timestamp, "%Y-%m-%dT%H:%M:%SZ") {
        let dt = parsed.and_utc();
        let duration = *now - dt;
        duration.num_hours() as f64 / 24.0
    } else {
        30.0 // Fallback: treat as 30 days old
    }
}

/// Choose compression level based on budget pressure.
fn choose_compression(
    content: &TieredContent,
    _budget_tokens: usize,
    results_so_far: usize,
) -> (String, CompressionLevel) {
    // Heuristic: first 3 results get full, next 5 get summary, rest get key_fact
    let level = if results_so_far < 3 {
        CompressionLevel::Full
    } else if results_so_far < 8 {
        CompressionLevel::Summary
    } else {
        CompressionLevel::KeyFact
    };

    let text = content.at_level(level).to_string();
    (text, level)
}

/// Trim results to fit within a token budget.
fn budget_trim(results: Vec<RetrievedMemory>, budget_tokens: usize) -> Vec<RetrievedMemory> {
    let mut trimmed = Vec::new();
    let mut tokens_used = 0;

    for result in results {
        if tokens_used + result.token_cost > budget_tokens {
            // Try a more compressed version if available
            // For now, just stop
            break;
        }
        tokens_used += result.token_cost;
        trimmed.push(result);
    }

    trimmed
}

/// §8.6 Momentum blending: combine current query embedding with the weighted
/// average of recent momentum embeddings to produce a trajectory-aware vector.
///
/// `current_weight` controls how much of the current query is retained (0.0–1.0).
/// The remainder (1.0 – current_weight) comes from the exponentially-weighted
/// average of momentum vectors (most recent → highest weight).
fn blend_momentum(current: &[f32], momentum: &[Vec<f32>], current_weight: f32) -> Vec<f32> {
    if momentum.is_empty() || current.is_empty() {
        return current.to_vec();
    }

    let dim = current.len();
    let momentum_weight = 1.0 - current_weight;

    // Exponentially-weighted average: most recent vector gets highest weight.
    // Weights: [0.5^n, 0.5^(n-1), ..., 0.5^1] for n momentum vectors,
    // then normalized so they sum to 1.
    let n = momentum.len();
    let raw_weights: Vec<f32> = (0..n).map(|i| 0.5_f32.powi((n - i) as i32)).collect();
    let weight_sum: f32 = raw_weights.iter().sum();

    // Compute weighted average of momentum vectors
    let mut mom_avg = vec![0.0_f32; dim];
    for (vec, &w) in momentum.iter().zip(raw_weights.iter()) {
        let norm_w = w / weight_sum;
        for (j, &v) in vec.iter().enumerate().take(dim) {
            mom_avg[j] += v * norm_w;
        }
    }

    // Blend: blended = current_weight * current + momentum_weight * mom_avg
    let blended: Vec<f32> = current
        .iter()
        .zip(mom_avg.iter())
        .map(|(&c, &m)| current_weight * c + momentum_weight * m)
        .collect();

    // L2-normalize the blended vector for cosine similarity
    let norm: f32 = blended.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-8 {
        blended.iter().map(|x| x / norm).collect()
    } else {
        current.to_vec()
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Tests
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_overlap() {
        assert!((content_overlap("hello world foo", "hello world bar") - 0.5).abs() < 0.01);
        assert!((content_overlap("hello world", "hello world") - 1.0).abs() < 0.01);
        assert!((content_overlap("alpha", "beta") - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_temporal_freshness() {
        // Now should be ~1.0
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let f = temporal_freshness(&now);
        assert!(f > 0.9, "freshness of 'now' should be > 0.9, got {}", f);
    }

    #[test]
    fn test_rrf_fuse_empty() {
        let result = rrf_fuse_episodic(&[], &[], 0.4, 0.6);
        assert!(result.is_empty());
    }

    #[test]
    fn test_budget_trim() {
        let results = vec![
            RetrievedMemory {
                content: "short".into(),
                compression_level: CompressionLevel::Full,
                memory_id: "a".into(),
                memory_type: MemoryType::Episodic,
                trust_score: TrustScore::default(),
                token_cost: 100,
                category: "general".into(),
                created_at: String::new(),
                agent_id: String::new(),
            },
            RetrievedMemory {
                content: "also short".into(),
                compression_level: CompressionLevel::Full,
                memory_id: "b".into(),
                memory_type: MemoryType::Episodic,
                trust_score: TrustScore::default(),
                token_cost: 200,
                category: "general".into(),
                created_at: String::new(),
                agent_id: String::new(),
            },
        ];

        let trimmed = budget_trim(results, 150);
        assert_eq!(trimmed.len(), 1);
        assert_eq!(trimmed[0].memory_id, "a");
    }

    #[test]
    fn test_choose_compression_progression() {
        let content = TieredContent {
            full: "full content here".into(),
            summary: Some("summary".into()),
            key_fact: Some("fact".into()),
            tags: Some("tag1, tag2".into()),
        };

        let (_, level0) = choose_compression(&content, 10000, 0);
        assert_eq!(level0, CompressionLevel::Full);

        let (_, level5) = choose_compression(&content, 10000, 5);
        assert_eq!(level5, CompressionLevel::Summary);

        let (_, level10) = choose_compression(&content, 10000, 10);
        assert_eq!(level10, CompressionLevel::KeyFact);
    }

    #[test]
    fn test_blend_momentum_identity_when_empty() {
        let current = vec![1.0, 0.0, 0.0];
        let result = blend_momentum(&current, &[], 0.7);
        assert_eq!(result, current);
    }

    #[test]
    fn test_blend_momentum_shifts_direction() {
        let current = vec![1.0, 0.0, 0.0]; // pointing along x
        let momentum = vec![vec![0.0, 1.0, 0.0]]; // pointing along y
        let blended = blend_momentum(&current, &momentum, 0.7);
        // Blended should have both x and y components, L2-normalized
        assert!(blended[0] > 0.5, "should retain x component");
        assert!(blended[1] > 0.1, "should gain y momentum");
        // Check L2 normalization
        let norm: f32 = blended.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5, "should be unit-normalized");
    }

    // ── HNSW Integration Tests ──────────────────────────────────────────

    #[test]
    fn test_hnsw_is_empty_shared_guards_search() {
        // Verify that an empty HNSW index reports empty via O(1) check
        let idx = super::super::hnsw::new_shared();
        assert!(super::super::hnsw::is_empty_shared(&idx));

        // After insert, should not be empty
        super::super::hnsw::insert_shared(&idx, "a", vec![1.0, 0.0, 0.0]);
        assert!(!super::super::hnsw::is_empty_shared(&idx));
    }

    #[test]
    fn test_hnsw_insert_and_search_shared() {
        let idx = super::super::hnsw::new_shared();

        // Insert 3 vectors with known directions
        super::super::hnsw::insert_shared(&idx, "x", vec![1.0, 0.0, 0.0]);
        super::super::hnsw::insert_shared(&idx, "y", vec![0.0, 1.0, 0.0]);
        super::super::hnsw::insert_shared(&idx, "z", vec![0.0, 0.0, 1.0]);

        // Search for x-direction — should find "x" first
        let results = super::super::hnsw::search_shared(&idx, &[1.0, 0.0, 0.0], 3, 0.0);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].memory_id, "x");
        assert!((results[0].similarity - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_hnsw_scope_filtering_after_search() {
        // Verifies the adaptive fallback logic: HNSW returns global results,
        // then scope filtering prunes to only the requesting agent's memories.
        use crate::atoms::engram_types::MemoryScope;

        let idx = super::super::hnsw::new_shared();

        // Simulate 10 memories: 9 from agent-A, 1 from agent-B
        for i in 0..9 {
            super::super::hnsw::insert_shared(
                &idx,
                &format!("a-{}", i),
                vec![1.0 + i as f32 * 0.01, 0.0, 0.0],
            );
        }
        super::super::hnsw::insert_shared(&idx, "b-0", vec![1.0, 0.01, 0.0]);

        // Search returns all 10 — scope filtering must happen at the caller
        let results = super::super::hnsw::search_shared(&idx, &[1.0, 0.0, 0.0], 10, 0.0);
        assert_eq!(results.len(), 10);

        // Verify scope_matches works correctly for filtering
        let scope_a = MemoryScope::agent("agent-A");
        let scope_b = MemoryScope::agent("agent-B");

        // scope_matches is used by the HNSW block in search() to filter
        assert!(crate::engine::sessions::engram::scope_matches(
            &scope_a, &scope_a
        ));
        assert!(!crate::engine::sessions::engram::scope_matches(
            &scope_a, &scope_b
        ));
    }
}
