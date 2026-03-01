// ── Engram: Emotional Memory Dimension (§37) ────────────────────────────────
//
// Biologically-inspired affective scoring pipeline.
// Emotionally charged memories encode stronger, resist decay, and recall
// more readily — mirroring the flashbulb effect in human cognition.
//
// Pipeline: content → 6 heuristic signals → AffectiveScore (valence/intensity/arousal)
// No LLM required — pure heuristics, sub-millisecond latency.
//
// Integration points:
//   - Storage: AffectiveScore modulates initial encoding strength
//   - Decay: High-arousal memories get longer half-life
//   - Retrieval: Affect-congruent recall boosts same-valence memories by 15%
//   - GC: Arousal ≥ 0.5 protects from garbage collection
//   - Working Memory: Priority bonus up to +30%
//   - Consolidation: High-affect memories consolidated first

use crate::atoms::engram_types::{AffectiveScore, EmotionalContext};

// ═════════════════════════════════════════════════════════════════════════════
// Emotional Marker Lexicon
// ═════════════════════════════════════════════════════════════════════════════

/// Positive emotional markers → positive valence.
const POSITIVE_MARKERS: &[&str] = &[
    "thank",
    "thanks",
    "awesome",
    "great",
    "perfect",
    "love",
    "amazing",
    "excellent",
    "wonderful",
    "fantastic",
    "brilliant",
    "beautiful",
    "happy",
    "glad",
    "appreciate",
    "helpful",
    "nice",
    "good job",
    "well done",
    "impressive",
    "superb",
    "outstanding",
    "delighted",
    "pleased",
    "excited",
    "celebrate",
    "success",
    "win",
    "breakthrough",
];

/// Negative emotional markers → negative valence.
const NEGATIVE_MARKERS: &[&str] = &[
    "frustrated",
    "annoying",
    "broken",
    "terrible",
    "hate",
    "awful",
    "horrible",
    "worst",
    "angry",
    "disappointing",
    "failed",
    "bug",
    "error",
    "crash",
    "stuck",
    "confused",
    "wrong",
    "impossible",
    "disaster",
    "nightmare",
    "pain",
    "suffer",
    "struggling",
    "helpless",
    "furious",
    "rage",
    "disgusting",
    "pathetic",
    "useless",
    "waste",
];

/// High-arousal markers → elevated activation regardless of valence.
const AROUSAL_MARKERS: &[&str] = &[
    "urgent",
    "critical",
    "asap",
    "immediately",
    "emergency",
    "deadline",
    "breaking",
    "important",
    "must",
    "need",
    "now",
    "hurry",
    "panic",
    "alert",
    "warning",
    "danger",
    "attention",
    "priority",
    "crucial",
    "vital",
    "essential",
    "blocking",
    "showstopper",
    "production",
];

/// Surprise markers → von Restorff effect (stronger encoding).
const SURPRISE_MARKERS: &[&str] = &[
    "unexpected",
    "surprisingly",
    "wow",
    "whoa",
    "didn't expect",
    "never thought",
    "turns out",
    "actually",
    "wait",
    "hold on",
    "plot twist",
    "incredible",
    "unbelievable",
    "shocking",
    "mind",
    "bizarre",
    "weird",
    "strange",
    "oddly",
    "interestingly",
];

// ═════════════════════════════════════════════════════════════════════════════
// AffectiveScorer — The Heuristic Pipeline
// ═════════════════════════════════════════════════════════════════════════════

/// Compute an affective score from text content using 6 heuristic signals.
///
/// Signals:
///   1. Emotional marker keywords (positive/negative lexicon)
///   2. Emphasis detection (caps, exclamation marks, repetition)
///   3. Task outcome sentiment (success/failure patterns)
///   4. User correction patterns ("no", "wrong", "actually")
///   5. Novelty markers (surprise lexicon)
///   6. Urgency signals (deadline/priority language)
///
/// Returns an `AffectiveScore` with valence (-1..1), intensity (0..1), arousal (0..1).
/// Total computation: O(n) single pass over the text, <1ms for typical content.
pub fn score_affect(content: &str) -> AffectiveScore {
    let lower = content.to_lowercase();
    let words: Vec<&str> = lower.split_whitespace().collect();
    let word_count = words.len().max(1) as f32;

    // ── Signal 1: Emotional marker keywords ──────────────────────
    let positive_hits = count_marker_hits(&lower, POSITIVE_MARKERS);
    let negative_hits = count_marker_hits(&lower, NEGATIVE_MARKERS);

    let marker_valence = if positive_hits + negative_hits > 0 {
        (positive_hits as f32 - negative_hits as f32)
            / (positive_hits as f32 + negative_hits as f32)
    } else {
        0.0
    };
    let marker_intensity = ((positive_hits + negative_hits) as f32 / word_count).min(1.0);

    // ── Signal 2: Emphasis detection ─────────────────────────────
    let exclamation_count = content.chars().filter(|c| *c == '!').count();
    let _question_count = content.chars().filter(|c| *c == '?').count();
    let caps_words = words
        .iter()
        .filter(|w| w.len() > 2 && w.chars().all(|c| c.is_uppercase()))
        .count();
    let emphasis_score = ((exclamation_count as f32 * 0.15) + (caps_words as f32 * 0.1)).min(1.0);

    // ── Signal 3: Task outcome sentiment ─────────────────────────
    let success_patterns = [
        "succeeded",
        "completed",
        "passed",
        "deployed",
        "fixed",
        "resolved",
        "merged",
    ];
    let failure_patterns = [
        "failed",
        "crashed",
        "broken",
        "rejected",
        "reverted",
        "timed out",
        "errored",
    ];
    let success_hits = count_marker_hits(&lower, &success_patterns);
    let failure_hits = count_marker_hits(&lower, &failure_patterns);
    let outcome_valence = if success_hits + failure_hits > 0 {
        (success_hits as f32 - failure_hits as f32) / (success_hits as f32 + failure_hits as f32)
    } else {
        0.0
    };

    // ── Signal 4: User correction patterns ───────────────────────
    let correction_patterns = [
        "no,",
        "no.",
        "wrong",
        "actually,",
        "not what i",
        "that's incorrect",
        "i meant",
    ];
    let correction_hits = count_marker_hits(&lower, &correction_patterns);
    let correction_valence = -(correction_hits as f32 * 0.3).min(0.6);

    // ── Signal 5: Novelty / surprise ─────────────────────────────
    let surprise_hits = count_marker_hits(&lower, SURPRISE_MARKERS);
    let surprise_score = (surprise_hits as f32 * 0.2).min(1.0);

    // ── Signal 6: Urgency / arousal ──────────────────────────────
    let urgency_hits = count_marker_hits(&lower, AROUSAL_MARKERS);
    let urgency_arousal = (urgency_hits as f32 * 0.15).min(1.0);

    // ── Combine signals ──────────────────────────────────────────
    // Valence: weighted average of marker, outcome, and correction signals
    let valence =
        (marker_valence * 0.5 + outcome_valence * 0.3 + correction_valence * 0.2).clamp(-1.0, 1.0);

    // Intensity: max of marker density, emphasis, and outcome strength
    let intensity = marker_intensity
        .max(emphasis_score)
        .max((success_hits + failure_hits) as f32 * 0.15)
        .clamp(0.0, 1.0);

    // Arousal: urgency + emphasis + surprise (independent of valence direction)
    let arousal = (urgency_arousal + emphasis_score * 0.5 + surprise_score * 0.3).clamp(0.0, 1.0);

    AffectiveScore {
        valence,
        intensity,
        arousal,
    }
}

/// Convert an AffectiveScore into an EmotionalContext for storage.
/// Maps the 3-signal score to the 4-dimensional PAD+Surprise model.
pub fn affect_to_emotional_context(score: &AffectiveScore, content: &str) -> EmotionalContext {
    let lower = content.to_lowercase();
    let surprise = (count_marker_hits(&lower, SURPRISE_MARKERS) as f32 * 0.25).min(1.0);

    // Dominance: high for imperative/commanding content, low for confused/uncertain
    let command_patterns = [
        "do this",
        "make sure",
        "you must",
        "change",
        "fix this",
        "update",
    ];
    let uncertain_patterns = ["i'm not sure", "maybe", "i think", "perhaps", "could you"];
    let cmd_hits = count_marker_hits(&lower, &command_patterns);
    let unc_hits = count_marker_hits(&lower, &uncertain_patterns);
    let dominance = ((cmd_hits as f32 * 0.2) - (unc_hits as f32 * 0.2)).clamp(-1.0, 1.0);

    EmotionalContext {
        valence: score.valence,
        arousal: score.arousal,
        dominance,
        surprise,
    }
}

/// Modulate the initial encoding strength of a memory based on its affect.
///
/// High-arousal memories get up to 1.5× initial strength.
/// This mirrors the biological "flashbulb" effect where emotional events
/// form stronger memory traces.
pub fn modulated_encoding_strength(base_importance: f32, affect: &AffectiveScore) -> f32 {
    let bonus = affect.encoding_bonus() as f32;
    (base_importance * bonus).clamp(0.0, 10.0)
}

/// Compute affect-congruent retrieval boost.
///
/// When current conversational affect matches a memory's emotional tone,
/// that memory gets a 15% RRF score boost. This mirrors mood-congruent
/// recall in human cognition.
///
/// Returns a multiplier: 1.0 (no boost) to 1.15 (max boost).
pub fn affect_congruent_boost(
    memory_affect: &EmotionalContext,
    current_affect: &EmotionalContext,
) -> f64 {
    let sim = memory_affect.similarity(current_affect);
    if sim > 0.3 {
        // Same emotional tone → boost by up to 15%
        1.0 + (sim as f64 - 0.3) * (0.15 / 0.7)
    } else {
        1.0
    }
}

/// Compute the Ebbinghaus half-life modulated by emotional arousal.
///
/// Base half-life (in days) is extended for emotionally significant memories.
/// Arousal of 1.0 doubles the half-life (memory decays 2× slower).
pub fn modulated_half_life(base_half_life_days: f64, affect: &AffectiveScore) -> f64 {
    base_half_life_days * affect.decay_resistance()
}

// ═════════════════════════════════════════════════════════════════════════════
// Helpers
// ═════════════════════════════════════════════════════════════════════════════

/// Count how many markers appear in the text (case-insensitive substring match).
fn count_marker_hits(text: &str, markers: &[&str]) -> usize {
    markers.iter().filter(|m| text.contains(**m)).count()
}

// ═════════════════════════════════════════════════════════════════════════════
// Tests
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positive_content_has_positive_valence() {
        let score = score_affect("Thank you so much! This is amazing and perfect!");
        assert!(score.valence > 0.0, "valence={}", score.valence);
        assert!(score.intensity > 0.0, "intensity={}", score.intensity);
    }

    #[test]
    fn negative_content_has_negative_valence() {
        let score = score_affect("This is terrible and frustrating. The whole thing is broken.");
        assert!(score.valence < 0.0, "valence={}", score.valence);
        assert!(score.intensity > 0.0, "intensity={}", score.intensity);
    }

    #[test]
    fn urgent_content_has_high_arousal() {
        let score =
            score_affect("URGENT: production is down! Fix this immediately, critical issue!");
        assert!(score.arousal > 0.3, "arousal={}", score.arousal);
    }

    #[test]
    fn neutral_content_is_low_intensity() {
        let score = score_affect("The function takes two parameters and returns a boolean.");
        assert!(score.intensity < 0.2, "intensity={}", score.intensity);
        assert!(score.arousal < 0.2, "arousal={}", score.arousal);
    }

    #[test]
    fn affect_congruent_boost_same_tone() {
        let mem = EmotionalContext {
            valence: 0.8,
            arousal: 0.6,
            dominance: 0.3,
            surprise: 0.1,
        };
        let cur = EmotionalContext {
            valence: 0.7,
            arousal: 0.5,
            dominance: 0.2,
            surprise: 0.0,
        };
        let boost = affect_congruent_boost(&mem, &cur);
        assert!(boost > 1.0, "boost={}", boost);
        assert!(boost <= 1.15, "boost={}", boost);
    }

    #[test]
    fn affect_congruent_boost_opposite_tone() {
        let mem = EmotionalContext {
            valence: 0.8,
            arousal: 0.6,
            dominance: 0.3,
            surprise: 0.1,
        };
        let cur = EmotionalContext {
            valence: -0.8,
            arousal: 0.6,
            dominance: -0.3,
            surprise: 0.1,
        };
        let boost = affect_congruent_boost(&mem, &cur);
        assert!(
            (boost - 1.0).abs() < 0.05,
            "boost should be near 1.0, got {}",
            boost
        );
    }

    #[test]
    fn modulated_half_life_increases_with_arousal() {
        let base = 30.0; // 30-day half-life
        let low = AffectiveScore {
            valence: 0.0,
            intensity: 0.1,
            arousal: 0.1,
        };
        let high = AffectiveScore {
            valence: 0.0,
            intensity: 0.8,
            arousal: 0.9,
        };
        assert!(modulated_half_life(base, &high) > modulated_half_life(base, &low));
    }

    #[test]
    fn encoding_bonus_ranges() {
        let calm = AffectiveScore {
            valence: 0.0,
            intensity: 0.0,
            arousal: 0.0,
        };
        let excited = AffectiveScore {
            valence: 0.5,
            intensity: 0.9,
            arousal: 1.0,
        };
        assert!((calm.encoding_bonus() - 1.0).abs() < f64::EPSILON);
        assert!(excited.encoding_bonus() >= 1.4);
        assert!(excited.encoding_bonus() <= 1.5);
    }
}
