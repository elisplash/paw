// Embedding-Indexed Tool Registry — Module barrel
//
// Persistent tool embedding storage with four-tier search failover.
// Extends existing Tool RAG (tool_index.rs) with SQLite persistence,
// incremental indexing, hierarchical search, and domain centroids.
//
// Atomic structure:
//   atoms.rs     — Pure types, BM25 scoring, domain classifier, cosine similarity
//   molecules.rs — SQLite persistence, incremental indexing, hierarchical search

pub mod atoms;
pub mod molecules;

// Re-export primary types
pub use atoms::{
    bm25_score, bm25_tokenize, classify_domain_by_keywords, cosine_similarity, SearchTier,
    ToolEmbeddingRecord, ToolSource,
};
pub use molecules::{PersistentToolRegistry, ToolSearchResult};
