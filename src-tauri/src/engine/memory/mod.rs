// Paw Agent Engine — Memory System
//
// Provides long-term semantic memory using SQLite + embedding vectors.
// Uses Ollama (local) for embeddings by default — works out of the box.
// Also supports OpenAI-compatible embedding APIs.
//
// Module layout:
//   ollama.rs    — Ollama lifecycle (auto-start, model discovery/pull)
//   embedding.rs — EmbeddingClient (Ollama + OpenAI-compatible API calls)
//   mod.rs       — store, search (hybrid BM25+vector), MMR, fact extraction

pub mod embedding;
pub mod ollama;

// Re-export public API at the module level
pub use embedding::EmbeddingClient;
pub use ollama::{OllamaReadyStatus, ensure_ollama_ready, is_ollama_init_done};

use crate::engine::sessions::{SessionStore, f32_vec_to_bytes};
use crate::engine::types::*;
use crate::atoms::error::EngineResult;
use log::{info, warn, error};

// ── Store ──────────────────────────────────────────────────────────────

/// Store a memory with embedding.
/// If embedding_client is provided, computes embedding automatically.
/// Logs clearly when embeddings succeed or fail.
pub async fn store_memory(
    store: &SessionStore,
    content: &str,
    category: &str,
    importance: u8,
    embedding_client: Option<&EmbeddingClient>,
    agent_id: Option<&str>,
) -> EngineResult<String> {
    let id = uuid::Uuid::new_v4().to_string();

    let embedding_bytes = if let Some(client) = embedding_client {
        match client.embed(content).await {
            Ok(vec) => {
                info!("[memory] ✓ Embedded {} dims for memory {}", vec.len(), &id[..8]);
                Some(f32_vec_to_bytes(&vec))
            }
            Err(e) => {
                error!("[memory] ✗ Embedding failed for memory {} — storing without vector: {}", &id[..8], e);
                None
            }
        }
    } else {
        warn!("[memory] No embedding client — storing memory {} without vector (semantic search won't find this)", &id[..8]);
        None
    };

    store.store_memory(&id, content, category, importance, embedding_bytes.as_deref(), agent_id)?;
    info!("[memory] Stored memory {} cat={} imp={} agent={:?} has_embedding={}",
        &id[..8], category, importance, agent_id, embedding_bytes.is_some());
    Ok(id)
}

// ── Search (hybrid BM25 + vector + temporal decay + MMR) ───────────────

/// Search memories using hybrid strategy (BM25 + vector + temporal decay + MMR).
///
/// Strategy:
/// 1. BM25 full-text search via FTS5 (fast, exact-match aware)
/// 2. Vector semantic search via embeddings (meaning-aware)
/// 3. Merge results with weighted scoring (0.4 BM25 + 0.6 vector)
/// 4. Apply temporal decay (newer memories score higher)
/// 5. Apply MMR re-ranking (maximize diversity in top results)
/// 6. Optionally filter by agent_id
pub async fn search_memories(
    store: &SessionStore,
    query: &str,
    limit: usize,
    threshold: f64,
    embedding_client: Option<&EmbeddingClient>,
    agent_id: Option<&str>,
) -> EngineResult<Vec<Memory>> {
    let query_preview = &query[..query.len().min(80)];
    let fetch_limit = limit * 3; // Fetch extra for MMR re-ranking

    // ── Step 1: BM25 full-text search ──────────────────────────────
    let bm25_results = match store.search_memories_bm25(query, fetch_limit, agent_id) {
        Ok(r) => {
            info!("[memory] BM25 search: {} results for '{}'", r.len(), query_preview);
            r
        }
        Err(e) => {
            warn!("[memory] BM25 search failed: {} — continuing with vector only", e);
            Vec::new()
        }
    };

    // ── Step 2: Vector semantic search ─────────────────────────────
    let mut vector_results = Vec::new();
    let mut query_embedding: Option<Vec<f32>> = None;
    if let Some(client) = embedding_client {
        match client.embed(query).await {
            Ok(query_vec) => {
                info!("[memory] Query embedded ({} dims), searching...", query_vec.len());
                match store.search_memories_by_embedding(&query_vec, fetch_limit, threshold, agent_id) {
                    Ok(results) => {
                        info!("[memory] Vector search: {} results (top score: {:.3})",
                            results.len(),
                            results.first().and_then(|r| r.score).unwrap_or(0.0));
                        vector_results = results;
                    }
                    Err(e) => warn!("[memory] Vector search failed: {}", e),
                }
                query_embedding = Some(query_vec);
            }
            Err(e) => {
                warn!("[memory] Embedding query failed: {}", e);
            }
        }
    }

    // ── Step 3: Merge with weighted scoring ────────────────────────
    let mut merged = merge_search_results(&bm25_results, &vector_results, 0.4, 0.6);

    if merged.is_empty() {
        // Final fallback: keyword LIKE search
        info!("[memory] No BM25/vector results, falling back to keyword search");
        let results = store.search_memories_keyword(query, limit)?;
        info!("[memory] Keyword fallback: {} results for '{}'", results.len(), query_preview);
        return Ok(results);
    }

    // ── Step 4: Apply temporal decay ───────────────────────────────
    apply_temporal_decay(&mut merged);

    // ── Step 5: MMR re-ranking for diversity ───────────────────────
    let merged_count = merged.len();
    let final_results = if query_embedding.is_some() && merged.len() > limit {
        mmr_rerank(&merged, limit, 0.7) // lambda=0.7 (70% relevance, 30% diversity)
    } else {
        merged.sort_by(|a, b| {
            b.score.unwrap_or(0.0).partial_cmp(&a.score.unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        merged.truncate(limit);
        merged
    };

    info!("[memory] Hybrid search: returning {} results for '{}' (BM25={}, vector={}, merged={})",
        final_results.len(), query_preview, bm25_results.len(), vector_results.len(), merged_count);

    Ok(final_results)
}

// ── Search internals ───────────────────────────────────────────────────

/// Merge BM25 and vector search results with weighted scoring.
/// Normalizes scores from each source to [0,1] range before combining.
fn merge_search_results(
    bm25: &[Memory],
    vector: &[Memory],
    bm25_weight: f64,
    vector_weight: f64,
) -> Vec<Memory> {
    use std::collections::HashMap;

    let mut score_map: HashMap<String, (Option<f64>, Option<f64>, Memory)> = HashMap::new();

    // Normalize BM25 scores to [0,1]
    let bm25_max = bm25.iter().filter_map(|m| m.score).fold(0.0f64, f64::max);
    let bm25_min = bm25.iter().filter_map(|m| m.score).fold(f64::MAX, f64::min);
    let bm25_range = if (bm25_max - bm25_min).abs() < 1e-12 { 1.0 } else { bm25_max - bm25_min };

    for mem in bm25 {
        let normalized = mem.score.map(|s| (s - bm25_min) / bm25_range);
        score_map.insert(mem.id.clone(), (normalized, None, mem.clone()));
    }

    // Vector scores are already cosine similarity [0,1]
    for mem in vector {
        if let Some(entry) = score_map.get_mut(&mem.id) {
            entry.1 = mem.score;
        } else {
            score_map.insert(mem.id.clone(), (None, mem.score, mem.clone()));
        }
    }

    // Combine scores
    score_map.into_values().map(|(bm25_score, vec_score, mut mem)| {
        let b = bm25_score.unwrap_or(0.0) * bm25_weight;
        let v = vec_score.unwrap_or(0.0) * vector_weight;
        mem.score = Some(b + v);
        mem
    }).collect()
}

/// Apply temporal decay: boost newer memories, penalize old ones.
/// Uses exponential decay with a half-life of 30 days.
fn apply_temporal_decay(memories: &mut [Memory]) {
    let now = chrono::Utc::now();
    let half_life_days: f64 = 30.0;
    let decay_constant = (2.0f64).ln() / half_life_days;

    for mem in memories.iter_mut() {
        if let Ok(created) = chrono::NaiveDateTime::parse_from_str(&mem.created_at, "%Y-%m-%d %H:%M:%S") {
            let created_utc = created.and_utc();
            let age_days = (now - created_utc).num_hours() as f64 / 24.0;
            let decay_factor = (-decay_constant * age_days).exp();
            if let Some(ref mut score) = mem.score {
                *score *= decay_factor;
            }
        }
    }
}

/// Maximal Marginal Relevance re-ranking.
/// Selects diverse results by penalizing redundancy.
/// lambda: 1.0 = pure relevance, 0.0 = pure diversity. 0.7 is a good default.
fn mmr_rerank(candidates: &[Memory], k: usize, lambda: f64) -> Vec<Memory> {
    if candidates.is_empty() || k == 0 {
        return Vec::new();
    }

    let mut selected: Vec<Memory> = Vec::with_capacity(k);
    let mut remaining: Vec<&Memory> = candidates.iter().collect();

    // Pick the highest-scored item first
    remaining.sort_by(|a, b| {
        b.score.unwrap_or(0.0).partial_cmp(&a.score.unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    if let Some(first) = remaining.first() {
        selected.push((*first).clone());
        remaining.remove(0);
    }

    // Greedily select remaining items using MMR
    while selected.len() < k && !remaining.is_empty() {
        let mut best_idx = 0;
        let mut best_mmr = f64::NEG_INFINITY;

        for (i, candidate) in remaining.iter().enumerate() {
            let relevance = candidate.score.unwrap_or(0.0);
            let max_similarity = selected.iter()
                .map(|s| content_similarity(&candidate.content, &s.content))
                .fold(0.0f64, f64::max);

            let mmr_score = lambda * relevance - (1.0 - lambda) * max_similarity;

            if mmr_score > best_mmr {
                best_mmr = mmr_score;
                best_idx = i;
            }
        }

        selected.push(remaining[best_idx].clone());
        remaining.remove(best_idx);
    }

    selected
}

/// Simple content similarity (Jaccard on word sets) for MMR diversity.
fn content_similarity(a: &str, b: &str) -> f64 {
    let a_words: std::collections::HashSet<&str> = a.split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| w.len() > 2)
        .collect();
    let b_words: std::collections::HashSet<&str> = b.split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| w.len() > 2)
        .collect();
    if a_words.is_empty() && b_words.is_empty() {
        return 1.0;
    }
    let intersection = a_words.intersection(&b_words).count() as f64;
    let union = a_words.union(&b_words).count() as f64;
    if union < 1.0 { 0.0 } else { intersection / union }
}

// ── Backfill ───────────────────────────────────────────────────────────

/// Backfill embeddings for memories that were stored without vectors.
pub async fn backfill_embeddings(
    store: &SessionStore,
    client: &EmbeddingClient,
) -> EngineResult<(usize, usize)> {
    let memories = store.list_memories_without_embeddings(500)?;
    if memories.is_empty() {
        info!("[memory] Backfill: all memories already have embeddings");
        return Ok((0, 0));
    }

    info!("[memory] Backfill: embedding {} memories...", memories.len());
    let mut success = 0usize;
    let mut fail = 0usize;

    for mem in &memories {
        match client.embed(&mem.content).await {
            Ok(vec) => {
                let bytes = f32_vec_to_bytes(&vec);
                if let Err(e) = store.update_memory_embedding(&mem.id, &bytes) {
                    warn!("[memory] Backfill: failed to update {} — {}", &mem.id[..8], e);
                    fail += 1;
                } else {
                    success += 1;
                }
            }
            Err(e) => {
                warn!("[memory] Backfill: embed failed for {} — {}", &mem.id[..8], e);
                fail += 1;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    info!("[memory] Backfill complete: {} succeeded, {} failed", success, fail);
    Ok((success, fail))
}

// ── Fact Extraction ────────────────────────────────────────────────────

/// Auto-capture: extract memorable facts from an assistant response.
/// Uses a simple heuristic approach — no LLM call needed.
/// Returns content strings suitable for memory storage.
pub fn extract_memorable_facts(user_message: &str, _assistant_response: &str) -> Vec<(String, String)> {
    let mut facts: Vec<(String, String)> = Vec::new();
    let user_lower = user_message.to_lowercase();

    // User preference patterns
    let preference_patterns = [
        "i like ", "i love ", "i prefer ", "i use ", "i work with ",
        "my favorite ", "my name is ", "i'm ", "i am ", "i live ",
        "my job ", "i work at ", "i work as ",
    ];
    for pattern in &preference_patterns {
        if user_lower.contains(pattern) {
            facts.push((user_message.to_string(), "preference".into()));
            break;
        }
    }

    // Factual statements about the user's environment
    let fact_patterns = [
        "my project ", "my repo ", "my app ", "the codebase ",
        "we use ", "our stack ", "our team ", "the database ",
    ];
    for pattern in &fact_patterns {
        if user_lower.contains(pattern) {
            facts.push((user_message.to_string(), "context".into()));
            break;
        }
    }

    // Instructions: "always...", "never...", "remember that..."
    let instruction_patterns = [
        "always ", "never ", "remember that ", "remember to ",
        "don't forget ", "make sure to ", "keep in mind ",
    ];
    for pattern in &instruction_patterns {
        if user_lower.contains(pattern) {
            facts.push((user_message.to_string(), "instruction".into()));
            break;
        }
    }

    facts
}
