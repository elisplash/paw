# Engram Implementation Gap Analysis

> Comprehensive comparison of `MEMORY_OVERHAUL_PLAN.md` (9,435 lines, 36 sections) against the
> current Rust implementation in `src-tauri/src/engine/engram/` (14 files, ~6,500 lines),
> `atoms/engram_types.rs` (1,137 lines), and `engine/sessions/engram.rs` (1,201 lines).

---

## Executive Summary

| Category | Count |
|---|---|
| **Fully Implemented** | 26 items |
| **Partially Implemented** | 12 items |
| **Not Implemented** | 31 items |

The **foundation is solid**: the three-tier memory model, DB schema, hybrid search, consolidation pipeline, model capability registry, reranking, retrieval quality metrics, working memory, sensory buffer, tokenizer, and metadata inference are all built. What remains is primarily: **security hardening** (§10), **integration wiring** (§14–§17), **advanced intelligence** (§6, §8.6–8.9, §15.6), **concurrency architecture** (§20), and **pluggable vector backends** (§36).

---

## 1. FULLY IMPLEMENTED

### 1.1 Three-Tier Database Schema (§3 + §24)
- **Plan**: 6 tables (episodic, semantic, procedural, edges, working_memory_snapshots, audit_log) + FTS5 + triggers + indices + anti-forensic padding.
- **Code**: `schema.rs` (395 lines) creates all 6 tables with `CREATE IF NOT EXISTS`, FTS5 virtual tables for episodic + semantic with `porter unicode61` tokenizer, insert/update/delete triggers for FTS sync, 13 indices, and `_engram_padding` table with 512KB bucket quantization.
- **Delta**: Schema column names differ slightly from plan §24 (e.g. `content_full` vs `event`; plan's `schemas` table is missing; plan has `scope_squad_id`/`scope_channel_user_id` on all tables — only episodic has those). FTS tokenizer uses `porter unicode61` vs plan's `unicode61 remove_diacritics 2 tokenchars "._-@#"`.

### 1.2 Episodic Memory CRUD (§3 + §11)
- **Code**: `sessions/engram.rs` — full CRUD: `engram_store_episodic`, `engram_get_episodic`, `engram_delete_episodic`, `engram_update_trust_score`, `engram_record_access`, `engram_set_consolidation_state`, `engram_set_inferred_metadata`, `engram_update_embedding`, `engram_list_without_embeddings`, `engram_count_episodic`.
- **Scope-aware**: BM25 search (`engram_search_episodic_bm25`) and vector search (`engram_search_episodic_vector`) both filter by `scope_agent_id`.

### 1.3 Semantic Memory CRUD (§3)
- **Code**: `sessions/engram.rs` — `engram_store_semantic` (with SPO triple), `engram_get_semantic`, `engram_delete_semantic`, `engram_search_semantic_bm25` (FTS5), `engram_lookup_by_subject`, `engram_count_semantic`.

### 1.4 Procedural Memory CRUD (§3)
- **Code**: `sessions/engram.rs` — `engram_store_procedural`, `engram_get_procedural`, `engram_delete_procedural`, `engram_record_procedural_success`, `engram_record_procedural_failure`, `engram_search_procedural` (LIKE-based), `engram_count_procedural`.

### 1.5 Memory Graph Edges (§5)
- **Code**: `sessions/engram.rs` — `engram_add_edge`, `engram_remove_edge`, `engram_get_edges_from`, `engram_get_edges_to`, `engram_count_edges`.

### 1.6 Spreading Activation (§5, 1-hop)
- **Code**: `sessions/engram.rs` — `engram_spreading_activation(seed_ids, min_weight)` finds 1-hop neighbors, sums weighted edges, excludes seeds, returns sorted.
- **Also**: `graph.rs` calls `store.engram_spreading_activation()` after initial retrieval and merges results.

### 1.7 Sensory Buffer (§3 Tier 1)
- **Code**: `sensory_buffer.rs` (307 lines) — `VecDeque<SensoryEntry>` ring buffer. `push()` returns evicted entry. `drain_within_budget()` respects token limit. `format_for_context()`. Comprehensive tests.

### 1.8 Working Memory (§3 Tier 2)
- **Code**: `working_memory.rs` (494 lines) — Slot-based with token budget. Priority-based eviction (lowest priority evicted first). `insert_recall()`, `insert_sensory()`, `insert_user_mention()`, `insert_tool_result()`. `boost_priority()`, `decay_priorities()`. Momentum embeddings stored. `snapshot()`/`restore()` for agent switching. `format_for_context()` with source tags.

### 1.9 Working Memory Snapshot Persistence (§17.5.5)
- **Code**: `sessions/engram.rs` — `engram_save_snapshot`, `engram_load_snapshot`. Stores `WorkingMemorySnapshot` as JSON in `working_memory_snapshots` table.

### 1.10 Consolidation Pipeline (§4.1–4.3)
- **Code**: `consolidation.rs` (875 lines) — Full pipeline: candidate selection (state=`raw`, 24h+ old), embedding enrichment, union-find clustering with cosine similarity ≥0.80, semantic extraction from clusters (SPO triple from highest-importance member), contradiction detection (same subject+predicate, different object → newer wins, confidence transferred), singleton marking as `reviewed`, metadata inference during consolidation (§35.3). `ConsolidationReport` with cluster/extracted/contradiction/singleton/gap counts. Comprehensive tests.

### 1.11 Strength Decay & Garbage Collection (§4.4)
- **Code**: `graph.rs` — `apply_decay()` implements Ebbinghaus curve (`e^(-t / half_life)`), multiplied by access count bonus. `garbage_collect()` finds candidates below threshold, calls `engram_secure_erase_*` (Phase 1 zero-then-delete), `engram_repad()`, returns count. Respects `importance >= 8` protection (user-explicit).

### 1.12 Gap Detection (§4.5 partial)
- **Code**: `consolidation.rs` — `detect_gaps()` finds 3 gap types via SQL: `StaleHighUse` (high access, old consolidation), `UnresolvedContradiction` (version > 1), `IncompleteSchema` (semantic memories with empty object). Returns `Vec<KnowledgeGap>`. *Note: gaps are detected but NOT injected into working memory as clarifying questions — that integration is not wired.*

### 1.13 Unified Hybrid Search (§5 + §35.2)
- **Code**: `graph.rs` — `search()` runs BM25 (episodic + semantic FTS5) and vector (episodic + semantic brute-force cosine) in parallel, applies hybrid text-boost weighting via `hybrid_search.rs`, fuses with `weighted_rrf_fuse()`, runs spreading activation, deduplicates cross-type, reranks, trims to budget, computes quality metrics.
- **Code**: `hybrid_search.rs` (261 lines) — `auto_detect_query_type()` scores factual signals (file paths, numbers, identifiers, short queries) vs conceptual signals ("how"/"why" prefixes, long queries) and adjusts `text_weight` within configured min/max bounds.

### 1.14 Reranking (§35.1) — 4 strategies
- **Code**: `reranking.rs` (410 lines) — `RRF` (composite score sort), `MMR` (maximal marginal relevance with word-overlap diversity), `RRFThenMMR` (combined), `CrossEncoder` (falls back to RRFThenMMR — not wired to Ollama). `cross_type_dedup()` removes episodic↔semantic duplicates via Jaccard. Tests included.

### 1.15 Retrieval Quality Metrics (§5.3)
- **Code**: `retrieval_quality.rs` (329 lines) — `compute_ndcg()` (NDCG@k), `compute_average_relevancy()`, `build_quality_metrics()` (assembles full `RetrievalQualityMetrics`), `build_recall_result()` (bundles memories + metrics into `RecallResult`), `assess_quality()` (returns warnings for low relevancy, NDCG, high latency, empty results). Tests included.

### 1.16 Model Capability Registry (§15.5)
- **Code**: `model_caps.rs` (831 lines) — `normalize_model_name()` strips date suffixes/preview tags. `resolve_model_capabilities()` → `ModelCapabilities`. Covers Anthropic (claude-opus-4-6 through claude-3-opus), OpenAI (codex-5 through gpt-3.5-turbo), Google (gemini-3.1-pro through gemini-2.0-flash), DeepSeek, Mistral, xAI/Grok, Ollama (llama, qwen). Catch-all prefix fallbacks per provider. Tests for name normalization and capability resolution.

### 1.17 Unified Tokenizer (§8.10)
- **Code**: `tokenizer.rs` (221 lines) — Model-specific chars-per-token ratios: Cl100kBase=3.7, O200kBase=3.9, Gemini=3.5, SentencePiece=3.3, Heuristic=3.5. `count_tokens()`, `count_tokens_for_messages()`, `chars_for_tokens()`, `truncate_to_budget()` (respects UTF-8 boundaries + word breaks). Tests included.

### 1.18 Budget-Aware Context Builder (§8.2)
- **Code**: `context_builder.rs` (728 lines) — Fluent builder: `.base_prompt()`, `.runtime_context()`, `.core_context()`, `.platform_awareness()`, `.foreman_protocol()`, `.skill_instructions()`, `.agent_roster()`, `.todays_memories()`, `.recall_from()`, `.working_memory()`, `.messages()`. Priority-ordered sections (0=highest). `build()` assembles within token budget, auto-recalls memories, injects working memory, trims history (oldest first). `AssembledContext` + `BudgetReport`. Tests.

### 1.19 Metadata Inference (§35.3)
- **Code**: `metadata_inference.rs` (595 lines) — Regex-based extraction of: file paths, URLs, technologies (100+ curated vocabulary with word-boundary matching), programming language (extension + keyword detection), ISO dates. `infer_metadata()` / `infer_metadata_full()`. `serialize_metadata()` / `deserialize_metadata()` for JSON storage. Called during consolidation. Tests.

### 1.20 Anti-Forensic Padding (§10.4.2)
- **Code**: `schema.rs` — `pad_to_bucket()` inflates DB to next 512KB boundary using `_engram_padding` table with 4KB zeroblob rows.

### 1.21 Secure Erasure (§10.6.1)
- **Code**: `sessions/engram.rs` — `engram_secure_erase_episodic()`, `engram_secure_erase_semantic()`, `engram_secure_erase_procedural()`. Phase 1: overwrite content fields with empty. Phase 2: DELETE row + orphaned edges. `engram_repad()` calls `pad_to_bucket()`.

### 1.22 Audit Trail (§19)
- **Code**: `sessions/engram.rs` — `engram_audit_log()`, `engram_audit_history()`. Schema has `memory_audit_log` table with operation/memory_id/agent_id/session_id/details_json columns + indices.

### 1.23 Engram Bridge to Engine (integration point)
- **Code**: `bridge.rs` (241 lines) — `store()`, `store_auto_capture()`, `search()`, `stats()`, `run_maintenance()`. Creates `EpisodicMemory`, routes through `graph::store_episodic_dedup` and `graph::search`. Maintenance runs consolidation + decay + GC. Integrated into `commands/chat.rs` (auto-capture on assistant messages and session summaries) and `engine/channels/agent.rs` (channel agent auto-capture).

### 1.24 All Core Types (§3, §5, §7, §8, §15.5, §35)
- **Code**: `atoms/engram_types.rs` (1,137 lines) — Comprehensive type definitions: `MemoryScope` (hierarchical with `to_sql_where()`), `MemorySource` (8 variants), `ConsolidationState`, `TrustScore` (4-dimensional composite), `TieredContent`, `CompressionLevel`, `EpisodicMemory`, `SemanticMemory`, `ProceduralMemory`, `ProceduralStep`, `EdgeType` (8 variants), `MemoryEdge`, `RetrievedMemory`, `MemoryType`, `MemorySearchConfig`, `HybridSearchConfig`, `RerankStrategy`, `RecallResult`, `RetrievalQualityMetrics`, `EngramConfig` (30+ tunable fields), `ModelCapabilities`, `ModelProvider`, `TokenizerType`, `WorkingMemorySlot`, `WorkingMemorySource`, `WorkingMemorySnapshot`, `MemoryCategory`, `InferredMetadata`, `MetadataFilters`, `AuditEntry`, `AuditOperation`.

### 1.25 Tauri IPC Commands (basic set)
- **Code**: `lib.rs` registers: `engine_memory_store`, `engine_memory_search`, `engine_memory_stats`, `engine_memory_delete`, `engine_memory_list`, `engine_memory_backfill`. These map to the existing memory system. The Engram bridge is called from `commands/chat.rs` for auto-capture.

### 1.26 Deduplication at Storage (§4)
- **Code**: `graph.rs` — `store_episodic_dedup()` uses Jaccard similarity (word overlap) + optional embedding cosine similarity (≥0.85) to detect near-duplicates. Existing memory is boosted in importance rather than creating a duplicate. `store_semantic_dedup()` checks subject+predicate match, handles reconsolidation (version increment, supersedes_id), and contradiction tracking.

---

## 2. PARTIALLY IMPLEMENTED

### 2.1 Spreading Activation — 1-hop only (§5 calls for 2-hop)
- **Implemented**: 1-hop in `sessions/engram.rs` and `graph.rs`.
- **Missing**: Plan specifies 2-hop traversal through typed edges with activation decay. Current code only traverses direct neighbors.
- **Effort**: Low — extend `engram_spreading_activation` to recurse one more level.

### 2.2 CrossEncoder Reranking (§35.1)
- **Implemented**: Strategy enum and selection logic in `reranking.rs`.
- **Missing**: Falls back to `RRFThenMMR` — not actually calling Ollama for cross-encoder scoring.
- **Effort**: Medium — requires Ollama integration for a reranking model.

### 2.3 Tiered Content Compression (§8.8)
- **Implemented**: `TieredContent` type exists in `engram_types.rs`. Schema has `content_full`, `content_summary`, `content_key_fact`, `content_tags` columns.
- **Missing**: Auto-generation of summary/key_fact/tag tiers at storage time. Currently only `content_full` is populated. No LLM or heuristic fallback to generate compressed tiers.
- **Effort**: High — requires LLM integration for summary generation + heuristic fallback.

### 2.4 Self-Healing Memory Graph (§4.5)
- **Implemented**: Gap detection (3 types) in `consolidation.rs`.
- **Missing**: Clarifying question generation and injection into working memory. Gaps are detected and returned in `ConsolidationReport` but never acted upon.
- **Effort**: Medium — need to wire gap results into working memory as `UserMention` or create a new source type.

### 2.5 Trust Score (§7)
- **Implemented**: `TrustScore` type with 4 dimensions (relevance, accuracy, freshness, utility) + `composite()`. Schema has `trust_source`, `trust_consistency`, `trust_recency`, `trust_user_feedback`. `engram_update_trust_score()` updates DB.
- **Missing**: Negative recall filtering (§7 — context-aware suppression of corrected memories). Feedback loop from agent `memory_feedback` tool back to trust scores. Trust score doesn't decay `trust_recency` based on age.
- **Effort**: Medium.

### 2.6 Hierarchical Memory Scoping (§11)
- **Implemented**: `MemoryScope` type with all 6 levels (global/project/squad/agent/channel/channel_user). `to_sql_where()` builds parameterized WHERE clause. Schema has scope columns. DB queries filter by `scope_agent_id`.
- **Missing**: Full `squad_id` integration (no squad concept in the current codebase). `channel_user_id` scoping not wired in channel bridges. `to_sql_where()` implementation in types vs actual querying in `sessions/engram.rs` only uses `agent_id` filter — doesn't use project/squad/channel filters in search queries.
- **Effort**: Medium — the infrastructure is there, needs wiring into actual queries.

### 2.7 EngramConfig (§18.5)
- **Implemented**: Full `EngramConfig` struct with 30+ fields and defaults in `engram_types.rs`.
- **Missing**: Frontend ↔ backend sync via IPC. Config hot-reload (`ArcSwap`). Config persistence to DB. Frontend `SearchConfig` remains decorative. Config validation with range checks (§34.6).
- **Effort**: Medium — config storage + IPC wiring + validation.

### 2.8 Chat Auto-Recall (§8 + §17.5.1 step 2)
- **Implemented**: `bridge::search()` is available and `context_builder.rs` has `.recall_from()`. `commands/chat.rs` includes Engram auto-capture on assistant messages.
- **Missing**: Auto-recall is not wired into the chat pipeline's context building before the LLM call. The context builder *can* do it, but the chat command doesn't call it for auto-recall of Engram memories.
- **Effort**: Low-Medium — wire `bridge::search()` into chat's context assembly.

### 2.9 Bridge Auto-Capture Completeness
- **Implemented**: `store_auto_capture()` called from `commands/chat.rs` (on assistant replies + session summaries) and `channels/agent.rs`.
- **Missing**: Not called from tasks, orchestrator, swarm, or flows. Plan §17.5.1 requires ALL execution paths participate.
- **Effort**: Medium — needs integration points in each execution path.

### 2.10 Vector Search — Brute Force (§9 calls for HNSW)
- **Implemented**: `sessions/engram.rs` — `engram_search_episodic_vector()` loads all embeddings and computes cosine similarity in a loop (O(n)).
- **Missing**: HNSW index (O(log n)). Current approach works for <10K memories but doesn't scale.
- **Effort**: High — HNSW implementation or integration of an HNSW crate.

### 2.11 Momentum Vector (§8.6)
- **Implemented**: `WorkingMemory` stores `momentum_embeddings`. `WorkingMemorySnapshot` persists them.
- **Missing**: Momentum embeddings are never used for trajectory-aware recall. No blending with query embedding during search.
- **Effort**: Medium — compute weighted average of recent query embeddings, blend into search query.

### 2.12 FTS5 Tokenizer Configuration (§34.7)
- **Implemented**: FTS5 uses `porter unicode61` tokenizer.
- **Missing**: Plan calls for `unicode61 remove_diacritics 2 tokenchars "._-@#"` for code-friendly tokenization (preserving file paths, identifiers). Current tokenizer may poorly handle code tokens.
- **Effort**: Low — change tokenizer string in schema.

---

## 3. NOT IMPLEMENTED

### Security (§10)

| Item | Plan Section | Priority | Effort | Dependencies |
|---|---|---|---|---|
| SQLCipher full-DB encryption | §10.3 | P2 High | 3 days | `bundled-sqlcipher` feature flag |
| Field-level tiered encryption (cleartext/sensitive/confidential) | §10.5 | P2 High | 2 days | Keychain field key |
| PII auto-detection | §10.5.2 | P2 High | 2 days | Regex + heuristic |
| `mlock` + `zeroize` for decrypted content | §10.7 | P2 High | 1 day | `secrecy` + `zeroize` crates |
| Disable core dumps on startup | §10.7.2 | P2 High | 0.5 day | None |
| Hidden vault path + filesystem hardening | §10.2 | P3 Medium | 1 day | Platform detection |
| Decoy database at old path | §10.2.4 | P3 Medium | 0.5 day | Hidden vault |
| Constant-time search envelope | §10.8 | P3 Medium | 1 day | Tiered encryption |
| Encrypted export/import | §10.9 | P3 Medium | 2 days | Field-level encryption |
| Score quantization (semantic oracle resistance) | §10.16.1 | P3 Medium | 0.5 day | None |
| Search rate limiting | §10.16.2 | P3 Medium | 0.5 day | None |
| Differential privacy noise on scores | §10.16.3 | P4 Low | 0.5 day | Score quantization |
| Deniable encryption (hidden volume) | §10.10 | P4 Low | 3 days | SQLCipher + stable padding |
| HKDF key derivation hierarchy | §10.24.6 | P3 Medium | 1 day | All keychain keys |
| Cross-agent access control (caller-derived identity) | §10.13 | P2 Critical | 2 days | Session identity |
| Prompt injection scanning on recalled memories | §10.14 | P2 High | 1 day | Existing scanner |
| Log redaction macro + log file hardening | §10.12 | P2 High | 1 day | None |
| Input validation / size limits | §10.17 | P2 High | 0.5 day | None |
| FTS5 query sanitization | §10.15.1 | P2 High | 0.5 day | None |
| LIKE pattern escaping | §10.15.2 | P4 Low | 0.5 day | None |
| Snapshot encryption + buffer zeroization | §10.19 | P3 Medium | 1 day | Field-level key |
| GDPR right-to-erasure API | §10.20 | P3 Medium | 1 day | Secure erase + FTS rebuild |
| FTS5 shadow table cleanup after erase | §10.18.2 | P3 Medium | 0.5 day | None |

### Intelligence Features (§6, §8, §15.6)

| Item | Plan Section | Priority | Effort | Dependencies |
|---|---|---|---|---|
| Smart chunking / proposition decomposition | §6 | P1 High | 2 days | LLM + heuristic fallback |
| BM25 single-result normalization bug fix | §6.3 | P0 Critical | 0.5 day | None |
| Anticipatory pre-loading | §8.9 | P1 Medium | 2 days | Momentum vector |
| Topic-change detection → working memory eviction | §8.7 | P1 Medium | 1 day | Momentum vector |
| Smart history compression (verbatim/compressed/summary tiers) | §8.3 | P1 High | 2 days | LLM + tokenizer |
| Negative recall filtering | §7 | P0 High | 1 day | Trust score feedback |
| Token-budget knapsack packing | §8.4 | P0 High | 1 day | Tiered content |
| Recursive memory-augmented reasoning | §15.6 | P0 High | 3 days | LLM + graph search |
| Transitive inference during consolidation | §15.6 | P1 Medium | 2 days | LLM |

### Integration Wiring (§12–§17)

| Item | Plan Section | Priority | Effort | Dependencies |
|---|---|---|---|---|
| Channel memory isolation (per-channel, per-user scope) | §12 | P0 High | 2 days | MemoryScope wiring |
| Compaction → memory bridge (propositions from summaries) | §13 | P0 High | 1 day | Proposition extraction |
| Expanded agent tools (update/delete/list/feedback/relate) | §14 | P0 High | 2 days | Tauri commands |
| `memory_reason` agent tool | §15.6 | P0 High | 1 day | RecursiveReasoner |
| Skill → memory bridge | §15 | P2 Low | 1 day | None |
| Task → memory bridge (auto-recall + auto-capture) | §15 + §17.5.3 | P0 High | 2 days | Task execution changes |
| Flow/Conductor memory (pre-recall + post-capture + procedural) | §16 | P0 High | 2 days | Flow executor changes |
| Orchestrator memory (project-scoped recall/capture) | §17 | P0 High | 2 days | Orchestrator changes |
| Swarm memory (fix `todays_memories: None`) | §17 | P0 High | 0.5 day | None |
| Memory tool scoping fix (`agent_id` passthrough) | §17.5.8 | P0 Critical | 0.5 day | None |
| Research → memory bridge | §15.6 | P0 High | 2 days | Research view changes |
| MCP memory context injection | §15/§22 | P2 Medium | 1 day | MCP capability flag |

### Concurrency & Architecture (§20–§22)

| Item | Plan Section | Priority | Effort | Dependencies |
|---|---|---|---|---|
| Read pool (r2d2, 8 WAL connections) | §20 | P0 High | 2 days | r2d2 crate |
| Write channel (mpsc + dedicated writer task) | §20 | P0 High | 2 days | tokio mpsc |
| `MemoryGraph` struct (replaces direct SessionStore calls) | §20 | P0 High | 2 days | Read pool + write channel |
| BackgroundScheduler (foreground yield, heavy_op_lock) | §22 | P0 High | 2 days | MemoryGraph |
| RamMonitor with adaptive pressure levels | §21 | P0 High | 1 day | sysinfo crate |
| Engram heartbeat loop (consolidation/GC/backfill every 60s) | §22 | P0 Medium | 1 day | BackgroundScheduler |

### Pluggable Vector Backend (§36)

| Item | Plan Section | Priority | Effort | Dependencies |
|---|---|---|---|---|
| `VectorBackend` trait + registry | §36.1 | P0 High | 2 days | None |
| HNSW in-memory backend | §36.1 | P0 High | 3 days | HNSW crate |
| Flat brute-force backend (<1K) | §36.1 | P0 Low | 0.5 day | VectorBackend trait |
| mmap-HNSW disk-resident backend | §36.4 | P1 Medium | 3 days | HNSW + mmap |
| sqlite-vec disk backend | §36.1 | P1 Medium | 1 day | sqlite-vec extension |
| Scalar quantization (SQ8) | §36.2 | P1 Medium | 2 days | VectorBackend trait |
| Product quantization (PQ) | §36.2 | P2 Low | 3 days | VectorBackend trait |
| Named vector spaces (multi-model) | §36.5 | P1 Medium | 2 days | VectorBackend trait |
| Adaptive index selection | §36.6 | P1 Medium | 1 day | Multiple backends |

### Frontend & Debugging (§25 Phase 5)

| Item | Plan Section | Priority | Effort |
|---|---|---|---|
| Memory Palace graph visualization | §25 Phase 5 | P1 | 3 days |
| Retrieval quality metrics in chat debug panel | §35 | P1 | 1 day |
| Hybrid search weight tuner in settings | §35.2 | P2 | 0.5 day |
| Metadata filter UI (tech/people/language/path) | §35.3 | P2 | 1 day |
| Agent switch with abort + working memory resume | §17.5.5-6 | P0 | 1 day |
| Token meter reset on model switch | §17.5.7 | P0 | 0.5 day |
| Working memory status indicator | §17.5.5 | P2 | 0.5 day |
| Recursive reasoning visualization | §15.6 | P2 | 2 days |

### Startup / Shutdown / Migration (§29, §32, §33)

| Item | Plan Section | Priority | Effort |
|---|---|---|---|
| Feature-flagged migration from `memories` → Engram | §29 | P0 | 2 days |
| Startup integrity checks (PRAGMA integrity_check) | §32.4 | P0 | 1 day |
| Ollama health probe with 30s cache | §32.2 | P0 | 0.5 day |
| Search degradation levels (Full/BM25Only/Degraded) | §32.3 | P0 | 1 day |
| Graceful shutdown with anti-forensic cleanup | §33.2 | P0 | 1 day |
| Crash recovery detection + WAL checkpoint | §33.3 | P0 | 0.5 day |
| Auto-backup on startup (3-rotation) | §34.14 | P1 | 0.5 day |

---

## 4. Recommended Implementation Order

Based on dependencies, impact, and the plan's own phase structure:

### Immediate (Week 1-2): Wire Foundation
1. **Memory tool scoping fix** (§17.5.8) — Critical security bug, 0.5 day
2. **Chat auto-recall wiring** — Connect `bridge::search()` to chat context, 1 day
3. **BM25 normalization bug** (§6.3) — Data quality, 0.5 day
4. **FTS5 query sanitization** (§10.15) — Security, 0.5 day
5. **Input validation / size limits** (§10.17) — Security, 0.5 day
6. **2-hop spreading activation** — Low-hanging retrieval quality win, 0.5 day
7. **FTS tokenizer upgrade** — Change to `unicode61 remove_diacritics 2 tokenchars "._-@#"`, 0.5 day

### Near-term (Week 3-4): Concurrency + Integration
8. **Read pool + write channel** (§20) — Eliminates mutex contention, 4 days
9. **Task/cron memory lifecycle** (§17.5.3) — High-value integration, 2 days
10. **Orchestrator/swarm memory** (§17) — Fix `todays_memories: None`, 2 days
11. **Expanded agent tools** (§14) — 2 days
12. **Cross-agent access control** (§10.13) — Critical security, 2 days
13. **Log redaction** (§10.12) — Security, 1 day

### Mid-term (Week 5-6): Intelligence
14. **Tiered content generation** (§8.8) — LLM summary/fact/tag, 2 days
15. **Proposition decomposition** (§6) — LLM + heuristic, 2 days
16. **Smart history compression** (§8.3) — 2 days
17. **Momentum vector usage in search** (§8.6) — 1 day
18. **Topic-change detection** (§8.7) — 1 day
19. **Negative recall filtering** (§7) — 1 day
20. **Self-healing gap → working memory injection** (§4.5) — 1 day

### Later (Week 7+): Security + Vector Backends + Migration
21. **SQLCipher** (§10.3) — 3 days
22. **Field-level encryption** (§10.5) — 2 days
23. **HNSW in-memory backend** (§36) — 3 days
24. **Feature-flagged migration** (§29) — 2 days
25. **Frontend wiring** — Phase 5 items, 2+ weeks

---

## 5. Key Observations

1. **The foundation is production-quality.** The type system, schema, consolidation pipeline, hybrid search, reranking, and model registry are well-tested and comprehensive.

2. **The biggest gap is integration wiring.** The Engram "brain" exists but is only wired into chat auto-capture. Tasks, orchestrator, swarm, flows, and channels don't use it for recall or capture.

3. **Security is entirely unimplemented** beyond secure erasure and padding. The plan specifies 14 defense layers; only 2 are built (padding + two-phase erase).

4. **Vector search is O(n).** Brute-force cosine scan will be the first bottleneck at scale. HNSW or a pluggable backend is needed before 10K+ memories.

5. **Concurrency is a single mutex.** All operations go through `SessionStore.conn.lock()`. The plan's r2d2 read pool + mpsc write channel architecture would be a major improvement.

6. **The plan is extremely detailed** (9,435 lines, 36 sections) and internally consistent. Implementation can follow it fairly literally.

---

## Session 2: Integration & Security Implementation

The following items were implemented to close critical integration and security gaps:

### Newly Implemented

| Item | Files Modified | Description |
|---|---|---|
| **ContextBuilder wired into chat pipeline** | `commands/chat.rs` | Replaced legacy `compose_chat_system_prompt` with `ContextBuilder::new(&model)` chain, with fallback to legacy path |
| **Anthropic max_tokens via registry** | `providers/anthropic.rs` | Replaced hardcoded 4096/8192 with `resolve_max_output_tokens(model)` |
| **Channel agent model-aware context** | `channels/agent.rs` | Replaced hardcoded 16K cap with `resolve_context_window()` |
| **Swarm auto-recall** | `swarm.rs` | Added Engram memory search with squad scope (was `None // todays_memories`) |
| **Agent tools scoping** | `tools/memory.rs`, `tools/mod.rs` | Threaded `agent_id` through all 7 memory tools |
| **3 new agent tools** | `tools/memory.rs` | `memory_delete`, `memory_update`, `memory_list` with agent scoping |
| **DB methods for update/list** | `sessions/engram.rs` | `engram_update_episodic_content`, `engram_list_episodic` |
| **Consolidation timer** | `lib.rs` | 5-minute background timer calling `run_maintenance()` |
| **Working memory save/restore** | `commands/memory.rs`, `lib.rs` | Tauri commands for `WorkingMemorySnapshot` persistence |
| **Unified tokenizer in graph.rs** | `engram/graph.rs` | Replaced 3x `len()/4` with `Tokenizer::heuristic().count_tokens()` |
| **Channel/user memory scoping** | `engram/bridge.rs`, `channels/agent.rs` | `store_auto_capture` now accepts channel/channel_user_id params |
| **Field-level encryption (§10.5)** | `engram/encryption.rs` (new, 766 lines) | AES-256-GCM with separate keychain key, PII auto-detection (9 patterns), tiered encryption (Cleartext/Sensitive/Confidential) |
| **FTS5 query sanitization (§10.15)** | `engram/encryption.rs` | `sanitize_fts5_query()` strips FTS5 operators/syntax |
| **Input validation (§10.17)** | `engram/encryption.rs`, `engram/bridge.rs` | `validate_memory_input()` with 256KB max, null-byte check |
| **Prompt injection scanning (§10.14)** | `engram/encryption.rs`, `engram/bridge.rs` | `sanitize_recalled_memory()` with 10 injection patterns, applied on retrieval |
| **Log redaction (§10.12)** | `engram/encryption.rs`, `engram/bridge.rs` | `redact_for_log()` and `safe_log_preview()` strip PII from log output |
| **GDPR purge API (§10.20)** | `engram/encryption.rs`, `commands/memory.rs`, `lib.rs` | `engram_purge_user()` with secure erase across all memory tables |
| **Compaction to Engram bridge (§13)** | `engine/compaction.rs` | Compaction summaries now stored in Engram episodic memory |
| **Task memory lifecycle (§15)** | `engine/tasks.rs` | Pre-recall (10 memories injected into task context) + post-capture (task results stored in Engram) |
| **Orchestrator memory lifecycle (§17)** | `engine/orchestrator/mod.rs` | Pre-recall (project-relevant memories for boss agent) + post-capture (project outcomes stored) |
| **Unified category alignment (§18)** | `atoms/engram_types.rs`, `tools/memory.rs`, frontend atoms | Expanded `MemoryCategory` enum to 18 categories, aligned across Rust, tool definitions, and frontend |

### Remaining Gaps (Not Yet Implemented)

| Priority | Item | Effort |
|---|---|---|
| P1 | SQLCipher full-DB encryption | Medium |
| P1 | Hidden vault path + filesystem hardening | Small |
| P2 | Process memory hardening (mlock, core dump, zeroize Drop) | Medium |
| P2 | Proposition decomposition (smart chunking) | Large |
| P2 | Smart history compression (verbatim/compressed/summary) | Medium |
| P2 | Topic-change detection + working memory eviction | Medium |
| P2 | HNSW vector index for O(log n) search | Large |
| P2 | Momentum vector blending in search | Small |
| P3 | Anticipatory pre-loading (zero-latency recall) | Medium |
| P3 | Recursive memory-augmented reasoning | Large |
| P3 | Concurrency architecture (ReadPool/WritePool) | Large |
| P3 | n8n memory access | Medium |
| P3 | Pluggable vector backend | Medium |
| P3 | Timing side-channel resistance | Small |
| P3 | Encrypted export/import | Medium |
| P4 | Deniable encryption (hidden volume) | Large |
| P4 | RAM budget and pressure management | Large |
| P4 | Graceful degradation / offline mode | Medium |
