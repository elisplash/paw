# Engram Implementation — Status Report

> Current state of Project Engram (memory overhaul) implementation.
> Covers `src-tauri/src/engine/engram/` (15 modules), `atoms/engram_types.rs`,
> `engine/sessions/engram.rs`, and all integration points.

---

## Executive Summary

| Status | Count |
|---|---|
| **Fully Implemented** | 34 items |
| **Partially Implemented** | 10 items |
| **Not Started** | 18 items |

The core architecture, search, consolidation, security, and integration wiring are complete.
Remaining work is primarily: advanced intelligence (proposition decomposition, smart compression),
concurrency architecture, pluggable vector backends, and some security hardening.

---

## Implemented

### Core Architecture (§2–§3)

| Feature | Module | Notes |
|---|---|---|
| Three-tier memory model | `sensory_buffer.rs`, `working_memory.rs`, `graph.rs` | Sensory → Working → Long-Term |
| Six-table SQLite schema | `schema.rs` | FTS5, triggers, 13 indices, anti-forensic padding |
| Episodic memory CRUD | `sessions/engram.rs` | Scope-aware BM25 + vector search |
| Semantic memory CRUD (SPO triples) | `sessions/engram.rs` | Reconsolidation on subject+predicate match |
| Procedural memory CRUD | `sessions/engram.rs` | Success/failure tracking |
| Graph edges (8 types) | `sessions/engram.rs` | Add, remove, query edges |
| Spreading activation (1-hop) | `graph.rs`, `sessions/engram.rs` | Boosts neighbors of retrieved memories |
| Deduplication at storage | `graph.rs` | Jaccard + cosine similarity, importance boost |
| Memory audit trail | `sessions/engram.rs` | `memory_audit_log` table |
| All core types | `engram_types.rs` (1,169 lines) | 40+ types, enums, configs |

### Search & Retrieval (§5, §35)

| Feature | Module | Notes |
|---|---|---|
| Hybrid BM25 + vector search | `graph.rs` | Parallel execution, weighted RRF fusion |
| Query classification | `hybrid_search.rs` | Factual vs. conceptual signal scoring |
| Reranking (RRF, MMR, RRF+MMR) | `reranking.rs` | Cross-type deduplication via Jaccard |
| Retrieval quality metrics | `retrieval_quality.rs` | NDCG, avg relevancy, quality warnings |
| Token-budget context assembly | `context_builder.rs` | Fluent API, priority-ordered, model-aware |
| Model-specific tokenizer | `tokenizer.rs` | 5 tokenizer types, UTF-8 safe truncation |
| Model capability registry | `model_caps.rs` | All major providers, fallback defaults |
| Metadata inference | `metadata_inference.rs` | Tech stack, URLs, file paths, languages |

### Consolidation (§4)

| Feature | Module | Notes |
|---|---|---|
| Full consolidation pipeline | `consolidation.rs` (875 lines) | Union-find clustering, SPO extraction |
| Contradiction detection | `consolidation.rs` | Newer wins, confidence transfer, `Contradicts` edges |
| Ebbinghaus strength decay | `graph.rs` | Exponential decay with access bonus |
| Garbage collection | `graph.rs` | Importance protection, two-phase secure erase |
| Gap detection (3 types) | `consolidation.rs` | Detected but not injected into working memory |
| Background consolidation timer | `lib.rs` | 5-minute cycle calling `run_maintenance()` |

### Security (§10)

| Feature | Module | Notes |
|---|---|---|
| Field-level AES-256-GCM encryption | `encryption.rs` (766 lines) | Separate keychain key (`paw-memory-vault`) |
| PII auto-detection (9 patterns) | `encryption.rs` | SSN, email, credit card, phone, address, etc. |
| Three security tiers | `encryption.rs` | Cleartext / Sensitive / Confidential |
| FTS5 query sanitization | `encryption.rs` | Strips FTS5 operators before search |
| Input validation & size limits | `encryption.rs` | 256 KB max, null-byte check |
| Prompt injection scanning | `encryption.rs` | 10 injection patterns on recalled memories |
| Log redaction | `encryption.rs` | PII replaced with `[TYPE]` placeholders |
| GDPR right-to-erasure | `encryption.rs`, `commands/memory.rs` | Purges all tables + snapshots + audit |
| Anti-forensic padding | `schema.rs` | 512 KB bucket quantization |
| Secure erasure (two-phase) | `sessions/engram.rs` | Zero content → delete row |
| 8KB pages + secure_delete | `sessions/mod.rs` | PRAGMA settings on DB open |
| 13 unit tests for encryption | `encryption.rs` | Roundtrip, PII detection, purge, etc. |

### Integration Wiring (§12–§17)

| Feature | Module | Notes |
|---|---|---|
| Chat auto-capture | `commands/chat.rs` | Assistant messages + session summaries |
| Chat auto-recall (ContextBuilder) | `commands/chat.rs` | `.recall_from()` when `auto_recall` enabled |
| Task pre-recall | `engine/tasks.rs` | 10 memories injected into task agent system prompt |
| Task post-capture | `engine/tasks.rs` | Results stored via Engram bridge with PII encryption |
| Orchestrator pre-recall | `engine/orchestrator/mod.rs` | Project memories for boss agent |
| Orchestrator post-capture | `engine/orchestrator/mod.rs` | Project outcomes stored in memory |
| Swarm auto-recall | `engine/swarm.rs` | Squad-scoped memory search |
| Compaction → Engram bridge | `engine/compaction.rs` | Summaries stored as `session` category |
| Channel/user memory scoping | `bridge.rs`, `channels/agent.rs` | Channel + user params on store |
| Engram bridge (public API) | `bridge.rs` | `store`, `store_auto_capture`, `search`, `stats`, `run_maintenance` |
| Model-aware context windows | `providers/anthropic.rs`, `channels/agent.rs` | Registry-based, no hardcoded limits |

### Agent Tools (§14)

| Tool | Module | Notes |
|---|---|---|
| `memory_store` (18 categories) | `tools/memory.rs` | Routes through Engram + legacy dual-write |
| `memory_search` | `tools/memory.rs` | Hybrid search, agent-scoped |
| `memory_knowledge` | `tools/memory.rs` | SPO triple store |
| `memory_stats` | `tools/memory.rs` | Engram statistics |
| `memory_delete` | `tools/memory.rs` | By memory ID |
| `memory_update` | `tools/memory.rs` | Update content |
| `memory_list` | `tools/memory.rs` | Agent-scoped + category filter |
| Agent ID scoping | `tools/mod.rs` | All 7 tools receive `agent_id` |

### Category Alignment (§18)

18 categories unified across Rust enum, agent tool enum, frontend `MEMORY_CATEGORIES`,
flow `MEMORY_CATEGORY_OPTIONS`, and auto-capture paths. Unknown categories fall back to `general`.

### Tauri Commands

15 memory commands registered in `lib.rs`, including `engine_working_memory_save`,
`engine_working_memory_restore`, and `engine_memory_purge_user`.

---

## Partially Implemented

| Feature | What Exists | What's Missing |
|---|---|---|
| **Spreading activation** (§5) | 1-hop traversal | Plan specifies 2-hop |
| **CrossEncoder reranking** (§35.1) | Strategy enum, fallback to RRF+MMR | No Ollama cross-encoder call |
| **Tiered content** (§8.8) | Schema has 4 columns, `TieredContent` type | Only `content_full` populated; no auto-summarize |
| **Self-healing gaps** (§4.5) | Gap detection (3 types) | Gaps not injected into working memory |
| **Trust scores** (§7) | `TrustScore` type, DB update method | No decay, no feedback tool, no negative filtering |
| **Hierarchical scoping** (§11) | `MemoryScope` type, `to_sql_where()` | Search queries only filter by `agent_id` |
| **EngramConfig** (§18.5) | 30+ field struct with defaults | No frontend sync, no hot-reload, no persistence |
| **Momentum vectors** (§8.6) | Stored in snapshots | Never used for trajectory-aware search |
| **Flow memory nodes** (§16) | `executeMemoryWrite/Recall` in flows | Routes through legacy, not Engram bridge |
| **WM snapshot content** (§17.5.5) | Tauri commands save/restore | Saves placeholder; actual slots need engine-level capture |

---

## Not Started

### Security
- SQLCipher full-DB encryption (§10.3)
- Hidden vault path + filesystem hardening (§10.2)
- Process memory hardening — mlock, core dump prevention, zeroize Drop (§10.7)
- Cross-agent access control with caller identity (§10.13)
- Encrypted export/import (§10.9)
- Snapshot encryption (§10.19)

### Intelligence
- Proposition decomposition / smart chunking (§6)
- Smart history compression — verbatim/compressed/summary tiers (§8.3)
- Topic-change detection + working memory eviction (§8.7)
- Anticipatory pre-loading (§8.9)
- Recursive memory-augmented reasoning (§15.6)

### Architecture
- Concurrency — read pool + write channel (§20)
- HNSW vector index for O(log n) search (§36)
- Pluggable vector backend trait (§36)
- RAM budget and pressure management (§21)

### Integration
- `memory_feedback` and `memory_relate` agent tools (§14)
- Research → memory bridge (§15)
- n8n/MCP memory context injection (§22)

---

## Priority Matrix

| Priority | Items | Effort |
|---|---|---|
| **P1** | SQLCipher, hidden vault, 2-hop activation, flow memory bridge | Medium |
| **P2** | Proposition decomposition, smart compression, HNSW index, concurrency | Large |
| **P2** | Process hardening, momentum vectors, trust score decay | Medium |
| **P3** | Anticipatory pre-loading, recursive reasoning, encrypted export | Large |
| **P3** | n8n access, RAM budget, pluggable backends, cross-agent ACL | Large |
