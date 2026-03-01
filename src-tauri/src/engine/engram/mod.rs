// ── Engram: Memory System Module ────────────────────────────────────────────
//
// Project Engram — a biologically-inspired, three-tier memory system.
//
// Architecture:
//   Sensory Buffer (in-memory ring) → Working Memory (active slots) → Long-Term Store (SQLite)
//
// Sub-modules:
//   - tokenizer: Unified token counting (replaces all chars/4 hacks)
//   - model_caps: Per-model capability registry (replaces all hardcoded limits)
//   - sensory_buffer: Tier 0 raw message ring buffer
//   - working_memory: Tier 1 active context with priority eviction
//   - schema: Tier 2 database tables and migrations
//   - graph: Memory graph business logic (store, search, relate, decay, GC)
//   - consolidation: Async pipeline: episodic→semantic extraction, contradiction resolution
//   - context_builder: Budget-aware prompt assembly with token-precise allocation
//   - bridge: Compatibility layer from old engine::memory API to Engram
//   - retrieval_quality: NDCG + relevancy metrics on every search (§5.3/§35)
//   - reranking: 4-strategy reranking pipeline (§35.1) + cross-type dedup (§34.3)
//   - hybrid_search: Auto-detect text-boost weighting (§35.2)
//   - metadata_inference: Auto-extract structured metadata during consolidation (§35.3)

pub mod bridge;
pub mod consolidation;
pub mod context_builder;
pub mod encryption;
pub mod entity_tracking;
pub mod graph;
pub mod hybrid_search;
pub mod intent_classifier;
pub mod metadata_inference;
pub mod model_caps;
pub mod reranking;
pub mod retrieval_quality;
pub mod schema;
pub mod sensory_buffer;
pub mod temporal_search;
pub mod tokenizer;
pub mod working_memory;

// Re-exports for convenience
pub use consolidation::{run_consolidation, ConsolidationReport, GapKind, KnowledgeGap};
pub use context_builder::{AssembledContext, BudgetReport, ContextBuilder};
pub use entity_tracking::{extract_entities, process_memory_entities, merge_entities};
pub use graph::{
    apply_decay, garbage_collect, memory_stats, relate, search, store_episodic_dedup,
    store_procedural, store_semantic_dedup, EngramStats,
};
pub use hybrid_search::resolve_hybrid_weight;
pub use intent_classifier::{classify_intent, intent_weights};
pub use metadata_inference::{infer_metadata, infer_metadata_full};
pub use model_caps::{
    resolve_context_window, resolve_max_output_tokens, resolve_model_capabilities,
};
pub use reranking::{cross_type_dedup, rerank_results};
pub use retrieval_quality::{
    assess_quality, build_quality_metrics, build_recall_result, compute_average_relevancy,
    compute_ndcg,
};
pub use sensory_buffer::SensoryBuffer;
pub use temporal_search::{temporal_search, recency_score, cluster_temporal};
pub use tokenizer::Tokenizer;
pub use working_memory::WorkingMemory;
