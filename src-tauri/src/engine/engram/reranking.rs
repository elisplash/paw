// ── Engram: Reranking Pipeline (§35.1) ──────────────────────────────────────
//
// Implements 4 reranking strategies applied after initial retrieval + filtering:
//
//   1. RRF — Reciprocal Rank Fusion (fast, no model dependency)
//   2. MMR — Maximal Marginal Relevance (diversity-focused)
//   3. RRFThenMMR — Combined: RRF first, then MMR for diversity (default)
//   4. CrossEncoder — LLM-based reranking (most accurate, requires Ollama)
//
// Reranking is step 5 in the recall pipeline (§8.4).
// Engram has 4 configurable strategies.

use crate::atoms::engram_types::{RerankStrategy, RetrievedMemory};
use log::warn;

/// Rerank a set of candidate memories after initial retrieval.
///
/// This is the main entry point. Dispatches to the appropriate strategy.
/// All strategies are O(n²) or better, where n is typically ≤50 candidates.
pub fn rerank_results(
    candidates: &[RetrievedMemory],
    _query: &str,
    query_embedding: Option<&[f32]>,
    strategy: RerankStrategy,
    mmr_lambda: f32,
) -> Vec<RetrievedMemory> {
    if candidates.is_empty() {
        return Vec::new();
    }

    match strategy {
        RerankStrategy::RRF => {
            // Already RRF-fused in graph.rs, but we re-sort by composite score
            // to ensure consistent ordering after any post-retrieval modifications.
            rrf_rerank(candidates)
        }
        RerankStrategy::MMR => mmr_rerank(candidates, query_embedding, mmr_lambda as f64),
        RerankStrategy::RRFThenMMR => {
            let rrf_ranked = rrf_rerank(candidates);
            mmr_rerank(&rrf_ranked, query_embedding, mmr_lambda as f64)
        }
        RerankStrategy::CrossEncoder => {
            // Cross-encoder reranking requires an Ollama model.
            // For now, fall back to RRF+MMR since we don't have a cross-encoder
            // endpoint integrated yet. This will be wired once Ollama exposes
            // a reranking API or we add a local cross-encoder model.
            warn!("[engram] CrossEncoder reranking not yet available — falling back to RRFThenMMR");
            let rrf_ranked = rrf_rerank(candidates);
            mmr_rerank(&rrf_ranked, query_embedding, mmr_lambda as f64)
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Strategy: RRF (Sort by Composite Trust Score)
// ═══════════════════════════════════════════════════════════════════════════

/// RRF reranking: sort by composite trust score (descending).
///
/// Since the initial retrieval already uses RRF fusion, this is mainly
/// a re-sort after any score modifications from spreading activation.
fn rrf_rerank(candidates: &[RetrievedMemory]) -> Vec<RetrievedMemory> {
    let mut sorted = candidates.to_vec();
    sorted.sort_by(|a, b| {
        let sa = a.trust_score.composite();
        let sb = b.trust_score.composite();
        sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
    });
    sorted
}

// ═══════════════════════════════════════════════════════════════════════════
// Strategy: MMR (Maximal Marginal Relevance)
// ═══════════════════════════════════════════════════════════════════════════

/// MMR reranking: balance relevance and diversity.
///
/// λ=1.0 means pure relevance (same as RRF sort).
/// λ=0.0 means max diversity (penalize near-duplicates heavily).
/// λ=0.7 is the recommended default.
///
/// If `query_embedding` is None, falls back to word-overlap diversity.
fn mmr_rerank(
    candidates: &[RetrievedMemory],
    _query_embedding: Option<&[f32]>,
    lambda: f64,
) -> Vec<RetrievedMemory> {
    if candidates.is_empty() {
        return Vec::new();
    }

    let n = candidates.len();
    let mut selected: Vec<usize> = Vec::with_capacity(n);
    let mut remaining: Vec<usize> = (0..n).collect();

    // Pre-compute pairwise word-overlap similarities (O(n²) but n ≤ 50)
    let similarities = compute_pairwise_similarities(candidates);

    // First selection: highest composite score
    let first_idx = remaining
        .iter()
        .max_by(|&&a, &&b| {
            let sa = candidates[a].trust_score.composite();
            let sb = candidates[b].trust_score.composite();
            sa.partial_cmp(&sb).unwrap_or(std::cmp::Ordering::Equal)
        })
        .copied()
        .unwrap();

    selected.push(first_idx);
    remaining.retain(|&i| i != first_idx);

    // Iterative selection: maximize λ * relevance - (1-λ) * max_sim_to_selected
    while !remaining.is_empty() {
        let best = remaining
            .iter()
            .max_by(|&&a, &&b| {
                let score_a = mmr_score(a, &selected, candidates, &similarities, lambda);
                let score_b = mmr_score(b, &selected, candidates, &similarities, lambda);
                score_a
                    .partial_cmp(&score_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .copied()
            .unwrap();

        selected.push(best);
        remaining.retain(|&i| i != best);
    }

    selected.iter().map(|&i| candidates[i].clone()).collect()
}

/// Compute the MMR score for a candidate.
///
/// MMR(d) = λ * relevance(d) - (1-λ) * max_similarity(d, selected)
fn mmr_score(
    candidate_idx: usize,
    selected: &[usize],
    candidates: &[RetrievedMemory],
    similarities: &[Vec<f64>],
    lambda: f64,
) -> f64 {
    let relevance = candidates[candidate_idx].trust_score.composite() as f64;

    // Max similarity to any already-selected item
    let max_sim = selected
        .iter()
        .map(|&s| similarities[candidate_idx][s])
        .fold(0.0_f64, f64::max);

    lambda * relevance - (1.0 - lambda) * max_sim
}

/// Compute pairwise word-overlap similarities between all candidates.
///
/// Returns a symmetric n×n matrix where `result[i][j]` is the Jaccard
/// similarity between candidate i and candidate j.
fn compute_pairwise_similarities(candidates: &[RetrievedMemory]) -> Vec<Vec<f64>> {
    let n = candidates.len();
    let word_sets: Vec<std::collections::HashSet<&str>> = candidates
        .iter()
        .map(|c| c.content.split_whitespace().collect())
        .collect();

    let mut sims = vec![vec![0.0_f64; n]; n];
    for i in 0..n {
        sims[i][i] = 1.0;
        for j in (i + 1)..n {
            let intersection = word_sets[i].intersection(&word_sets[j]).count();
            let union = word_sets[i].union(&word_sets[j]).count();
            let sim = if union == 0 {
                0.0
            } else {
                intersection as f64 / union as f64
            };
            sims[i][j] = sim;
            sims[j][i] = sim;
        }
    }
    sims
}

// ═══════════════════════════════════════════════════════════════════════════
// Cross-Type Deduplication (§34.3)
// ═══════════════════════════════════════════════════════════════════════════

/// Deduplicate search results that span episodic and semantic types.
///
/// If the same fact appears as both an episodic memory ("User said they prefer
/// TypeScript") and a semantic triple ("User prefers TypeScript"), only the
/// higher-scoring one should appear in results.
///
/// Uses word-level Jaccard similarity to detect cross-type duplicates.
pub fn cross_type_dedup(results: &mut Vec<RetrievedMemory>, threshold: f64) {
    if results.len() <= 1 {
        return;
    }

    let mut to_remove: Vec<usize> = Vec::new();

    for i in 0..results.len() {
        if to_remove.contains(&i) {
            continue;
        }
        for j in (i + 1)..results.len() {
            if to_remove.contains(&j) {
                continue;
            }
            // Only dedup cross-type (episodic vs semantic)
            if results[i].memory_type == results[j].memory_type {
                continue;
            }

            let sim = word_jaccard(&results[i].content, &results[j].content);
            if sim > threshold {
                // Remove the one with the lower composite score
                let score_i = results[i].trust_score.composite();
                let score_j = results[j].trust_score.composite();
                if score_i >= score_j {
                    to_remove.push(j);
                } else {
                    to_remove.push(i);
                }
            }
        }
    }

    // Remove in reverse order to preserve indices
    to_remove.sort_unstable();
    to_remove.dedup();
    for &idx in to_remove.iter().rev() {
        results.remove(idx);
    }
}

/// Word-level Jaccard similarity.
fn word_jaccard(a: &str, b: &str) -> f64 {
    let a_words: std::collections::HashSet<&str> = a.split_whitespace().collect();
    let b_words: std::collections::HashSet<&str> = b.split_whitespace().collect();
    if a_words.is_empty() && b_words.is_empty() {
        return 1.0;
    }
    let intersection = a_words.intersection(&b_words).count();
    let union = a_words.union(&b_words).count();
    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atoms::engram_types::{CompressionLevel, MemoryType, TrustScore};

    fn make_memory(id: &str, content: &str, relevance: f32) -> RetrievedMemory {
        RetrievedMemory {
            content: content.into(),
            compression_level: CompressionLevel::Full,
            memory_id: id.into(),
            memory_type: MemoryType::Episodic,
            trust_score: TrustScore {
                relevance,
                accuracy: 0.7,
                freshness: 0.8,
                utility: 0.6,
            },
            token_cost: content.len() / 4,
            category: "general".into(),
            created_at: String::new(),
        }
    }

    #[test]
    fn test_rrf_rerank_sorts_by_score() {
        let mems = vec![
            make_memory("low", "low score memory", 0.3),
            make_memory("high", "high score memory", 0.9),
            make_memory("mid", "mid score memory", 0.6),
        ];
        let ranked = rrf_rerank(&mems);
        assert_eq!(ranked[0].memory_id, "high");
        assert_eq!(ranked[2].memory_id, "low");
    }

    #[test]
    fn test_mmr_rerank_preserves_all() {
        let mems = vec![
            make_memory("a", "the quick brown fox", 0.9),
            make_memory("b", "the quick brown fox jumps", 0.8), // very similar to a
            make_memory("c", "completely different topic about rust", 0.7),
        ];
        let ranked = mmr_rerank(&mems, None, 0.7);
        assert_eq!(ranked.len(), 3, "MMR should preserve all candidates");
    }

    #[test]
    fn test_mmr_lambda_1_is_pure_relevance() {
        let mems = vec![
            make_memory("a", "hello world", 0.5),
            make_memory("b", "hello world foo", 0.9),
            make_memory("c", "goodbye world", 0.7),
        ];
        let ranked = mmr_rerank(&mems, None, 1.0);
        // λ=1.0 means pure relevance sort
        assert_eq!(ranked[0].memory_id, "b"); // highest relevance
    }

    #[test]
    fn test_mmr_promotes_diversity() {
        // Two very similar items (a+b) and one diverse item (c)
        let mems = vec![
            make_memory("a", "the quick brown fox jumps over the lazy dog", 0.9),
            make_memory("b", "the quick brown fox jumps over the lazy cat", 0.85),
            make_memory("c", "rust programming language is systems level", 0.6),
        ];
        // Low lambda = high diversity
        let ranked = mmr_rerank(&mems, None, 0.3);
        // After selecting "a" first (highest score), "c" should come before "b"
        // because "c" is more diverse from "a" than "b" is
        assert_eq!(ranked[0].memory_id, "a");
        assert_eq!(
            ranked[1].memory_id, "c",
            "Diverse item should be promoted with low lambda"
        );
    }

    #[test]
    fn test_rerank_empty() {
        let result = rerank_results(&[], "test", None, RerankStrategy::RRF, 0.7);
        assert!(result.is_empty());
    }

    #[test]
    fn test_cross_type_dedup_removes_duplicate() {
        let mut results = vec![
            {
                let mut m = make_memory("ep1", "user prefers TypeScript for web development", 0.8);
                m.memory_type = MemoryType::Episodic;
                m
            },
            {
                let mut m = make_memory("sem1", "user prefers TypeScript for web development", 0.9);
                m.memory_type = MemoryType::Semantic;
                m
            },
        ];
        cross_type_dedup(&mut results, 0.6);
        assert_eq!(
            results.len(),
            1,
            "One of the cross-type duplicates should be removed"
        );
        assert_eq!(
            results[0].memory_id, "sem1",
            "Higher-scored semantic should survive"
        );
    }

    #[test]
    fn test_cross_type_dedup_keeps_different() {
        let mut results = vec![
            {
                let mut m = make_memory("ep1", "user prefers TypeScript", 0.8);
                m.memory_type = MemoryType::Episodic;
                m
            },
            {
                let mut m = make_memory("sem1", "rust is a systems programming language", 0.7);
                m.memory_type = MemoryType::Semantic;
                m
            },
        ];
        cross_type_dedup(&mut results, 0.6);
        assert_eq!(results.len(), 2, "Different content should not be deduped");
    }

    #[test]
    fn test_cross_type_dedup_same_type_not_deduped() {
        let mut results = vec![
            make_memory("ep1", "user prefers TypeScript for development", 0.8),
            make_memory("ep2", "user prefers TypeScript for development", 0.7),
        ];
        // Both are Episodic — cross-type dedup should NOT remove them
        cross_type_dedup(&mut results, 0.6);
        assert_eq!(
            results.len(),
            2,
            "Same-type duplicates are not cross-type deduped"
        );
    }

    #[test]
    fn test_pairwise_similarities() {
        let mems = vec![
            make_memory("a", "hello world foo bar", 0.9),
            make_memory("b", "hello world baz qux", 0.8),
            make_memory("c", "completely different text here", 0.7),
        ];
        let sims = compute_pairwise_similarities(&mems);
        // a and b share "hello" and "world" out of 6 unique words → 2/6 ≈ 0.33
        assert!(sims[0][1] > 0.2, "a and b should have some similarity");
        // a and c share no words
        assert!(sims[0][2] < 0.1, "a and c should have low similarity");
        // Diagonal should be 1.0
        assert!((sims[0][0] - 1.0).abs() < 0.001);
    }
}
