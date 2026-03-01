// ── Engram: Hybrid Search with Text-Boost Weighting (§35.2) ─────────────────
//
// Controls the balance between vector similarity (semantic) and FTS5 keyword
// matching (lexical) in the search pipeline.
//
// Key innovation: auto-detection of query type. Factual queries ("what port?")
// get boosted text weight; conceptual queries ("how does auth work?") get
// boosted vector weight.
//
// Integration: called from graph.rs search() to determine the optimal text/vector
// balance per-query.

use crate::atoms::engram_types::HybridSearchConfig;

// ═══════════════════════════════════════════════════════════════════════════
// Auto-Detect Query Type
// ═══════════════════════════════════════════════════════════════════════════

/// Auto-detect query type and return the optimal text weight for hybrid search.
///
/// Factual queries (contain specific names, numbers, paths) → higher text weight.
/// Conceptual queries ("how", "why", "explain") → higher vector weight.
///
/// When `auto_detect` is false, returns the static `text_weight` from config.
pub fn resolve_hybrid_weight(query: &str, config: &HybridSearchConfig) -> f64 {
    if !config.auto_detect {
        return config.text_weight;
    }

    let query_lower = query.to_lowercase();
    let words: Vec<&str> = query.split_whitespace().collect();
    let word_count = words.len();

    // ── Factual signals: boost text weight ──────────────────────────
    let factual_signals: usize = [
        // File paths: contains `/`
        query.contains('/'),
        // Numbers or ports
        query.chars().any(|c| c.is_ascii_digit()),
        // Identifiers: contains `_` or `.` (file.rs, my_var)
        query.contains('_') || (query.contains('.') && !query.ends_with('?')),
        // Exact phrase (quoted)
        query.contains('"') || query.contains('\''),
        // Short, specific (≤3 words, likely a lookup)
        word_count <= 3 && word_count > 0,
        // Contains code-like tokens (camelCase or ALL_CAPS)
        words.iter().any(|w| {
            w.len() > 2
                && (w.chars().any(|c| c.is_uppercase()) && w.chars().any(|c| c.is_lowercase())
                    || *w == w.to_uppercase())
        }),
    ]
    .iter()
    .filter(|&&b| b)
    .count();

    // ── Conceptual signals: boost vector weight ────────────────────
    let conceptual_signals: usize = [
        query_lower.starts_with("how"),
        query_lower.starts_with("why"),
        query_lower.starts_with("explain"),
        query_lower.starts_with("what is"),
        query_lower.starts_with("describe"),
        query_lower.starts_with("tell me about"),
        // Long, descriptive queries
        word_count > 8,
        // Questions with broad scope
        query_lower.contains("overview") || query_lower.contains("summary"),
    ]
    .iter()
    .filter(|&&b| b)
    .count();

    // ── Compute adjustment ─────────────────────────────────────────
    let base = config.text_weight;
    // Each factual signal pushes text_weight up by 0.08
    // Each conceptual signal pushes text_weight down by 0.06
    let adjustment = (factual_signals as f64 * 0.08) - (conceptual_signals as f64 * 0.06);

    (base + adjustment).clamp(config.auto_min, config.auto_max)
}

/// Compute the corresponding vector weight from text weight.
/// Ensures they always sum to 1.0.
#[inline]
pub fn vector_weight_from_text(text_weight: f64) -> f64 {
    1.0 - text_weight
}

// ═══════════════════════════════════════════════════════════════════════════
// Weighted RRF Fusion
// ═══════════════════════════════════════════════════════════════════════════

/// Fuse BM25 and vector search results using weighted RRF.
///
/// Unlike standard RRF which uses equal weights, this version applies the
/// hybrid text/vector balance to the RRF contributions.
///
/// Returns a list of `(memory_id, fused_score)` sorted descending by score.
pub fn weighted_rrf_fuse(
    bm25_ids_ranked: &[String],
    vector_ids_ranked: &[String],
    text_weight: f64,
    rrf_k: f64,
) -> Vec<(String, f64)> {
    let vector_weight = vector_weight_from_text(text_weight);
    let mut scores: std::collections::HashMap<String, f64> = std::collections::HashMap::new();

    // BM25 contributions (weighted by text_weight)
    for (rank, id) in bm25_ids_ranked.iter().enumerate() {
        *scores.entry(id.clone()).or_default() += text_weight / (rrf_k + rank as f64 + 1.0);
    }

    // Vector contributions (weighted by vector_weight)
    for (rank, id) in vector_ids_ranked.iter().enumerate() {
        *scores.entry(id.clone()).or_default() += vector_weight / (rrf_k + rank as f64 + 1.0);
    }

    let mut fused: Vec<(String, f64)> = scores.into_iter().collect();
    fused.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    fused
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> HybridSearchConfig {
        HybridSearchConfig::default()
    }

    #[test]
    fn test_static_weight_when_auto_detect_off() {
        let config = HybridSearchConfig {
            text_weight: 0.5,
            auto_detect: false,
            auto_min: 0.1,
            auto_max: 0.9,
        };
        let weight = resolve_hybrid_weight("anything goes here", &config);
        assert!((weight - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_factual_query_boosts_text() {
        let config = default_config();
        // Contains path, numbers, identifiers — very factual
        let weight = resolve_hybrid_weight("src/main.rs line 42", &config);
        assert!(
            weight > config.text_weight,
            "Factual query should boost text weight: {} > {}",
            weight,
            config.text_weight
        );
    }

    #[test]
    fn test_conceptual_query_reduces_text() {
        let config = default_config();
        // Conceptual, long, starts with "how"
        let weight = resolve_hybrid_weight(
            "how does the authentication system work in this project",
            &config,
        );
        assert!(
            weight < config.text_weight,
            "Conceptual query should reduce text weight: {} < {}",
            weight,
            config.text_weight
        );
    }

    #[test]
    fn test_weight_clamped_to_bounds() {
        let config = HybridSearchConfig {
            text_weight: 0.9,
            auto_detect: true,
            auto_min: 0.1,
            auto_max: 0.7,
        };
        // Even with boosted text, should not exceed auto_max
        let weight = resolve_hybrid_weight("src/main.rs:42", &config);
        assert!(weight <= config.auto_max + 0.001);
        assert!(weight >= config.auto_min - 0.001);
    }

    #[test]
    fn test_short_specific_query_is_factual() {
        let config = default_config();
        let weight = resolve_hybrid_weight("HNSW index", &config);
        assert!(
            weight >= config.text_weight,
            "Short specific query should have similar or higher text weight"
        );
    }

    #[test]
    fn test_explain_query_is_conceptual() {
        let config = default_config();
        let weight = resolve_hybrid_weight("explain the memory consolidation pipeline", &config);
        assert!(
            weight < config.text_weight,
            "Explain query should reduce text weight"
        );
    }

    #[test]
    fn test_vector_weight_complement() {
        assert!((vector_weight_from_text(0.3) - 0.7).abs() < 0.001);
        assert!((vector_weight_from_text(0.0) - 1.0).abs() < 0.001);
        assert!((vector_weight_from_text(1.0) - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_weighted_rrf_fuse_basic() {
        let bm25 = vec!["a".into(), "b".into(), "c".into()];
        let vector = vec!["b".into(), "a".into(), "d".into()];

        let fused = weighted_rrf_fuse(&bm25, &vector, 0.3, 60.0);

        // All 4 unique IDs should appear
        assert_eq!(fused.len(), 4);
        // "a" and "b" appear in both lists → should score highest
        let top_ids: Vec<&str> = fused.iter().take(2).map(|(id, _)| id.as_str()).collect();
        assert!(top_ids.contains(&"a") || top_ids.contains(&"b"));
    }

    #[test]
    fn test_weighted_rrf_vector_heavy() {
        // With text_weight=0.1, vector results should dominate
        let bm25 = vec!["text_only".into()];
        let vector = vec!["vec_only".into()];

        let fused = weighted_rrf_fuse(&bm25, &vector, 0.1, 60.0);
        assert_eq!(fused.len(), 2);
        // Vector-only result should score higher because vector weight = 0.9
        assert_eq!(fused[0].0, "vec_only");
    }

    #[test]
    fn test_weighted_rrf_text_heavy() {
        // With text_weight=0.9, BM25 results should dominate
        let bm25 = vec!["text_only".into()];
        let vector = vec!["vec_only".into()];

        let fused = weighted_rrf_fuse(&bm25, &vector, 0.9, 60.0);
        assert_eq!(fused.len(), 2);
        assert_eq!(fused[0].0, "text_only");
    }

    #[test]
    fn test_weighted_rrf_empty_inputs() {
        assert!(weighted_rrf_fuse(&[], &[], 0.5, 60.0).is_empty());
    }
}
