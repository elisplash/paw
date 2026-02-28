// ── Engram: Retrieval Quality Metrics (§5.3 / §35) ─────────────────────────
//
// Computes NDCG (Normalized Discounted Cumulative Gain) and average relevancy
// on every search operation. These metrics serve dual purposes:
//
//   1. Frontend display — users can see retrieval health in the debug panel
//   2. Self-tuning feedback loop — if quality degrades, the system flags it
//
// No existing local-first memory system (Mem0, MemGPT, Zep) provides retrieval
// Engram uses them to
// self-tune.

use crate::atoms::engram_types::{
    RecallResult, RerankStrategy, RetrievalQualityMetrics, RetrievedMemory,
};

// ═══════════════════════════════════════════════════════════════════════════
// NDCG Computation
// ═══════════════════════════════════════════════════════════════════════════

/// Compute NDCG for a ranked list of retrieved memories.
///
/// Uses each memory's composite trust score as the "ideal" relevance grade.
/// NDCG = DCG / IDCG (ratio of actual ranking vs. ideal ranking).
///
/// Range: 0.0–1.0 where 1.0 means the results are in perfect descending
/// relevance order and 0.0 means empty results.
pub fn compute_ndcg(memories: &[RetrievedMemory]) -> f64 {
    if memories.is_empty() {
        return 0.0;
    }

    // DCG: sum of relevance / log2(rank + 2)
    // We use rank + 2 because rank is 0-indexed and log2(1) = 0 causes division by zero
    let dcg: f64 = memories
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let relevance = m.trust_score.composite() as f64;
            relevance / (i as f64 + 2.0).log2()
        })
        .sum();

    // IDCG: same formula but with scores sorted in ideal (descending) order
    let mut ideal_scores: Vec<f64> = memories
        .iter()
        .map(|m| m.trust_score.composite() as f64)
        .collect();
    ideal_scores.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));

    let idcg: f64 = ideal_scores
        .iter()
        .enumerate()
        .map(|(i, &rel)| rel / (i as f64 + 2.0).log2())
        .sum();

    if idcg == 0.0 {
        0.0
    } else {
        (dcg / idcg).clamp(0.0, 1.0)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Average Relevancy
// ═══════════════════════════════════════════════════════════════════════════

/// Compute average composite trust score across all retrieved memories.
/// Range: 0.0–1.0. Below 0.3 indicates poor recall quality.
pub fn compute_average_relevancy(memories: &[RetrievedMemory]) -> f64 {
    if memories.is_empty() {
        return 0.0;
    }
    let sum: f64 = memories
        .iter()
        .map(|m| m.trust_score.composite() as f64)
        .sum();
    sum / memories.len() as f64
}

// ═══════════════════════════════════════════════════════════════════════════
// Metrics Builder
// ═══════════════════════════════════════════════════════════════════════════

/// Build a complete `RetrievalQualityMetrics` from the final set of returned memories.
///
/// Call this after reranking and budget-trimming — it measures the final output quality.
pub fn build_quality_metrics(
    memories: &[RetrievedMemory],
    candidates_after_filter: usize,
    search_latency_ms: u64,
    rerank_applied: Option<RerankStrategy>,
    hybrid_text_weight: f64,
) -> RetrievalQualityMetrics {
    let tokens_consumed: usize = memories.iter().map(|m| m.token_cost).sum();

    RetrievalQualityMetrics {
        average_relevancy: compute_average_relevancy(memories),
        ndcg: compute_ndcg(memories),
        candidates_after_filter,
        memories_packed: memories.len(),
        tokens_consumed,
        search_latency_ms,
        rerank_applied,
        hybrid_text_weight,
    }
}

/// Construct a `RecallResult` from memories and their quality metrics.
pub fn build_recall_result(
    memories: Vec<RetrievedMemory>,
    candidates_after_filter: usize,
    search_latency_ms: u64,
    rerank_applied: Option<RerankStrategy>,
    hybrid_text_weight: f64,
) -> RecallResult {
    let quality = build_quality_metrics(
        &memories,
        candidates_after_filter,
        search_latency_ms,
        rerank_applied,
        hybrid_text_weight,
    );
    RecallResult { memories, quality }
}

// ═══════════════════════════════════════════════════════════════════════════
// Health Assessment
// ═══════════════════════════════════════════════════════════════════════════

/// Quality thresholds for self-tuning.
const LOW_RELEVANCY_THRESHOLD: f64 = 0.3;
const LOW_NDCG_THRESHOLD: f64 = 0.4;

/// Assess whether the retrieval quality is degraded.
/// Returns a list of human-readable warnings (empty = healthy).
pub fn assess_quality(metrics: &RetrievalQualityMetrics) -> Vec<String> {
    let mut warnings = Vec::new();

    if metrics.memories_packed == 0 {
        warnings.push("No memories matched the query — long-term store may be empty".into());
        return warnings;
    }

    if metrics.average_relevancy < LOW_RELEVANCY_THRESHOLD {
        warnings.push(format!(
            "Low average relevancy ({:.2}) — recalled memories may not be relevant to the query",
            metrics.average_relevancy
        ));
    }

    if metrics.ndcg < LOW_NDCG_THRESHOLD && metrics.memories_packed > 1 {
        warnings.push(format!(
            "Low NDCG ({:.2}) — result ranking may be suboptimal, consider enabling reranking",
            metrics.ndcg
        ));
    }

    if metrics.search_latency_ms > 1000 {
        warnings.push(format!(
            "High search latency ({}ms) — consider reducing search scope or enabling HNSW index",
            metrics.search_latency_ms
        ));
    }

    if metrics.candidates_after_filter > 0 && metrics.memories_packed == 0 {
        warnings.push("Candidates found but none packed — token budget may be too small".into());
    }

    warnings
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atoms::engram_types::{CompressionLevel, MemoryType, TrustScore};

    fn make_memory(relevance: f32, accuracy: f32, token_cost: usize) -> RetrievedMemory {
        RetrievedMemory {
            content: "test".into(),
            compression_level: CompressionLevel::Full,
            memory_id: uuid::Uuid::new_v4().to_string(),
            memory_type: MemoryType::Episodic,
            trust_score: TrustScore {
                relevance,
                accuracy,
                freshness: 0.8,
                utility: 0.7,
            },
            token_cost,
            category: "general".into(),
            created_at: String::new(),
        }
    }

    #[test]
    fn test_ndcg_empty() {
        assert_eq!(compute_ndcg(&[]), 0.0);
    }

    #[test]
    fn test_ndcg_single_item() {
        let mems = vec![make_memory(0.9, 0.8, 50)];
        let ndcg = compute_ndcg(&mems);
        assert!(
            (ndcg - 1.0).abs() < 0.001,
            "Single item should have NDCG=1.0, got {}",
            ndcg
        );
    }

    #[test]
    fn test_ndcg_perfect_order() {
        // Already in descending relevance order → NDCG = 1.0
        let mems = vec![
            make_memory(0.9, 0.9, 50),
            make_memory(0.7, 0.7, 50),
            make_memory(0.5, 0.5, 50),
        ];
        let ndcg = compute_ndcg(&mems);
        assert!(
            (ndcg - 1.0).abs() < 0.01,
            "Perfect order should have NDCG≈1.0, got {}",
            ndcg
        );
    }

    #[test]
    fn test_ndcg_reversed_order() {
        // Worst-case: ascending order when it should be descending
        let mems = vec![
            make_memory(0.3, 0.3, 50),
            make_memory(0.6, 0.6, 50),
            make_memory(0.9, 0.9, 50),
        ];
        let ndcg = compute_ndcg(&mems);
        assert!(
            ndcg < 1.0,
            "Reversed order should have NDCG < 1.0, got {}",
            ndcg
        );
        // Still should be > 0 because some value is still captured
        assert!(ndcg > 0.0, "NDCG should be > 0 for non-empty list");
    }

    #[test]
    fn test_average_relevancy() {
        let mems = vec![
            make_memory(0.8, 0.8, 50),
            make_memory(0.6, 0.6, 50),
            make_memory(0.4, 0.4, 50),
        ];
        let avg = compute_average_relevancy(&mems);
        // Average of composite scores — should be roughly middle of range
        assert!(avg > 0.3 && avg < 0.9, "avg_relevancy = {}", avg);
    }

    #[test]
    fn test_average_relevancy_empty() {
        assert_eq!(compute_average_relevancy(&[]), 0.0);
    }

    #[test]
    fn test_build_quality_metrics() {
        let mems = vec![make_memory(0.9, 0.8, 100), make_memory(0.7, 0.6, 80)];
        let metrics = build_quality_metrics(&mems, 10, 42, None, 0.3);

        assert_eq!(metrics.memories_packed, 2);
        assert_eq!(metrics.candidates_after_filter, 10);
        assert_eq!(metrics.tokens_consumed, 180);
        assert_eq!(metrics.search_latency_ms, 42);
        assert!(metrics.ndcg > 0.0);
        assert!(metrics.average_relevancy > 0.0);
        assert_eq!(metrics.rerank_applied, None);
        assert!((metrics.hybrid_text_weight - 0.3).abs() < 0.001);
    }

    #[test]
    fn test_assess_quality_healthy() {
        let metrics = build_quality_metrics(
            &[make_memory(0.8, 0.8, 100)],
            5,
            50,
            Some(RerankStrategy::RRF),
            0.3,
        );
        let warnings = assess_quality(&metrics);
        assert!(
            warnings.is_empty(),
            "Healthy metrics should produce no warnings: {:?}",
            warnings
        );
    }

    #[test]
    fn test_assess_quality_low_relevancy() {
        let metrics = RetrievalQualityMetrics {
            average_relevancy: 0.1,
            ndcg: 0.8,
            candidates_after_filter: 10,
            memories_packed: 5,
            tokens_consumed: 500,
            search_latency_ms: 50,
            rerank_applied: None,
            hybrid_text_weight: 0.3,
        };
        let warnings = assess_quality(&metrics);
        assert!(
            warnings.iter().any(|w| w.contains("Low average relevancy")),
            "Should warn about low relevancy"
        );
    }

    #[test]
    fn test_assess_quality_empty_results() {
        let metrics = RetrievalQualityMetrics::default();
        let warnings = assess_quality(&metrics);
        assert!(
            warnings.iter().any(|w| w.contains("No memories matched")),
            "Should warn about empty results"
        );
    }
}
