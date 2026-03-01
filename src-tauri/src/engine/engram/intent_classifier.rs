// ── Engram: Intent-Aware Retrieval (§40) ─────────────────────────────────────
//
// Classify the *type* of question being asked and adjust retrieval
// signal weights accordingly. Example:
//   "How do I set up SSH keys?" → Procedural → heavy BM25, light graph
//   "Why did the deploy fail?"  → Causal     → heavy graph, medium temporal
//   "What happened Tuesday?"    → Episodic   → heavy temporal, light BM25
//
// This module:
//   - Classifies queries into `QueryIntent` via keyword/pattern heuristics
//   - Returns `IntentClassification` with per-intent confidence scores
//   - Exposes `signal_weights` for the main search pipeline to use

use crate::atoms::engram_types::IntentClassification;

// ═══════════════════════════════════════════════════════════════════════════
// Query Intent Classification
// ═══════════════════════════════════════════════════════════════════════════

/// Classify user query into intent distribution.
/// Uses keyword heuristics — no ML model required, fast & deterministic.
pub fn classify_intent(query: &str) -> IntentClassification {
    let q = query.to_lowercase();
    let _tokens: Vec<&str> = q.split_whitespace().collect();

    let mut factual = 0.0_f32;
    let mut procedural = 0.0_f32;
    let mut causal = 0.0_f32;
    let mut episodic = 0.0_f32;
    let mut exploratory = 0.0_f32;
    let mut reflective = 0.0_f32;

    // ── Factual signals ──────────────────────────────────────────────────
    // "what is X", "define X", "who is", "how many", "what's the"
    if starts_with_any(&q, &["what is ", "what's ", "define ", "who is ", "who's "]) {
        factual += 0.6;
    }
    if contains_any(&q, &["how many", "how much", "what version", "what port", "what url"]) {
        factual += 0.5;
    }
    if contains_any(&q, &["name of", "type of", "value of", "default"]) {
        factual += 0.3;
    }

    // ── Procedural signals ───────────────────────────────────────────────
    // "how do I", "how to", "steps to", "guide for", "set up", "configure"
    if starts_with_any(&q, &["how do i ", "how to ", "how can i "]) {
        procedural += 0.6;
    }
    if contains_any(&q, &[
        "steps to", "guide", "tutorial", "set up", "setup", "configure",
        "install", "deploy", "create a", "build a", "make a",
    ]) {
        procedural += 0.4;
    }
    if contains_any(&q, &["command", "run ", "execute", "script"]) {
        procedural += 0.2;
    }

    // ── Causal signals ───────────────────────────────────────────────────
    // "why did", "why is", "what caused", "reason for", "because"
    if starts_with_any(&q, &["why did ", "why is ", "why does ", "why was ", "why are "]) {
        causal += 0.7;
    }
    if contains_any(&q, &[
        "what caused", "reason for", "root cause", "leads to",
        "results in", "because of", "due to", "consequence",
    ]) {
        causal += 0.5;
    }
    if contains_any(&q, &["error", "fail", "broke", "broken", "crash", "bug"]) {
        causal += 0.2;
    }

    // ── Episodic signals ─────────────────────────────────────────────────
    // "what happened", "when did", "last time", temporal references
    if starts_with_any(&q, &["what happened", "when did ", "when was ", "when is "]) {
        episodic += 0.7;
    }
    if contains_any(&q, &[
        "yesterday", "last week", "last month", "last time", "today",
        "this morning", "this afternoon", "last night", "earlier",
        "tuesday", "wednesday", "monday", "thursday", "friday",
        "saturday", "sunday", "ago", "recently", "before",
    ]) {
        episodic += 0.5;
    }
    if contains_any(&q, &["remember when", "that time", "conversation about", "discussed"]) {
        episodic += 0.4;
    }

    // ── Exploratory signals ──────────────────────────────────────────────
    // "what are the options", "alternatives", "compare", "explore"
    if contains_any(&q, &[
        "options for", "alternatives", "compare", "difference between",
        "pros and cons", "tradeoff", "possibilities", "explore",
        "suggest", "recommend", "ideas for", "brainstorm",
    ]) {
        exploratory += 0.6;
    }
    if contains_any(&q, &["what about", "what if", "could we", "should we"]) {
        exploratory += 0.3;
    }

    // ── Reflective signals ───────────────────────────────────────────────
    // "how well", "review", "summarize", "what have we", "progress"
    if contains_any(&q, &[
        "summarize", "summary", "review", "recap", "overview",
        "what have we", "progress", "how well", "how far",
        "lessons learned", "retrospective", "reflect",
    ]) {
        reflective += 0.6;
    }
    if contains_any(&q, &["status", "update on", "where are we"]) {
        reflective += 0.3;
    }

    // ── Question-word boost ──────────────────────────────────────────────
    // If nothing matched strongly, fall back to question-word heuristics
    let total = factual + procedural + causal + episodic + exploratory + reflective;
    if total < 0.1 {
        // Default to a gentle mix — mostly factual/exploratory
        factual = 0.3;
        exploratory = 0.3;
        episodic = 0.1;
        procedural = 0.1;
        causal = 0.1;
        reflective = 0.1;
    }

    // Normalize to sum = 1.0
    let total = factual + procedural + causal + episodic + exploratory + reflective;
    if total > 0.0 {
        factual /= total;
        procedural /= total;
        causal /= total;
        episodic /= total;
        exploratory /= total;
        reflective /= total;
    }

    IntentClassification {
        factual,
        procedural,
        causal,
        episodic,
        exploratory,
        reflective,
    }
}

/// Convenience: classify and immediately return signal weights.
/// Returns (bm25, vector, graph, temporal, emotional) weights.
pub fn intent_weights(query: &str) -> (f32, f32, f32, f32, f32) {
    classify_intent(query).signal_weights()
}

// ═══════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════

fn starts_with_any(s: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|p| s.starts_with(p))
}

fn contains_any(s: &str, terms: &[&str]) -> bool {
    terms.iter().any(|t| s.contains(t))
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atoms::engram_types::QueryIntent;

    #[test]
    fn test_factual() {
        let c = classify_intent("What is the default port for Redis?");
        assert!(c.factual > 0.4, "Expected factual dominant, got {c:?}");
        assert_eq!(c.dominant(), QueryIntent::Factual);
    }

    #[test]
    fn test_procedural() {
        let c = classify_intent("How do I set up SSH keys on Ubuntu?");
        assert!(c.procedural > 0.4, "Expected procedural dominant, got {c:?}");
        assert_eq!(c.dominant(), QueryIntent::Procedural);
    }

    #[test]
    fn test_causal() {
        let c = classify_intent("Why did the deployment fail last night?");
        assert!(c.causal > 0.3, "Expected causal dominant, got {c:?}");
    }

    #[test]
    fn test_episodic() {
        let c = classify_intent("What happened yesterday in the meeting?");
        assert!(c.episodic > 0.4, "Expected episodic dominant, got {c:?}");
    }

    #[test]
    fn test_exploratory() {
        let c = classify_intent("What are the alternatives to Redis for caching?");
        assert!(c.exploratory > 0.3, "Expected exploratory signal, got {c:?}");
    }

    #[test]
    fn test_reflective() {
        let c = classify_intent("Can you summarize what we've done this sprint?");
        assert!(c.reflective > 0.3, "Expected reflective signal, got {c:?}");
    }

    #[test]
    fn test_fallback_generic() {
        let c = classify_intent("asdfghjkl");
        // Should get default distribution
        let total = c.factual + c.procedural + c.causal + c.episodic + c.exploratory + c.reflective;
        assert!((total - 1.0).abs() < 0.01, "Should normalize to 1.0");
    }

    #[test]
    fn test_signal_weights_procedural() {
        let (bm25, _vector, _graph, _temporal, _emotional) =
            intent_weights("How do I configure nginx?");
        assert!(bm25 >= 0.3, "Procedural should have strong BM25 weight, got {bm25}");
    }

    #[test]
    fn test_signal_weights_episodic() {
        let (_bm25, _vector, _graph, temporal, _emotional) =
            intent_weights("What happened last Tuesday?");
        assert!(temporal > 0.2, "Episodic should have strong temporal weight");
    }
}
