// ─────────────────────────────────────────────────────────────────────────────
// Speculative Tool Execution — Module barrel
//
// CPU branch prediction for agents: predict the next tool call based on
// historical transition patterns, pre-warm connections, and optionally
// pre-fire read-only operations while the model is still generating.
//
// Safety invariant: only READ operations are ever speculatively executed.
// Writes, creates, deletes, and mutations are never pre-fired.
//
// Architecture:
//   atoms.rs     — Pure types, constants, classification, prediction logic
//   molecules.rs — SQLite transition store, in-memory cache, connection warming
// ─────────────────────────────────────────────────────────────────────────────

pub mod atoms;
pub mod molecules;

// Re-export primary types for ergonomic access.
pub use atoms::{
    CacheEntry, CacheStats, SpeculationCandidate, SpeculationConfig, SpeculationOutcome,
    SpeculationStats, SpeculativeResult, ToolMutability, ToolTransition, TransitionMatrix,
    WarmTarget,
};

pub use atoms::{
    classify_tool_mutability, compute_hit_rate, compute_transition_probabilities,
    format_speculation_summary, hash_tool_args, is_cache_valid, is_read_only_tool,
    merge_transition_count, predict_next_tools, resolve_outcome, should_speculate,
    warm_target_for_domain,
};

pub use molecules::{
    log_session_speculation_stats, log_speculation_outcome, predict_and_record, warm_connection,
    warm_connections_batch, SpeculationSession, SpeculativeCache, TransitionStore,
};
