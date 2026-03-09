// ── Engram: Self-Tuning Recall Threshold (§5 ENGRAM.md) ─────────────────────
//
// Adaptive similarity threshold based on rolling NDCG quality metrics.
//
// When retrieval quality is consistently low (NDCG < 0.4), the threshold is
// lowered to cast a wider net. When quality is consistently high (NDCG > 0.75),
// the threshold is raised to reduce noise. This creates a closed feedback loop:
//
//   search → NDCG measurement → threshold adjustment → next search benefits
//
// The tuner uses exponential moving average (EMA) of recent NDCG scores to
// smooth out per-query variance. Threshold adjustments are bounded within
// [MIN_THRESHOLD, MAX_THRESHOLD] to prevent runaway drift.
//
// No existing local-first memory system (Mem0, MemGPT, Zep) has this.

use parking_lot::Mutex;
use std::sync::LazyLock;

// ═════════════════════════════════════════════════════════════════════════════
// Configuration
// ═════════════════════════════════════════════════════════════════════════════

/// Minimum allowed similarity threshold (very permissive).
const MIN_THRESHOLD: f64 = 0.10;

/// Maximum allowed similarity threshold (very selective).
const MAX_THRESHOLD: f64 = 0.55;

/// Default starting threshold.
const DEFAULT_THRESHOLD: f64 = 0.30;

/// EMA smoothing factor (α). Higher = more responsive to recent scores.
const EMA_ALPHA: f64 = 0.15;

/// NDCG target above which we tighten the threshold.
const NDCG_HIGH: f64 = 0.75;

/// NDCG target below which we loosen the threshold.
const NDCG_LOW: f64 = 0.40;

/// Step size for threshold adjustment per observation.
const STEP_SIZE: f64 = 0.01;

/// Minimum number of observations before tuning begins.
const MIN_SAMPLES: usize = 5;

// ═════════════════════════════════════════════════════════════════════════════
// State
// ═════════════════════════════════════════════════════════════════════════════

/// Internal state tracking for the recall tuner.
struct TunerState {
    /// Current adapted threshold.
    threshold: f64,
    /// Exponential moving average of NDCG scores.
    ema_ndcg: f64,
    /// Total observations recorded.
    observations: usize,
}

/// Global tuner state. Initialized lazily on first access.
static TUNER: LazyLock<Mutex<TunerState>> = LazyLock::new(|| {
    Mutex::new(TunerState {
        threshold: DEFAULT_THRESHOLD,
        ema_ndcg: 0.5, // neutral starting point
        observations: 0,
    })
});

// ═════════════════════════════════════════════════════════════════════════════
// Public API
// ═════════════════════════════════════════════════════════════════════════════

/// Record an NDCG observation from a completed search and return the
/// (potentially adjusted) similarity threshold for the next search.
///
/// Call this after every search that produces a non-empty result set.
/// The returned threshold can be passed to `MemorySearchConfig.similarity_threshold`.
pub fn observe_and_tune(ndcg: f64) -> f64 {
    let mut state = TUNER.lock();

    // Update EMA
    if state.observations == 0 {
        state.ema_ndcg = ndcg;
    } else {
        state.ema_ndcg = EMA_ALPHA * ndcg + (1.0 - EMA_ALPHA) * state.ema_ndcg;
    }
    state.observations += 1;

    // Don't adjust until we have enough samples
    if state.observations < MIN_SAMPLES {
        return state.threshold;
    }

    // Adjust threshold based on quality signal
    if state.ema_ndcg < NDCG_LOW {
        // Quality is poor → loosen threshold (lower = more permissive)
        state.threshold = (state.threshold - STEP_SIZE).max(MIN_THRESHOLD);
    } else if state.ema_ndcg > NDCG_HIGH {
        // Quality is excellent → tighten threshold (higher = more selective)
        state.threshold = (state.threshold + STEP_SIZE).min(MAX_THRESHOLD);
    }
    // Between NDCG_LOW and NDCG_HIGH → hold steady (hysteresis zone)

    state.threshold
}

/// Get the current adapted threshold without recording an observation.
pub fn current_threshold() -> f64 {
    TUNER.lock().threshold
}

/// Get the current EMA NDCG for diagnostics.
pub fn current_ema_ndcg() -> f64 {
    TUNER.lock().ema_ndcg
}

// ═════════════════════════════════════════════════════════════════════════════
// Tests
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    /// Test the tuner with a fresh state (not using the global singleton).
    fn make_state() -> TunerState {
        TunerState {
            threshold: DEFAULT_THRESHOLD,
            ema_ndcg: 0.5,
            observations: 0,
        }
    }

    fn step(state: &mut TunerState, ndcg: f64) -> f64 {
        if state.observations == 0 {
            state.ema_ndcg = ndcg;
        } else {
            state.ema_ndcg = EMA_ALPHA * ndcg + (1.0 - EMA_ALPHA) * state.ema_ndcg;
        }
        state.observations += 1;

        if state.observations < MIN_SAMPLES {
            return state.threshold;
        }

        if state.ema_ndcg < NDCG_LOW {
            state.threshold = (state.threshold - STEP_SIZE).max(MIN_THRESHOLD);
        } else if state.ema_ndcg > NDCG_HIGH {
            state.threshold = (state.threshold + STEP_SIZE).min(MAX_THRESHOLD);
        }

        state.threshold
    }

    #[test]
    fn test_stable_quality_no_change() {
        let mut state = make_state();
        // Feed stable mid-range NDCG (in hysteresis zone)
        for _ in 0..20 {
            step(&mut state, 0.55);
        }
        // Threshold should remain at default
        assert!((state.threshold - DEFAULT_THRESHOLD).abs() < 1e-9);
    }

    #[test]
    fn test_low_quality_loosens_threshold() {
        let mut state = make_state();
        // Feed consistently low NDCG
        for _ in 0..20 {
            step(&mut state, 0.2);
        }
        // Threshold should have decreased
        assert!(state.threshold < DEFAULT_THRESHOLD);
        // But never below min
        assert!(state.threshold >= MIN_THRESHOLD);
    }

    #[test]
    fn test_high_quality_tightens_threshold() {
        let mut state = make_state();
        // Feed consistently high NDCG
        for _ in 0..20 {
            step(&mut state, 0.9);
        }
        // Threshold should have increased
        assert!(state.threshold > DEFAULT_THRESHOLD);
        // But never above max
        assert!(state.threshold <= MAX_THRESHOLD);
    }

    #[test]
    fn test_bounded() {
        let mut state = make_state();
        // Extreme low quality for many iterations
        for _ in 0..1000 {
            step(&mut state, 0.0);
        }
        assert!(state.threshold >= MIN_THRESHOLD);

        // Extreme high quality for many iterations
        for _ in 0..1000 {
            step(&mut state, 1.0);
        }
        assert!(state.threshold <= MAX_THRESHOLD);
    }
}
