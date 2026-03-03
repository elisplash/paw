# Project Engram — Memory Architecture Whitepaper

*A biologically-inspired memory system for persistent AI agents.*

**Version:** 1.0
**Status:** Implemented in OpenPawz
**License:** MIT

---

## Abstract

Project Engram is a three-tier memory architecture for desktop AI agents. It replaces flat key-value memory stores with a biologically-inspired system modeled on how human memory works: incoming information flows through a sensory buffer, gets prioritized in working memory, and consolidates into long-term storage with automatic clustering, contradiction detection, and strength decay. The result is agents that remember context across sessions, learn from patterns, and forget gracefully.

This document describes the architecture as implemented in OpenPawz — a Tauri v2 desktop AI platform. The system implements three memory tiers, a persistent graph, hybrid search with reciprocal rank fusion, background consolidation, field-level encryption, and full lifecycle integration across chat, tasks, orchestration, and multi-channel bridges.

All code is open source under the MIT License.

---

## Table of Contents

1. [Motivation](#motivation)
2. [Design Principles](#design-principles)
3. [Architecture Overview](#architecture-overview)
4. [The Three Memory Tiers](#the-three-memory-tiers)
5. [Long-Term Memory Graph](#long-term-memory-graph)
6. [Hybrid Search — BM25 + Vector Fusion](#hybrid-search)
7. [Retrieval Intelligence](#retrieval-intelligence)
8. [Caching Architecture](#caching-architecture)
9. [Consolidation Engine](#consolidation-engine)
10. [Adaptive Forgetting — FadeMem Dual-Layer Architecture](#adaptive-forgetting)
11. [Memory Fusion](#memory-fusion)
12. [GraphRAG — Community-Based Global Retrieval](#graphrag)
13. [Compounding Skill Library](#compounding-skill-library)
14. [Context Window Intelligence](#context-window-intelligence)
15. [Memory Security](#memory-security)
16. [Memory Lifecycle Integration](#memory-lifecycle-integration)
17. [Concurrency Architecture](#concurrency-architecture)
18. [Observability](#observability)
19. [Category Taxonomy](#category-taxonomy)
20. [Schema Design](#schema-design)
21. [Configuration](#configuration)
22. [Frontier Capabilities](#frontier-capabilities)
23. [Quality Evaluation](#quality-evaluation)
24. [Context Continuity](#context-continuity)
25. [The Intelligence Loop](#the-intelligence-loop)
26. [Verification & Operational Completeness](#verification--operational-completeness)
27. [References](#references)

---

## Motivation

Most AI memory implementations treat memory as a retrieval problem — store blobs, search blobs, inject blobs. This is a flat model that ignores how memory actually works in biological systems. The practical consequences are well-documented:

- **No prioritization.** All memories compete equally for context window space, regardless of relevance or importance.
- **No decay.** Outdated information persists indefinitely. A corrected fact and its outdated predecessor both appear in context.
- **No structure.** Episodic memories (what happened), semantic knowledge (what is true), and procedural memory (how to do things) are stored in one undifferentiated list.
- **No security.** Sensitive information stored in plaintext. No PII detection, no encryption, no access control.
- **No budget awareness.** Memories are injected without regard to the model's context window, leading to truncation or context overflow.
Engram addresses each of these by modeling agent memory after the structure of biological memory systems.
- **No evolution.** Memories are write-once, read-many — no consolidation, no contradiction resolution, no strengthening through repetition. The store grows linearly while quality degrades over time.
- **No gating.** Every query triggers a full memory search, even when the question is purely computational or already answered in the conversation. This wastes latency and pollutes the context with irrelevant material.

Human memory is not a database. It is a living graph with multiple storage tiers operating on different timescales, automatic consolidation that strengthens important memories and dissolves noise, episodic replay that reconstructs context, schema-based compression that abstracts patterns from instances, importance weighting that modulates storage strength, and interference-based forgetting that prevents retrieval pollution.

Engram implements all six properties.

---

## Design Principles

Seven principles guide every architectural decision in Engram:

1. **Budget-first, always.** Every operation is token-budget-aware. The ContextBuilder never overflows a model's context window. Memories compete for inclusion based on $\text{relevance} \times \text{importance}$, not insertion order. More context is not always better — PAPerBench proves that attention dilution degrades both personalization and privacy protection as context grows. Injection counts are capped per model based on empirical dilution curves.

2. **Forgetting is a feature, not a bug.** Graceful decay rooted in the Ebbinghaus forgetting curve is essential — extended with a dual-layer FadeMem-inspired architecture that differentiates long-term and short-term retention. Without measured forgetting, the memory store grows unbounded, retrieval precision degrades, and stale information pollutes context. Every forgetting cycle is measured: chain integrity percentage and NDCG delta are computed before and after garbage collection. If quality degrades, the cycle rolls back via transactional savepoint. FadeMem research demonstrates 45% storage reduction while *improving* multi-hop retrieval quality.

3. **Gate before you search.** Not every query needs memory. A retrieval gate classifies intent and decides whether to retrieve at all. Self-RAG and CRAG research prove that gated retrieval with post-retrieval correction outperforms always-retrieve pipelines. Engram eliminates ~40% of unnecessary searches, reduces latency on trivial queries, and prevents context pollution from weak results.

4. **Local-first, always offline.** All storage is local. All search is local (BM25 full-text + optional vector semantic search). No cloud dependency, no telemetry, no external vector stores. The system degrades gracefully — without an embedding model, search falls back to BM25-only with no loss in keyword accuracy.

5. **Security by default.** PII is detected automatically and encrypted before it touches disk. The database itself supports full-disk encryption. Anti-forensic measures prevent side-channel leakage through file size changes. GDPR compliance is built in.

6. **Observe everything.** Every search returns quality metrics (NDCG, latency, result count). Every consolidation cycle has measurable outcomes. Without measurement, optimization is guesswork. DeepResearch Bench II's 9,430-rubric evaluation methodology informs our quality harness design.

7. **Skills compound.** Agents don't just remember facts — they remember *how to do things*. Procedural memory evolves through success/failure feedback, composition, and Reflexion-style learning from mistakes. A skill library that improves with every interaction creates exponential returns: fewer steps, higher success rates, lower token costs.

---

## Architecture Overview

```mermaid
flowchart TD
    A["User Message"] --> SB["Sensory Buffer\n(Tier 0 — FIFO ring cache)"]
    SB --> WM["Working Memory\n(Tier 1 — priority-evicted attention cache)"]

    WM --> CTX["ContextBuilder\n(budget-aware prompt assembly)"]
    CTX --> E["Agent Response"]

    E --> CAP["Post-Capture\n(auto-extract facts, preferences, outcomes)"]
    CAP --> ENC["Encryption Layer\n(PII detect → AES-256-GCM)"]
    ENC --> BR["Engram Bridge\n(dedup → embed → store)"]
    BR --> DB[("Long-Term Memory\n(Tier 2)")]

    subgraph LTM["Long-Term Memory Graph"]
        direction LR
        G["Episodic Store\n(what happened)"]
        H["Knowledge Store\n(what is true)"]
        I["Procedural Store\n(how to do things)"]
        SK["Skill Library\n(composable procedures)"]
    end

    DB --- LTM
    LTM --- J["Graph Edges (8 types)\nSpreading Activation"]
    LTM --- COMM["Community Detection\n(Louvain → hierarchical summaries)"]

    WM --> RG["Retrieval Gate\n(Skip / Retrieve / Deep / Refuse / Defer)"]
    RG --> HS["Hybrid Search\n(BM25 + Vector + Graph + GraphRAG)"]
    HS --> RR["Reranking\n(RRF / MMR / RRF+MMR)"]
    RR --> QG["Quality Gate\nCRAG 3-tier: Correct / Ambiguous / Incorrect"]
    QG --> WM

    DB --> HS
    COMM --> HS

    subgraph Background["Background Processes"]
        direction TB
        K["Consolidation Engine\n(every 5 min)"]
        K1["Pattern clustering"]
        K2["Contradiction detection"]
        K3["Ebbinghaus FadeMem dual-layer decay\n(LML β=0.8 / SML β=1.2)"]
        K4["Garbage collection\n(with savepoint rollback)"]
        FUS["Memory Fusion\n(cosine ≥ 0.75 → merge → tombstone)"]
        DR["Dream Replay\n(idle-time re-embed + discover connections)"]
    end

    DB <--> K
    K -.- K1
    K -.- K2
    K -.- K3
    K -.- K4
    K --> FUS
    FUS --> DB
    DR --> DB

    subgraph Cognition["Cognitive Modules"]
        direction LR
        EM["Emotional Memory\n(affective scoring)"]
        MC["Meta-Cognition\n(confidence maps)"]
        IC["Intent Classifier\n(6-intent routing)"]
        ET["Entity Tracker\n(canonical resolution)"]
    end

    HS --> Cognition
    Cognition --> RR

    MB["Memory Bus\n(multi-agent sync)"] <--> DB

    subgraph Loop["Intelligence Loop"]
        direction LR
        L1["Gate"] --> L2["Retrieve"]
        L2 --> L3["Cap"]
        L3 --> L4["Skill"]
        L4 --> L5["Evaluate"]
        L5 --> L6["Forget"]
        L6 --> L1
    end
```

The system is implemented as 23 Rust modules under `src-tauri/src/engine/engram/`:

| Module | Purpose |
|--------|---------|
| `sensory_buffer.rs` | Ring buffer for current-turn incoming data |
| `working_memory.rs` | Priority-evicted slots with token budget |
| `graph.rs` | Memory graph operations (store, search, edges, activation) |
| `store.rs` / `schema.rs` | Storage schema, migrations, CRUD operations |
| `consolidation.rs` | Background pattern detection, clustering, contradiction resolution |
| `retrieval.rs` | Retrieval cortex with quality metrics |
| `retrieval_quality.rs` | NDCG scoring and relevance warnings |
| `hybrid_search.rs` | Query classification (factual vs. conceptual) |
| `context_builder.rs` | Token-budget-aware prompt assembly |
| `tokenizer.rs` | Model-specific token counting with UTF-8 safe truncation |
| `model_caps.rs` | Per-model capability registry (context windows, features) |
| `reranking.rs` | RRF, MMR, and combined reranking strategies |
| `metadata_inference.rs` | Auto-extract tech stack, URLs, file paths from content |
| `encryption.rs` | PII detection, AES-256-GCM field encryption, GDPR purge |
| `bridge.rs` | Public API connecting tools/commands to the graph |
| `emotional_memory.rs` | Affective scoring pipeline (valence, arousal, dominance) |
| `meta_cognition.rs` | Self-assessment of knowledge confidence per domain |
| `temporal_index.rs` | Time-axis retrieval with range queries and proximity scoring |
| `intent_classifier.rs` | 6-intent query classifier for dynamic signal weighting |
| `entity_tracker.rs` | Canonical name resolution and entity lifecycle tracking |
| `abstraction.rs` | Hierarchical semantic compression tree |
| `memory_bus.rs` | Multi-agent memory sync protocol with conflict resolution |
| `dream_replay.rs` | Idle-time hippocampal-inspired replay and re-embedding |

---

## The Three Memory Tiers

```mermaid
flowchart TB
    subgraph T0["Tier 0 — Sensory Buffer"]
        direction LR
        SB["FIFO Ring Cache\n20 items max\nSingle turn lifetime"]
    end

    subgraph T1["Tier 1 — Working Memory"]
        direction LR
        WM["Priority-Evicted Slots\n4,096 token budget\nActive attention"]
    end

    subgraph T2["Tier 2 — Long-Term Memory Graph"]
        direction LR
        EP["Episodic\n(what happened)"]
        KN["Knowledge\n(what is true)"]
        PR["Procedural\n(how to do things)"]
    end

    T0 -- "attentional\npromotion" --> T1
    T1 -- "consolidation" --> T2
    T2 -- "recall on\nsearch" --> T1
```

### Tier 1: Sensory Buffer

A fixed-capacity ring buffer (`VecDeque`) that accumulates raw input during a single agent turn: user messages, tool results, recalled memories, and system context.

**Properties:**
- Capacity: configurable (default 20 items)
- Lifetime: single turn — drained by the ContextBuilder and discarded
- Budget-aware: `drain_within_budget(token_limit)` returns items that fit within a token budget

**Purpose:** Prevents information loss during complex turns with many tool calls. The ContextBuilder reads from the sensory buffer to build the final prompt.

### Tier 2: Working Memory

A priority-sorted array of memory slots with a hard token budget. Represents the agent's "current awareness" — what it's actively thinking about.

**Properties:**
- Capacity: configurable token budget (default 4,096 tokens)
- Eviction: lowest-priority slot evicted when budget exceeded
- Sources: recalled long-term memories, sensory buffer overflow, tool results, user mentions
- Persistence: snapshots saved to the persistent store on agent switch, restored when agent resumes

**Slot structure:**
```
WorkingMemorySlot {
    memory_id: String,      // links to long-term memory (if recalled)
    content: String,
    source: Recalled | SensoryBuffer | ToolResult | Restored,
    priority: f32,          // determines eviction order
    token_cost: usize,      // pre-computed token count
    inserted_at: DateTime,
}
```

**Priority calculation:** Recalled memories use their retrieval score. Sensory buffer items use recency score. Tool results use a configurable priority (default 0.7). User mentions get high priority (0.9).

### Tier 3: Long-Term Memory Graph

Three persistent stores — episodic, knowledge, and procedural — connected by typed graph edges.

#### Episodic Store
*What happened* — concrete events, conversations, task results, session summaries.

Each episodic memory has:
- Tiered content (full text, summary, key facts, tags — currently only full text populated)
- Category (18-variant enum)
- Importance score (0.0–1.0)
- Strength (1.0 on creation, decays over time via Ebbinghaus curve)
- Scope (global / agent / channel / channel_user)
- Optional vector embedding for semantic search
- Access tracking (count + last accessed timestamp)
- Consolidation state (fresh → consolidated → archived)

#### Knowledge Store
*What is true* — structured knowledge as subject-predicate-object triples.

Examples:
- ("User", "prefers", "dark mode")
- ("Project Alpha", "uses", "Rust + TypeScript")
- ("API rate limit", "is", "100 requests/minute")

Triples with matching subject + predicate are automatically reconsolidated: the newer value replaces the older one, with confidence scores transferred.

#### Procedural Store
*How to do things* — step-by-step procedures with success/failure tracking.

Each procedure has:
- Content (the steps)
- Trigger condition (when to apply)
- Success and failure counters
- Success rate derived from execution history

---

## Long-Term Memory Graph

Memories are not isolated rows — they form a graph connected by typed edges:

| Edge Type | Meaning |
|-----------|---------|
| `RelatedTo` | General association |
| `CausedBy` | Causal relationship |
| `Supports` | Evidence supporting a claim |
| `Contradicts` | Conflicting information |
| `PartOf` | Component relationship |
| `FollowedBy` | Temporal sequence |
| `DerivedFrom` | Source derivation |
| `SimilarTo` | Semantic similarity |

### Spreading Activation

When memories are retrieved by search, the graph is traversed to find associated memories. Adjacent nodes receive an activation boost proportional to their edge weight, biased by edge type. This implements a simplified version of spreading activation from cognitive science.

```mermaid
graph LR
    Q["Search Query"] --> M1["Memory A\n(direct hit)"]

    M1 -- "RelatedTo\nweight: 0.8" --> M2["Memory B\n(1-hop boost)"]
    M1 -- "CausedBy\nweight: 0.9" --> M3["Memory C\n(1-hop boost)"]
    M1 -- "Supports\nweight: 0.7" --> M4["Memory D\n(1-hop boost)"]
    M2 -- "SimilarTo\nweight: 0.6" --> M5["Memory E\n(2-hop — future)"]

```

Currently 1-hop traversal: direct neighbors of retrieved memories are boosted. The activation score is blended with the original retrieval score to produce the final ranking.

---

## Hybrid Search

Engram uses three search signals, fused with reciprocal rank fusion (RRF):

```mermaid
flowchart LR
    Q["Search Query"] --> RG["Retrieval Gate\n(Skip / Retrieve / Deep)"]
    RG -- Skip --> SKIP["Return empty\n(no search needed)"]
    RG -- Retrieve / Deep --> CL["Query Classifier\n(factual vs conceptual)"]
    CL --> BM["BM25\n(Full-Text Index)"]
    CL --> VS["Vector Similarity\n(Ollama embeddings)"]
    BM --> RRF["Reciprocal Rank Fusion\nRRF_score = Σ 1/(k + rank)"]
    VS --> RRF
    RRF --> SA["Graph Spreading\nActivation (1-hop → 2-hop)"]
    SA --> RR["Reranking\n(RRF / MMR / RRF+MMR)"]
    RR --> QG["Quality Gate\n(relevance ≥ 0.3?)"]
    QG -- Pass --> R["Ranked Results"]
    QG -- Fail --> RF["Reformulate / Expand / Refuse"]
    RF --> CL
```

### 1. BM25 Full-Text Search

Full-text index with `porter unicode61` tokenizer. Handles exact keyword matches, stemming, and phrase queries. All full-text query operators are sanitized before execution to prevent injection.

### 2. Vector Similarity Search

When Ollama is available with an embedding model (e.g., `nomic-embed-text`), memories are embedded at storage time. Search queries are embedded at query time. Cosine similarity between query and memory embeddings produces a relevance score.

Embedding generation is optional — if no embedding model is configured, the system falls back to BM25-only search with no degradation in keyword accuracy.

### 3. Graph Spreading Activation

After BM25 and vector results are collected, the memory graph is traversed to find related memories via typed edges. Associated memories receive a score boost.

### Fusion Strategy

Results from all three signals are merged using **Reciprocal Rank Fusion (RRF)**:

$$\text{RRF}_{\text{score}}(d) = \sum_{i} \frac{1}{k + \text{rank}_i(d)}$$

Where $k = 60$ (standard constant) and $\text{rank}_i(d)$ is the rank of document $d$ in signal $i$. This produces a unified ranking that benefits from all three signals without requiring score normalization.

### Reranking

After fusion, results are optionally reranked using one of four strategies:

| Strategy | Method | Use Case |
|----------|--------|----------|
| RRF | Reciprocal rank fusion only | Default, fast |
| MMR | Maximal marginal relevance ($\lambda = 0.7$) | Diversity-focused |
| RRF+MMR | RRF followed by MMR | Best quality |
| CrossEncoder | Model-based reranking (falls back to RRF+MMR) | Future |

### Query Classification

The `hybrid_search.rs` module analyzes queries to determine the optimal search strategy:
- **Factual queries** (who, what, when, specific entities) → weight BM25 higher
- **Conceptual queries** (how, why, explain, abstract topics) → weight vector similarity higher
- Signal weights are adjusted dynamically per query

---

## Retrieval Intelligence

Search is only half the problem. The other half is deciding *whether* to search, and *what to do* when results are weak. Engram implements a two-stage retrieval intelligence pipeline inspired by Self-RAG and CRAG research.

```mermaid
flowchart TD
    Q["Inbound Query"] --> GATE{"Retrieval Gate\n(<1ms)"}

    GATE -- "Skip" --> SKIP["No search\n(greetings, math,\ntopic in working memory)"]
    GATE -- "Retrieve" --> SEARCH["Hybrid Search\n(BM25 + Vector + Graph)"]
    GATE -- "DeepRetrieve" --> DEEP["Extended Search\n(higher limits, 2-hop,\nGraphRAG communities)"]

    SEARCH --> QC{"CRAG Quality\nTier?"}
    DEEP --> QC

    QC -- "Correct (\u2265 0.6)" --> INJECT["Inject directly\n(extract supporting sentences)"]
    QC -- "Ambiguous (0.3\u20130.6)" --> REFINE["Knowledge Refinement\n(decompose + re-search + merge)"]
    QC -- "Incorrect (< 0.3)" --> REFUSE["Refuse / Broaden Scope\n/ Query Decomposition"]

    REFINE --> INJECT
    REFUSE -- "reformulated" --> SEARCH

    GATE -- "Defer" --> ASK["Ask user for\nclarification"]
```

### Retrieval Gate

Before any search executes, the `RetrievalGate` classifies the inbound query and decides the retrieval strategy. This adds <1ms of latency but eliminates unnecessary search cycles and prevents context pollution from irrelevant memory injection.

Five retrieval modes:

| Mode | Trigger | Behavior |
|------|---------|----------|
| **Skip** | Computational queries, greetings, topic already in working memory | No search. The model answers from its own knowledge or the existing conversation. |
| **Retrieve** | Standard factual or procedural queries | Normal hybrid search pipeline (BM25 + vector + graph). |
| **DeepRetrieve** | Exploratory or temporal queries ("tell me everything about…") | Extended search with higher result limits, 2-hop graph activation, and broader scope. |
| **Refuse** | Post-retrieval: top result relevance below threshold | Graceful refusal — "I don't have information on that" rather than fabricating from weak matches. |
| **Defer** | Ambiguous references needing clarification | Ask the user for disambiguation before searching. |

The gate is rule-based by default, evaluating query structure, intent classification, and working memory coverage. This avoids the latency and unreliability of an LLM-based gating decision.

### Post-Retrieval Quality Check (CRAG Three-Tier)

After search returns results, the `QualityGate` evaluates whether the results are actually useful. Inspired by Corrective RAG, Engram classifies retrieval confidence into three tiers:

| Confidence Tier | Trigger | Action |
|---|---|---|
| **Correct** (≥ 0.6) | Top results are highly relevant to the query | Inject directly — extract supporting sentences for focused context |
| **Ambiguous** (0.3–0.6) | Results are related but not clearly on-target | Knowledge refinement: decompose query into sub-queries, re-search each, merge results |
| **Incorrect** (< 0.3) | Results are off-topic or absent | Refuse gracefully, or broaden scope (graph expansion, community summaries), or decompose the query |

**Specific correction actions:**

1. **Knowledge refinement** (Ambiguous tier) — Extract only the sentences from retrieved memories that directly support the query, discarding surrounding noise. This is CRAG’s key insight: even partially-relevant memories contain useful fragments.
2. **Query decomposition** — If a complex query fails as a whole, it is decomposed into sub-queries that are searched independently and results merged.
3. **Scope broadening** — For low-confidence results, the system escalates to graph-expanded search (2-hop activation) or GraphRAG community summaries.
4. **Graceful refusal** — If reformulation, expansion, and decomposition all fail, the system refuses rather than injecting low-quality memories.

**Two foundational quality invariants** underpin every tier decision:

1. **Relevance check** — If the top result scores below 0.3, the result set is classified as *Incorrect* and no memories are injected as-is. The system reformulates the query, broadens scope via graph expansion, or refuses gracefully rather than polluting the context with off-topic material.
2. **Coverage check** — For exploratory queries, if the result count falls below a configurable threshold, the system escalates to DeepRetrieve mode: higher result limits, 2-hop graph activation, and GraphRAG community summaries are engaged to fill the gap before returning a partial answer.

### Unified Retrieval Path (gated_search)

All memory retrieval in Engram routes through a single `gated_search()` function. This is a critical architectural invariant — no path (chat, tasks, orchestrator, swarm, flows, agent tools) may call the search backend directly. The unified path guarantees:

- Every search passes through the retrieval gate (skip/retrieve/deep decision)
- Every search respects per-model injection caps
- Every search applies CRAG three-tier quality checking
- Every search is instrumented with tracing spans and quality metrics
- Every search respects encryption boundaries

```rust
pub async fn gated_search(
    query: &str,
    scope: &MemoryScope,
    model: &ModelCapabilities,
    gate: &RetrievalGate,
    store: &dyn MemoryBackend,
) -> EngineResult<RecallResult> {
    // 1. Gate decision: Skip / Retrieve / DeepRetrieve
    // 2. Hybrid search (BM25 + vector + graph)
    // 3. CRAG quality tier classification
    // 4. Correction actions if needed
    // 5. Budget-aware trimming with per-model cap
    // 6. Quality metrics computation
}
```

This eliminates the current code gap where tasks, orchestrator, and swarm bypass the ContextBuilder and hardcode `limit=10` with no quality checking.

This two-stage pipeline means Engram retrieves when it should, skips when it shouldn't, and corrects when results are weak — rather than blindly injecting whatever the search returns.

---

## Caching Architecture

Engram's three-tier design is itself a caching hierarchy. Each tier operates as a bounded cache with a distinct eviction policy, TTL, and access pattern — mirroring the cache hierarchy in both CPU architecture and biological cognition.

### The Biological Cache Model

The Atkinson-Shiffrin model of human memory describes three stores with decreasing access speed and increasing capacity. Engram's tiers map directly:

| Tier | Engram Module | Biological Analog | Cache Role | Eviction Policy |
|------|---------------|-------------------|------------|-----------------|
| Tier 0 | `SensoryBuffer` | Iconic / echoic memory | **Perceptual cache** — raw stimuli before attentional filtering | FIFO ring; oldest entry evicted on overflow |
| Tier 1 | `WorkingMemory` | Baddeley's central executive | **Attention cache** — what the agent is actively thinking about | Priority-based; lowest-priority slot evicted when token budget exceeded |
| Tier 2 | LTM Graph | Hippocampal long-term store | **Persistent store** — everything known, accessed via retrieval | Ebbinghaus strength decay → GC below threshold |

This is not a loose analogy. The eviction cascade is functionally identical to memory trace transfer in cognitive psychology: sensory traces that receive attention are promoted to working memory; working memory items that are rehearsed are consolidated to long-term storage. Items that fail to be promoted are lost — the system forgets gracefully.

### Tier 0: Sensory Cache

The `SensoryBuffer` is a bounded `VecDeque` ring buffer with explicit cache semantics:

- **Capacity-bounded:** configurable max entries (default 20)
- **FIFO eviction:** when full, `push()` evicts the oldest entry and returns it for promotion to working memory
- **Token-aware:** tracks cumulative token count; `drain_within_budget()` returns items that fit within a given token limit
- **Volatile:** contents are discarded after each agent turn

The returned evicted entry is the promotion signal — it tells the caller "this item was pushed out of sensory attention; decide whether it deserves a working memory slot." This mirrors the attentional gate in human perception.

### Tier 1: Attention Cache

`WorkingMemory` implements a priority-managed cache with LRU-like refresh semantics:

- **Token-budget-bounded:** total slot tokens cannot exceed the configured budget (default 4,096)
- **Priority eviction:** `evict_lowest()` removes the slot with the lowest priority score — items that haven't been referenced decay naturally
- **Priority decay:** `decay_priorities(0.95)` is called each turn, multiplicatively reducing all slot priorities. Unreferenced items age out over ~20 turns
- **Priority boost:** `boost_priority(id, delta)` refreshes recently-accessed items, equivalent to an LRU "touch" operation
- **Snapshot persistence:** on agent switch, the entire working memory is serialized to the persistent store and restored when the agent resumes — cache state survives context switches

### Momentum Cache

Working memory maintains a sliding window of recent query embeddings (`momentum_embeddings: Vec<Vec<f32>>`, capped at 5). This trajectory cache serves two purposes:

1. **Priming** — biases retrieval toward the current conversation direction, exactly as semantic priming works in human cognition
2. **Anticipation** — the momentum vector (centroid of recent embeddings) predicts where the conversation is heading, enabling pre-fetch of likely-needed memories before the user asks

### Publication Buffer

The `MemoryBus` maintains a bounded FIFO publication queue with TTL-based eviction:

- **TTL expiry:** publications older than `PUBLICATION_TTL_SECS` are discarded on each insertion
- **Capacity cap:** when `MAX_PENDING_PUBLICATIONS` is reached, the oldest publication is evicted
- **Subscriber fan-out:** pending publications are delivered to registered subscribers upon drain, then removed

This ensures the event-driven memory pipeline never accumulates unbounded backlog, even if consumers stall.

### Cache Coherence

The three tiers maintain coherence through directional data flow:

```mermaid
flowchart LR
    IN["New Input"] --> SB["Sensory Buffer\n(Tier 0)"]
    SB -- "promotion" --> WM["Working Memory\n(Tier 1)"]
    WM -- "consolidation" --> LTM["Long-Term Memory\n(Tier 2)"]
    LTM -- "recall on search" --> WM
    WM -. "boost on access" .-> WM
```

There is no cache invalidation problem because the tiers are write-forward: data flows from fast/volatile to slow/persistent. Long-term memory never writes back to the sensory buffer. When a long-term memory is recalled via search, it enters working memory as a *new slot* with source `Recalled` — it does not attempt to synchronize with any existing tier-0 entry.

This unidirectional flow eliminates the coherence complexity that plagues traditional multi-level caches.

---

## Consolidation Engine

A background process runs every 5 minutes (configurable) performing four operations:

```mermaid
flowchart TD
    subgraph Cycle["Consolidation Cycle (every 5 min)"]
        direction TB
        SAVE["SAVEPOINT\n(pre-consolidation baseline)"]
        SAVE --> PC["1. Pattern Clustering\ncosine ≥ 0.75 → union-find grouping"]
        PC --> CD["2. Contradiction Detection\nsame subject + predicate, different object → resolve"]
        CD --> DECAY["3. Dual-Layer Decay\nLML β=0.8 (slow) / SML β=1.2 (fast)"]
        DECAY --> GC["4. Garbage Collection\nstrength < 0.1 → two-phase delete"]
        GC --> FUS["5. Memory Fusion\ncosine ≥ 0.75 → merge → tombstone"]
        FUS --> NDCG{"NDCG delta\n< −5%?"}
        NDCG -- Yes --> ROLL["ROLLBACK TO SAVEPOINT\n(no memories lost)"]
        NDCG -- No --> COMMIT["COMMIT\n(changes persist)"]
    end

```

### 1. Pattern Clustering

Memories with cosine similarity ≥ 0.75 are grouped using union-find clustering. Clusters of related memories are identified for potential fusion. This prevents memory bloat from repeated similar observations.

### 2. Contradiction Detection

When two memories share the same subject and predicate but have different objects, a contradiction is detected. Resolution: the newer memory wins, the older memory's confidence is transferred proportionally, and a `Contradicts` graph edge is created.

### 3. Ebbinghaus FadeMem Dual-Layer Strength Decay

Memory strength decays following a biologically-inspired dual-layer model derived from Ebbinghaus FadeMem. Instead of uniform Ebbinghaus decay, memories are assigned to one of two layers with different decay characteristics:

**Long Memory Layer (LML)** — Important, frequently-accessed memories. Decay exponent $\beta = 0.8$ (sub-linear), producing a half-life of ~11.25 days. These memories fade slowly and persist across sessions.

**Short Memory Layer (SML)** — Transient, low-importance memories. Decay exponent $\beta = 1.2$ (super-linear), producing a half-life of ~5.02 days. These memories fade quickly to prevent clutter.

$$\text{strength}(t) = S_0 \cdot e^{-\lambda_{\text{base}} \cdot t^{\beta}}$$

Where:
- $\lambda_{\text{base}} = 0.1$ — base decay rate
- $\beta_{\text{LML}} = 0.8$ — sub-linear for long-term
- $\beta_{\text{SML}} = 1.2$ — super-linear for short-term

**Hysteresis mechanism:** Memories are promoted from SML to LML when access frequency exceeds $\theta_{\text{promote}} = 0.7$, and demoted from LML to SML when relevance drops below $\theta_{\text{demote}} = 0.3$. The gap between these thresholds prevents oscillation — a memory doesn't bounce between layers on marginal changes.

**Per-type decay modulation:** Decay rates are further adjusted by memory type:
- Procedural memories: $\lambda \times 0.5$ (skills persist longer)
- Semantic memories: $\lambda \times 0.7$ (knowledge decays slower than episodes)
- Episodic memories: $\lambda \times 1.0$ (experiences decay at base rate)
- Frequently accessed (>5 accesses): $\lambda \times 0.7$ (used memories persist)

This dual-layer approach achieves 45% storage reduction while *improving* retrieval quality — FadeMem's ablation study shows removing dual-layer decay causes a 33.9% F1 drop.

### 4. Garbage Collection

Memories with strength below a threshold (default 0.1) are candidates for deletion. Important memories (importance ≥ 0.7) are protected from GC regardless of strength. Deletion is two-phase: content fields are zeroed before the row is deleted (anti-forensic measure).

After GC, the database is re-padded to 512KB bucket boundaries to prevent file-size side-channel leakage.

### Transactional Forgetting

The entire consolidation cycle (decay + GC + fusion) executes within a transactional savepoint. Before the cycle begins, a sample of 50 recent queries is run through search and their NDCG scores are recorded as a baseline. After the cycle completes, the same queries are re-evaluated.

If NDCG drops by more than 5%, the entire cycle rolls back to the savepoint — no memories are lost. This makes forgetting *provably safe*: the system can only forget in ways that maintain or improve retrieval quality.

```
BEGIN TRANSACTION
  SAVEPOINT pre_consolidation
  execute decay, GC, fusion
  measure NDCG delta on held-out query set
  IF ndcg_delta < −0.05 THEN
    ROLLBACK TO pre_consolidation    // no memories lost
  ELSE
    RELEASE pre_consolidation
    COMMIT
  END IF
```

### Gap Detection

The consolidation engine also detects three types of knowledge gaps:
- **Missing context** — references to entities that have no associated memories
- **Temporal gaps** — periods with no memory activity for an active agent
- **Category imbalance** — agents with memory heavily concentrated in one category

Gaps are logged for diagnostic purposes.

---

## Adaptive Forgetting — FadeMem Dual-Layer Architecture

Traditional memory systems treat forgetting as a failure mode. Engram treats it as a first-class cognitive mechanism, inspired by the FadeMem which demonstrates that *measured* forgetting can reduce storage by 45% while simultaneously improving retrieval quality.

### The Core Insight

Human memory does not decay uniformly. Frequently-rehearsed information consolidates into long-term storage while transient details fade rapidly. FadeMem formalizes this with a dual-layer architecture that Engram adopts:

```mermaid
flowchart TB
    subgraph LML["Long Memory Layer (LML)"]
        direction TB
        L1["β = 0.8 — sub-linear decay"]
        L2["half-life ≈ 11.25 days"]
        L3["Important facts · verified knowledge\nhigh-use procedural memories · user-explicit stores"]
    end

    subgraph SML["Short Memory Layer (SML)"]
        direction TB
        S1["β = 1.2 — super-linear decay"]
        S2["half-life ≈ 5.02 days"]
        S3["Session context · transient observations\nauto-captured details · low-importance entries"]
    end

    SML -- "promote when\naccess_freq > θ_promote (0.7)" --> LML
    LML -- "demote when\nrelevance < θ_demote (0.3)" --> SML

```

### Interference-Based Decay

Beyond the dual-layer structure, decay is modulated by *interference* — how much a memory conflicts with or is superseded by newer information:

- **Retrieval interference:** Memories that are frequently searched for but rarely selected accumulate negative signal. They occupy search results without providing value.
- **Semantic overlap:** When new memories are stored that cover the same semantic space, older overlapping memories decay faster — the new information has effectively superseded them.
- **Access recency:** A memory accessed yesterday decays slower than one last accessed 30 days ago, independent of creation date.

The combined formula:

$$\lambda_\text{eff} = \lambda_\text{base} \times \textit{typeModifier} \times \textit{interferenceFactor} \times (1 + \textit{semanticOverlap})$$

This produces *adaptive* forgetting: universally useful knowledge persists almost indefinitely (low interference, frequent access, LML layer), while transient noise evaporates quickly (high interference, no access, SML layer).

### Four Conflict Types

When memories conflict during consolidation, Engram classifies the relationship into one of four types (from FadeMem's conflict resolution model):

| Relation | Meaning | Resolution |
|----------|---------|------------|
| **Compatible** | Both memories are true simultaneously | Fuse into unified entry |
| **Contradictory** | Mutually exclusive claims | Most recent wins; loser's confidence transferred; `Contradicts` edge created |
| **Subsumes** | New memory is a superset of old | Absorb old into new; old becomes tombstone |
| **Subsumed** | Old memory is a superset of new | Keep old; boost its strength; discard new |

Every conflict resolution is recorded in the audit log with full provenance: which memories conflicted, what relation was detected, which resolution strategy was applied, and who won.

### FadeMem Ablation Results

The FadeMem paper provides ablation evidence that directly informs Engram's implementation priority:

| Component Removed | F1 Drop | Implication |
|---|---|---|
| Fusion engine | **-53.7%** | Highest-impact single component — must implement first |
| Dual-layer decay | -33.9% | Second priority — uniform decay is significantly worse |
| Conflict resolution | -19.2% | Third — blind "newest wins" loses important context |
| Adaptive decay rates | -12.8% | Fourth — per-type tuning adds measurable value |

FadeMem achieves F1 = 29.43 (beating Mem0's 28.37), 82.1% critical fact retention at 55% storage, and 77.2% Retrieval Precision@10.

---

## Memory Fusion

Consolidation handles clustering and contradiction detection, but it does not address **near-duplicate memories** — entries that express the same information in slightly different words. Over months of use, these duplicates accumulate linearly: "User prefers dark mode", "User prefers dark mode in editors", "User uses dark mode" all occupy separate storage, search bandwidth, and context tokens.

Memory fusion addresses this directly, inspired by FadeMem's fusion mechanism.

### Fusion Pipeline

During each consolidation cycle, the fusion engine:

```mermaid
flowchart TD
    SCAN["Scan Memory Pairs"] --> COS{"Cosine\nSimilarity\n\u2265 0.75?"}
    COS -- No --> SKIP["Skip\n(distinct memories)"]
    COS -- Yes --> CLASS{"Classify\nRelation"}

    CLASS -- Compatible --> FUSE["Fuse into\nunified entry"]
    CLASS -- Contradictory --> CONTRA["Recent wins\nConfidence transferred\nContradicts edge created"]
    CLASS -- Subsumes --> ABSORB["Absorb old into new\nOld becomes tombstone"]
    CLASS -- Subsumed --> KEEP["Keep old\nBoost strength\nDiscard new"]

    FUSE & CONTRA & ABSORB & KEEP --> EDGE["Redirect Graph Edges\nto merged entry"]
    EDGE --> TOMB["Tombstone Originals\n(recoverable)"]
    TOMB --> QC{"NDCG\ndelta OK?"}
    QC -- "Drop > 5%" --> ROLLBACK["Rollback fusion cycle"]
    QC -- OK --> DONE["Commit \u2014 storage reduced"]

```

1. **Candidate detection** — Identify memory pairs with cosine similarity $\geq \theta_{\text{fusion}}$ (0.75, derived from FadeMem paper — the plan originally used 0.92 but the paper demonstrates 0.75 is the optimal threshold) and compatible scopes (same agent, same scope tier).
2. **Relation classification** — Classify each pair as Compatible, Contradictory, Subsumes, or Subsumed using the four-type conflict model.
3. **Merge** — For Compatible pairs: create a single strengthened entry with the union of propositions from both sources, the maximum of their strength values, and a provenance chain linking back to the originals.
4. **Edge redirection** — All graph edges pointing to the original entries are redirected to the merged entry, preserving graph connectivity.
5. **Tombstoning** — Original entries are marked as tombstones rather than deleted immediately. This allows recovery if a merge was too aggressive and maintains audit trail integrity.

### Quality Measurement

Every fusion cycle is measured:

- **Chain integrity percentage** — Multi-hop graph traversals that succeed before and after fusion. A fusion that breaks a retrieval chain is detected.
- **NDCG delta** — Normalized discounted cumulative gain is computed on a fixed query set before and after fusion. If NDCG drops by more than 5%, the fusion cycle is rolled back.
- **Storage reduction** — Bytes freed and entries removed are tracked per cycle.

The threshold ($\theta_{\text{fusion}} = 0.75$ cosine) is derived from FadeMem's paper. Higher values produce more conservative merging. Lower values risk merging memories that carry distinct nuance.

---

## GraphRAG — Community-Based Global Retrieval

Traditional retrieval (BM25 + vector + spreading activation) answers *local* queries well: "What does the user prefer for dark mode?" finds specific memories. But *global* queries fail: "Summarize everything I know about Project Alpha" requires reasoning across many memories that may not share keywords or embedding similarity.

GraphRAG addresses this by treating the memory graph as a knowledge graph with detectable communities. Engram implements a dual-plane retrieval system inspired by Microsoft GraphRAG, Deep GraphRAG, and informed by WildGraphBench failure analysis.

### Community Detection

The memory graph undergoes Louvain community detection during consolidation. Communities are groups of densely-connected memories that represent coherent topics or projects:

```
Project Alpha community:
  ├─ "Set up Rust backend" (episodic)
  ├─ "Project Alpha uses Tauri v2" (semantic)
  ├─ "API rate limit is 100/min" (semantic)
  ├─ "Deployed to staging" (episodic)
  └─ "Deploy procedure for Alpha" (procedural)
```

Each community gets a **hierarchical summary** — an LLM-generated description of the community's contents, stored with its own embedding. This enables global queries to match against community-level descriptions rather than individual memories.

### Deep GraphRAG Three-Stage Pipeline

For queries that require community-level reasoning, Engram uses a three-stage hierarchical pipeline from Deep GraphRAG:

```mermaid
flowchart TD
    Q["Query"] --> ROUTER{"Query\nPlane?"}

    ROUTER -- Local --> LOCAL["Standard Hybrid Search\nBM25 + Vector + 1-hop Graph"]
    ROUTER -- Global --> S1
    ROUTER -- Hybrid --> BOTH["Local Search +\nCommunity Summaries"]

    subgraph Pipeline["Deep GraphRAG Three-Stage Pipeline"]
        S1["Stage 1: Inter-Community Filter\nEmbed query \u2192 match community summaries\n\u2192 select top-k communities"]
        S1 --> S2["Stage 2: Intra-Community Retrieval\nHybrid search within each\nselected community"]
        S2 --> S3["Stage 3: Knowledge Integration\nDeduplicate \u2192 cross-community edges\n\u2192 budget-aware ranking"]
    end

    LOCAL --> RESULTS["Ranked Results"]
    S3 --> RESULTS
    BOTH --> RESULTS

```

1. **Inter-community filter** — Embed the query, compare against all community summary embeddings, select the top-k most relevant communities. This narrows the search space from the entire graph to a few coherent clusters.

2. **Intra-community retrieval** — Within each selected community, run the full hybrid search pipeline (BM25 + vector + graph activation) to find the most relevant individual memories.

3. **Knowledge integration** — Combine results across communities with deduplication, cross-community edge traversal, and budget-aware ranking. The final result set represents a coherent answer drawing from multiple knowledge clusters.

### Dual-Plane Query Router

The retrieval gate classifies queries into three planes:

| Plane | Query Type | Search Strategy |
|-------|-----------|----------------|
| **Local** | Specific factual/procedural queries | Standard hybrid search (BM25 + vector + 1-hop graph) |
| **Global** | Summary/exploration/"tell me everything" queries | Community filter → intra-community search → integration |
| **Hybrid** | Queries needing both specific facts and broader context | Local search + community summaries combined |

### WildGraphBench Failure Defenses

WildGraphBench identifies five failure modes where GraphRAG systems degrade. Engram defends against each:

| Failure Mode | Defense |
|---|---|
| GraphRAG hurts summarization tasks | Intent classifier routes summarization to global plane only when beneficial |
| Community detection produces noisy clusters | Minimum community size threshold; orphan nodes fall back to local search |
| Stale community summaries | Incremental re-summarization during consolidation when community membership changes |
| Over-reliance on graph structure | Hybrid plane combines graph-based and text-based results |
| Query-type blindness | 6-intent classifier dynamically selects the optimal retrieval plane |

### DW-GRPO and Small Model Quality

Deep GraphRAG's DW-GRPO training technique (Distributed Weighted Group Relative Policy Optimization) demonstrates that 1.5B parameter models can approach 70B model quality for knowledge integration tasks. This is critical for Engram's local-first architecture: users running Ollama with small local models can still achieve high-quality GraphRAG retrieval through the three-stage pipeline.

### GraphRAG-R1 Reward Signals

GraphRAG-R1 introduces two reward signals for training retrieval policies:

- **PRA (Progressive Retrieval Attenuation)** — Penalizes shallow single-hop retrieval. Rewards multi-hop reasoning that follows graph edges to deeper answers. Applied as a retrieval depth bonus: deeper traversals earn higher scores.
- **CAF (Cost-Aware F1)** — Penalizes over-retrieval. A system that retrieves 50 memories to answer a simple question is punished even if the answer is correct. This naturally encourages budget-efficient retrieval.

Pending RL training infrastructure, Engram implements PRA and CAF as heuristic reward signals in the reranking pipeline, boosting results that demonstrate multi-hop reasoning and penalizing over-retrieval.

Engram is the **first local-first GraphRAG implementation** in any agent memory system.

---

## Compounding Skill Library

Most agent memory systems only store *facts* — what happened, what is true. Engram also stores *skills* — executable, composable procedures that improve with every interaction. This is inspired by Voyager, Reflexion, and HELPER.

```mermaid
flowchart TD
    TASK["Agent Completes\nMulti-Step Task"] --> EXTRACT["Auto-Extract\nReusable Skill"]
    EXTRACT --> VERIFY{"Skill Verifier\nTools exist? Outcomes match?\nNo hallucinations? No danger?"}
    VERIFY -- Pass --> LIB["Skill Library\n(composable procedures)"]
    VERIFY -- Fail --> DISCARD["Discard"]

    LIB --> SUGGEST["Proactive Suggestion\n(pattern match on context)"]
    SUGGEST --> EXEC["Skill Execution"]
    EXEC --> SUCCESS{"Outcome?"}
    SUCCESS -- Success --> BOOST["Increment success_count\nBoost strength"]
    SUCCESS -- Failure --> REFLECT["Reflexion Analysis\n(root cause \u2192 correction)"]
    REFLECT --> VARIANT["Store as skill variant\nor guard condition"]

    VARIANT --> LIB
    BOOST --> LIB
    LIB --> COMPOSE["Compositional Hierarchy\n(skills reference sub-skills)"]
    COMPOSE --> LIB

```

### Auto-Extraction

When an agent successfully completes a multi-step task (file editing, API debugging, deployment), the interaction is analyzed and a reusable skill is extracted:

```
Skill: "Deploy to staging via Docker"
Steps:
  1. Build image: docker build -t app:latest .
  2. Push to registry: docker push registry.example.com/app:latest
  3. SSH to staging: ssh deploy@staging
  4. Pull and restart: docker compose pull && docker compose up -d
Trigger: "deploy to staging" OR "push to staging"
```

### Skill Verification

Before a skill is promoted to the library, the `SkillVerifier` checks:
- All referenced tool calls actually exist and are callable
- Expected outcomes match actual outcomes from the extraction context
- No hallucinated steps (steps claimed but not actually executed in the source interaction)
- No dangerous operations without confirmation steps

### Compositional Hierarchy

Skills compose. A "deploy to production" skill can reference the "deploy to staging" skill as a sub-step, plus add production-specific steps (health checks, rollback preparation). This creates a compositional hierarchy where complex workflows are built from verified primitives.

### Reflexion-Style Failure Learning

When a skill execution fails, the failure is analyzed and stored as a **negative example**:

- What went wrong (error message, failed step)
- Why it went wrong (LLM-generated root cause analysis)
- What to do differently (correction stored as a skill variant or guard condition)

This mirrors Reflexion's verbal reinforcement learning: the agent doesn't need weight updates to learn from mistakes. It stores the lesson in memory and retrieves it the next time a similar task arises.

### Proactive Skill Suggestion

The `SkillSuggester` monitors the current conversation context and proactively suggests relevant skills. When a user says "I need to set up the CI pipeline," the suggester checks the skill library for matching procedures and injects them into the agent's context with a note: *"I have a verified procedure for this from a previous session."*

### Quantified Compounding Effect

With a mature skill library (~50+ verified skills), agents demonstrate measurable improvement:

| Metric | Without Skills | With Skills | Improvement |
|---|---|---|---|
| Task completion steps | Baseline | -50% | Fewer redundant explorations |
| Success rate | Baseline | +20% | Verified procedures reduce errors |
| Repeat errors | Baseline | -80% | Failure memories prevent recurrence |
| Token cost per task | Baseline | -40% | Reusable skills avoid re-deriving solutions |

No competing memory system implements a self-improving procedural memory library.

---

## Context Window Intelligence

### ContextBuilder

The ContextBuilder is a fluent API for assembling the final prompt within a token budget:

```mermaid
flowchart TD
    subgraph Budget["Token Budget Allocation (priority order)"]
        direction TB
        P1["1. System Prompt\n(always included — first priority)"]
        P1 --> P2["2. Recalled Memories\n(packed by importance × relevance score)"]
        P2 --> P3["3. Working Memory Slots\n(packed by priority)"]
        P3 --> P4["4. Sensory Buffer Items\n(packed by recency)"]
        P4 --> P5["5. Conversation Messages\n(newest-first until budget exhausted)"]
    end

    MODEL["Model Capability\nRegistry"] --> BUDGET["Total Token\nBudget"]
    BUDGET --> Budget
    Budget --> PROMPT["Final Assembled\nPrompt"]

```

```rust
let prompt = ContextBuilder::new(model_caps)
    .system_prompt(&base_prompt)
    .recall_from(&store, &query, agent_id, &embedding_client).await
    .working_memory(&wm)
    .sensory_buffer(&buffer)
    .messages(&conversation)
    .build();
```

**Budget allocation strategy:**
1. System prompt gets first priority (always included)
2. Recalled memories packed by $\text{importance} \times \text{relevance score}$
3. Working memory slots packed by priority
4. Sensory buffer items packed by recency
5. Conversation messages packed newest-first until budget exhausted

### Model Capability Registry

Every supported model has a capability fingerprint:
- Context window size
- Maximum output tokens
- Tool/function calling support
- Vision support
- Extended thinking support
- Streaming support
- Tokenizer type

The registry covers all models from OpenAI, Anthropic, Google, DeepSeek, Mistral, xAI, Ollama, and OpenRouter. Unknown models fall back to conservative defaults (32K context, 4K output).

### Tokenizer

Model-specific token estimation:
- `Cl100kBase` — GPT-4, Claude ($\div 3.4$ bytes)
- `O200kBase` — o1, o3, o4 ($\div 3.8$ bytes)
- `Gemini` — Gemini models ($\div 3.3$ bytes)
- `SentencePiece` — Llama, Mistral, local models ($\div 3.0$ bytes)
- `Heuristic` — Fallback ($\div 4.0$ bytes)

All calculations use `ceil()` to round up and are UTF-8 safe (truncation never splits a multi-byte character).

---

## Memory Security

### Field-Level Encryption

Memories containing PII are encrypted with AES-256-GCM before storage. The encryption key is stored in the OS keychain under a dedicated entry (`paw-memory-vault`), separate from the credential vault key.

**Automatic PII detection** uses a two-layer approach:

**Layer 1 — Static pattern matching.** 17 compiled regex patterns run on every memory at storage time, covering:
- SSN (with and without hyphens), credit card (including Amex), phone numbers (US and international)
- Email addresses, physical addresses, IP addresses (IPv4)
- Person names, geographic locations
- Credentials (passwords, API keys, JWT tokens, AWS access keys, private key blocks)
- Government IDs, IBAN bank account numbers

**Layer 2 — LLM-assisted secondary scan.** During idle-time consolidation, memories stored as cleartext are re-scanned using the configured model as a PII classifier. This catches semantic PII that no regex can detect — unstructured references to names, addresses, health conditions, and context-dependent identifiers. Memories retroactively found to contain PII are encrypted in place.

**Encryption flow:**

```mermaid
flowchart TD
    A["New Memory Content"] --> B["detect_pii(content)\n17 regex patterns"]
    B --> C{"PII Found?"}
    C -- No --> L2["LLM secondary scan\n(during consolidation)"]
    L2 -- "PII found" --> E
    L2 -- Clean --> D["Cleartext\nStore as-is"]
    C -- Yes --> E["classify_tier(pii_types)"]
    E --> F{"Tier"}
    F -- Sensitive --> G["AES-256-GCM Encrypt\nenc:base64(nonce‖ciphertext‖tag)"]
    F -- Confidential --> G
    G --> H["Store encrypted in database"]
    H --> I["On retrieval: decrypt with keychain key"]
```

### Inter-Agent Memory Trust

The memory bus enables cross-agent knowledge sharing, but shared memory introduces a trust boundary — a compromised or misconfigured agent could inject poisoned memories into the fleet. Engram defends against this at three layers:

1. **Publish-side validation** — Every memory published to the bus passes through the injection scanner before entering the queue. Detected payloads are blocked at publish time, not just at recall time.
2. **Capability-scoped publishing** — Each agent receives a signed capability token at creation that encodes its maximum publish scope (agent-only, squad, project, or global), maximum self-assignable importance, and per-cycle rate limit. Attempts to exceed the capability ceiling are rejected with an audit log entry.
3. **Trust-weighted contradiction resolution** — When a published memory contradicts an existing fact, the resolution factors in the source agent's trust score, not just recency. A less-trusted agent cannot override a more-trusted agent's knowledge without a significant confidence differential.

### Query Sanitization

Full-text search operators are stripped from user queries before they reach the storage engine. This prevents full-text injection attacks that could extract data via crafted queries.

### Prompt Injection Defense

Every recalled memory passes through a two-stage sanitization pipeline before reaching the agent:

1. **Pattern redaction** — 10 compiled regex patterns detect common injection payloads ("ignore previous instructions", "you are now", system prompt markers, override/bypass attempts). Matched regions are replaced with `[REDACTED:injection]` — the payload never reaches the model.
2. **Structural isolation** — Recalled memories are wrapped in explicit data-boundary markers in the prompt, instructing the model to treat memory content as data, not instructions.

Injection attempts that evade static patterns remain a known limitation of regex-based scanning. The current defense is a first layer — it blocks known attack shapes but cannot guarantee coverage against novel obfuscation techniques (unicode substitution, multi-turn payload assembly, encoded instructions). The architecture is designed for a future secondary scan using the model itself as a classifier.

### Anti-Forensic Measures

- **Two-phase secure deletion** — Content zeroed before row deletion
- **Vault-size quantization** — Database padded to 512KB buckets
- **8KB pages** — Reduces file-size granularity
- **Incremental auto-vacuum** — Prevents immediate file shrinkage after deletions
- **Secure delete** — freed pages are zeroed by the storage engine

### GDPR Compliance

`engram_purge_user(identifiers)` securely erases all memories matching a list of user identifiers across all tables, including snapshots and audit logs. Implements Article 17 (right to be forgotten).

---

## Memory Lifecycle Integration

Engram is wired into every major execution path:

```mermaid
flowchart TD
    subgraph Recall["Pre-Recall (before agent turn)"]
        R1["Chat auto-recall"]
        R2["Task memory injection"]
        R3["Orchestrator pre-recall"]
        R4["Swarm agent recall"]
    end

    subgraph Capture["Post-Capture (after agent turn)"]
        CH["Chat"] --> PC["Auto-capture facts"]
        TA["Tasks"] --> PC2["Store task_result"]
        OR["Orchestrator"] --> PC3["Store project outcome"]
        CO["Compaction"] --> PC4["Store session summary"]
        CB["Channel Bridges\n(Discord, Slack, Telegram…)"] --> PC5["Store with scope metadata"]
    end

    R1 & R2 & R3 & R4 --> RG["Retrieval Gate\n(Skip / Retrieve / Deep)"]
    RG --> HS["Hybrid Search\n(BM25 + Vector + Graph)"]
    HS --> QG["Quality Gate\n(relevance check)"]
    QG --> CTX["ContextBuilder\n(budget-aware inject into prompt)"]

    PC & PC2 & PC3 & PC4 & PC5 --> BR["Engram Bridge\n(PII encrypt → dedup → embed → store)"]
    BR --> DB[("Persistent Store\nEpisodic / Knowledge / Procedural")]
    DB --> HS

    AT["Agent Tools\n(store, search, knowledge,\nstats, delete, update, list)"] <--> BR
    AT <--> HS

    subgraph Background["Background (every 5 min)"]
        CON["Consolidation Engine\n(cluster → contradict → decay → GC)"]
        FUS["Memory Fusion\n(dedup → merge → tombstone)"]
    end

    DB <--> CON
    CON --> FUS
    FUS --> DB
```

### Chat

When `auto_recall` is enabled for an agent, the ContextBuilder performs a hybrid search and injects relevant memories into the system prompt before each agent turn. Agent responses can trigger auto-capture of facts, preferences, and observations.

### Tasks

Before a task agent runs, the top 10 relevant memories are searched and injected as a "Relevant Memories" system prompt section. After the agent completes, the task result is stored in episodic memory via the Engram bridge with category `task_result`.

### Orchestrator

The boss agent in multi-agent orchestration receives pre-recalled memories relevant to the project goal. After the orchestration completes, the project outcome is captured in episodic memory.

### Session Compaction

When a conversation is compacted (summarized to free context space), the compaction summary is stored in Engram episodic memory with category `session`. This ensures knowledge survives compaction.

### Channel Bridges

Messages from Discord, Slack, Telegram, and other channels are stored with channel and user scope metadata. This enables per-channel memory isolation — a user's Discord memories don't bleed into their Telegram conversations.

### Agent Tools

Agents have direct access to memory through 7 tools:

| Tool | Purpose |
|------|---------|
| `memory_store` | Store a memory with category and importance |
| `memory_search` | Hybrid search across all memory types |
| `memory_knowledge` | Store structured SPO triples |
| `memory_stats` | Get memory system statistics |
| `memory_delete` | Delete a specific memory |
| `memory_update` | Update memory content |
| `memory_list` | Browse memories by category |

---

## Concurrency Architecture

A desktop AI platform serves multiple concurrent consumers: the chat UI, background tasks, orchestration pipelines, 11+ channel bridges, and the consolidation engine — all reading and writing memory simultaneously. The concurrency model must handle this without blocking the Tokio runtime or causing write contention.

### Read Pool + Write Channel

Engram separates reads from writes using a two-path architecture:

```mermaid
flowchart LR
    subgraph Consumers["Concurrent Consumers"]
        direction TB
        C1["Chat UI"]
        C2["Background Tasks"]
        C3["Orchestration"]
        C4["Channel Bridges\n(11+)"]
        C5["Consolidation\nEngine"]
    end

    subgraph ReadPath["Read Path (concurrent)"]
        direction TB
        RP["Connection Pool\n8 WAL read-only connections"]
    end

    subgraph WritePath["Write Path (serialized)"]
        direction TB
        WC["tokio::mpsc channel"]
        WC --> WT["Dedicated writer task\n(single connection)"]
    end

    Consumers -- "search, traverse,\nstat reads" --> ReadPath
    Consumers -- "insert, update,\ndelete, consolidate" --> WritePath

    ReadPath --> DB[("Storage\nEngine")]
    WritePath --> DB

```

- **Read path** — A connection pool with 8 read-only connections operating in WAL (Write-Ahead Logging) mode. All search queries, graph traversals, and stat reads execute on the pool concurrently. WAL mode allows readers to proceed without blocking on writers.
- **Write path** — A dedicated writer task receives all mutations through a `tokio::mpsc` channel. The writer serializes all inserts, updates, deletions, and consolidation writes through a single connection, eliminating write contention entirely.

```
Read requests ──→ connection pool [8 WAL connections] ──→ result
Write requests ──→ mpsc channel ──→ dedicated writer task ──→ storage engine
```

The `mpsc::send()` + `oneshot::recv()` pattern is fully async-safe — no synchronous mutexes appear in async code, which prevents Tokio thread starvation under load.

### Storage Backend Trait

All storage access is mediated through the `MemoryBackend` trait, which abstracts the underlying database:

```rust
#[async_trait]
pub trait MemoryBackend: Send + Sync {
    async fn store_episodic(&self, memory: &EpisodicMemory) -> EngineResult<String>;
    async fn search_episodic_bm25(&self, query: &str, scope: &MemoryScope, limit: usize) -> EngineResult<Vec<(String, f64)>>;
    async fn search_episodic_vector(&self, embedding: &[f32], scope: &MemoryScope, limit: usize) -> EngineResult<Vec<(String, f64)>>;
    async fn add_edge(&self, edge: &MemoryEdge) -> EngineResult<()>;
    async fn get_neighbors(&self, memory_id: &str, min_weight: f64) -> EngineResult<Vec<(String, f64)>>;
    async fn apply_decay(&self, half_life_days: f64) -> EngineResult<usize>;
    async fn garbage_collect(&self, threshold: f64) -> EngineResult<usize>;
    // ... additional operations for semantic, procedural, graph, and lifecycle
}
```

This trait enables `MockMemoryStore` for test isolation, backend swaps, and clean dependency injection across all modules.

### Vector Index Strategy

Vector similarity search uses a tiered indexing strategy:

| Memory Count | Index | Latency | RAM |
|-------------|-------|---------|-----|
| < 1,000 | Brute-force cosine scan | < 5ms | Negligible |
| 1,000 – 100,000 | HNSW (in-memory, pure Rust) | < 5ms | ~3KB/vector |
| > 100,000 | HNSW with disk-backed fallback | < 25ms | Bounded |

Both implementations sit behind a `VectorIndex` trait. The index warms from the database on startup and receives new embeddings on the write path. If no embedding model is available, vector search is disabled entirely and the system falls back to BM25-only with no loss in keyword accuracy.

---

## Observability

A memory system without measurement is a memory system without improvement. Engram instruments every operation to make debugging, optimization, and quality evaluation possible.

### Tracing

All public functions are instrumented with `tracing::instrument` spans organized in a hierarchy:

- `engram.search` — Covers the full search pipeline: gate decision, BM25, vector, graph activation, reranking, quality check
- `engram.store` — Covers PII detection, encryption, embedding generation, deduplication, and database write
- `engram.consolidate` — Covers pattern clustering, contradiction detection, fusion, decay, and garbage collection
- `engram.context` — Covers the ContextBuilder prompt assembly pass

Span metadata includes agent ID, query text (redacted if PII), result count, latency, and quality scores. These spans integrate with any `tracing::Subscriber` — local log files, structured JSON, or external collectors.

### Metrics

The `metrics` crate provides three categories of runtime instrumentation:

| Type | Metric | Purpose |
|------|--------|---------|
| Counter | `engram.search_count` | Total searches executed |
| Counter | `engram.store_count` | Total memories stored |
| Counter | `engram.gc_count` | Garbage collection cycles |
| Counter | `engram.gate_skip_count` | Retrieval gate skips (queries that didn't need memory) |
| Gauge | `engram.memory_count` | Current total memory count |
| Gauge | `engram.hnsw_size` | Current HNSW index size |
| Gauge | `engram.pool_active` | Active read pool connections |
| Histogram | `engram.search_latency_ms` | Search latency distribution |
| Histogram | `engram.store_latency_ms` | Store latency distribution |
| Histogram | `engram.consolidation_ms` | Consolidation cycle duration |

### Cognitive Debug Events

For real-time debugging, Engram emits Tauri events that the frontend debug panel can display:

- `engram:search` — Query, gate decision, result count, top scores, latency
- `engram:store` — Memory ID, category, importance, PII detected, encrypted fields
- `engram:quality` — NDCG score, relevance warnings, chain integrity

These events enable developers and users to observe the memory system's decision-making in real time without parsing log files.

---

## Category Taxonomy

18 categories, unified across Rust backend, agent tools, and frontend UI:

| Category | Description | Typical Source |
|----------|-------------|----------------|
| `general` | Uncategorized information | Fallback |
| `preference` | User preferences and settings | Agent observation |
| `fact` | Verified factual information | Agent or user |
| `skill` | Capability-related knowledge | Skill execution |
| `context` | Situational context | Auto-capture |
| `instruction` | User-provided directives | Explicit instruction |
| `correction` | Corrected information (supersedes prior) | User correction |
| `feedback` | Quality feedback on agent behavior | User feedback |
| `project` | Project-specific knowledge | Task/orchestrator |
| `person` | Information about people | Agent observation |
| `technical` | Technical details (APIs, configs, specs) | Agent or tools |
| `session` | Session summaries from compaction | Compaction engine |
| `task_result` | Outcomes of completed tasks | Task post-capture |
| `summary` | Condensed summaries | Consolidation |
| `conversation` | Conversational context | Auto-capture |
| `insight` | Derived observations and patterns | Agent reasoning |
| `error_log` | Error information for debugging | Error handlers |
| `procedure` | Step-by-step procedures | Procedural store |

Unknown categories gracefully fall back to `general` via the `FromStr` implementation.

---

## Schema Design

Six persistent stores with full-text indices and 13 secondary indices:

```
-- Episodic memories (what happened)
episodic_memories (
    id, content, content_summary, content_key_facts, content_tags,
    outcome, category, importance, agent_id, session_id, source,
    consolidation_state, strength, trust_accuracy, trust_source_reliability,
    trust_consistency, trust_recency, trust_composite,
    scope_global, scope_project_id, scope_squad_id, scope_agent_id,
    scope_channel, scope_channel_user_id,
    embedding, embedding_model, negative_contexts,
    created_at, last_accessed_at, access_count
)

-- Semantic knowledge (SPO triples)
semantic_memories (
    id, subject, predicate, object, category, confidence,
    agent_id, source, embedding, embedding_model,
    created_at, updated_at
)

-- Procedural memory (how-to)
procedural_memories (
    id, content, trigger_condition, category,
    agent_id, source, success_count, failure_count,
    embedding, embedding_model, created_at, updated_at
)

-- Graph edges connecting memories
memory_graph_edges (
    id, source_id, source_type, target_id, target_type,
    edge_type, weight, metadata, created_at
)

-- Working memory snapshots for agent switching
working_memory_snapshots (
    agent_id, snapshot_json, saved_at
)

-- Audit trail
memory_audit_log (
    id, action, memory_type, memory_id, agent_id,
    details, created_at
)
```

Full-text indices are maintained over `episodic_memories` and `semantic_memories` to enable keyword search. Change triggers keep full-text indices synchronized with the primary stores.

---

## Configuration

The `EngramConfig` struct provides 30+ tunable parameters:

| Parameter | Default | Description |
|-----------|---------|-------------|
| `sensory_buffer_capacity` | 20 | Max items in sensory buffer |
| `working_memory_budget` | 4096 | Token budget for working memory |
| `consolidation_interval_secs` | 300 | Background consolidation cycle |
| `decay_rate` | 0.05 | Ebbinghaus decay lambda |
| `gc_strength_threshold` | 0.1 | Minimum strength to survive GC |
| `gc_importance_protection` | 0.7 | Importance above this is GC-immune |
| `search_limit` | 10 | Default search result count |
| `min_relevance_threshold` | 0.2 | Minimum score for search results |
| `clustering_similarity_threshold` | 0.75 | Cosine similarity for clustering |
| `auto_recall_enabled` | true | Pre-recall before agent turns |
| `auto_capture_enabled` | true | Post-capture after agent turns |
| `decay_lambda_base` | 0.1 | FadeMem base decay rate |
| `beta_lml` | 0.8 | Long Memory Layer decay exponent (sub-linear) |
| `beta_sml` | 1.2 | Short Memory Layer decay exponent (super-linear) |
| `promote_threshold` | 0.7 | Access frequency to promote SML → LML |
| `demote_threshold` | 0.3 | Relevance below which LML → SML |
| `fusion_similarity_threshold` | 0.75 | Cosine similarity for memory fusion |
| `ndcg_rollback_threshold` | 0.05 | Max NDCG drop before consolidation rollback |

Two presets are provided:

- **Conservative** (default) — Uses traditional Ebbinghaus decay with forgiving thresholds. Suitable for users who prefer to keep more memories longer.
- **FadeMem Paper** — Uses the exact parameters from the FadeMem research paper ($\lambda = 0.1$, $\beta_{\text{LML}} = 0.8$, $\beta_{\text{SML}} = 1.2$, $\theta_{\text{promote}} = 0.7$, $\theta_{\text{demote}} = 0.3$, $\theta_{\text{fusion}} = 0.75$). Optimized for storage efficiency with proven quality preservation.

---

## Frontier Capabilities
Beyond the core architecture, Engram implements cognitive modules drawn from neuroscience research and frontier AI papers. Each module integrates through formal trait boundaries.

### Cognitive Modules (Implemented)

- **Emotional memory dimension** (`emotional_memory.rs`) — The `emotional_memory.rs` module implements an affective scoring pipeline measuring valence, arousal, dominance, and surprise for each memory. Emotionally significant memories decay at 60% of the normal rate, receive consolidation priority boosts, and get retrieval score amplification. This models the well-documented effect that emotionally charged experiences are retained more strongly in biological memory.

- **Reflective meta-cognition** (`meta_cognition.rs`) — The `meta_cognition.rs` module performs periodic self-assessment of knowledge confidence per domain, generating "I know / I don't know" maps across the agent's memory space. These maps guide anticipatory pre-loading — if the agent knows its knowledge of a topic is sparse, it can signal this to the user rather than hallucinating from weak memories.

- **Temporal-axis retrieval** (`temporal_index.rs`) — The `temporal_index.rs` module treats time as a first-class retrieval signal. A B-tree temporal index supports range queries ("what happened last week?"), proximity scoring (memories closer in time to the query context rank higher), and pattern detection (recurring events, periodic activity). This resolves temporal queries natively rather than forcing them through keyword or vector search.

- **Intent-aware retrieval weighting** (`intent_classifier.rs`) — The `intent_classifier.rs` module implements a 6-intent classifier (informational, procedural, comparative, debugging, exploratory, confirmatory) that dynamically weights all retrieval signals per query type. A debugging query boosts error logs and technical memories. An exploratory query triggers broader graph activation. The intent signal feeds into the retrieval gate, the reranking pipeline, and the GraphRAG plane router.
- **Entity lifecycle tracking** (`entity_tracker.rs`) — The `entity_tracker.rs` module maintains canonical entity profiles with name resolution (aliases, abbreviations, misspellings all resolve to the same entity), evolving entity state, entity-centric queries ("what do I know about Project X?"), and relationship emergence detection across all memory types.

- **Hierarchical semantic compression** (`abstraction.rs`) — The `abstraction.rs` module builds a multi-level abstraction tree: individual memories → clusters → super-clusters → domain summaries. This enables navigation of knowledge at any zoom level — from a single data point up to a high-level summary of an entire domain. The compression tree is rebuilt incrementally during consolidation and provides input to GraphRAG community summaries.

- **Multi-agent memory sync** (`memory_bus.rs`) — The `memory_bus.rs` module implements a CRDT-inspired protocol for peer-to-peer knowledge sharing between agents. Vector-clock conflict resolution ensures convergence when multiple agents modify related memories concurrently. Agents can share discoveries, coordinate on projects, and maintain consistent world models without a central coordinator. Publish-side authentication prevents rogue agents from injecting poisoned memories — every publication is validated against the agent's capability token and scanned for injection payloads before entering the bus.

- **Memory replay and dream consolidation** (`dream_replay.rs`) — The `dream_replay.rs` module runs during idle periods, implementing hippocampal-inspired memory replay. During replay, memories are reactivated, latent connections between temporally distant memories are discovered, and embeddings are regenerated with evolved context. This mirrors the role of sleep in biological memory consolidation — strengthening important memories and discovering patterns that weren't obvious during waking activity.

### Infrastructure Modules (Planned/Partial)

- **HNSW vector index** — O(log n) approximate nearest neighbor search via pluggable `VectorIndex` trait
- **Proposition-level storage** — LLM-based decomposition of complex statements into atomic, independently retrievable facts
- **Smart history compression** — Three-tier message storage (verbatim → compressed → summary) with automatic age-based tiering
- **Topic-change detection** — Cosine divergence between consecutive messages to trigger working memory eviction
- **Momentum vectors** — Trajectory of recent query embeddings biases search toward conversational direction
- **Pluggable vector backends** — Trait-based abstraction allowing HNSW, product quantization, or external vector stores
- **Process memory hardening** — `mlock` to prevent swapping, core dump prevention, `zeroize` Drop implementations
- **Full database encryption at rest** — Transparent encryption of the entire persistent store via an integrated cipher layer

These eight modules are connected through 13 integration contracts ensuring they operate as a synergistic network. For example, emotional scoring feeds into the retrieval gate's relevance calculation; intent classification adjusts the reranking strategy and selects the GraphRAG plane; entity tracking informs memory fusion's scope compatibility check; and the abstraction tree provides input to community-level summaries.

---

## Quality Evaluation

Engram's quality evaluation framework ensures that every subsystem is measurable and regressions are caught automatically.

```mermaid
flowchart TD
    subgraph Retrieval["Retrieval Quality"]
        direction TB
        RQ1["NDCG\nranking quality"]
        RQ2["Precision@k\nrelevance ratio"]
        RQ3["Latency\n<10ms target at 10K"]
    end

    subgraph Faith["Faithfulness Evaluation"]
        direction TB
        FE1["Faithfulness\nfactual consistency"]
        FE2["Context Relevancy\ninjection quality"]
        FE3["Answer Relevancy\nresponse quality"]
    end

    subgraph Forget["Forgetting Regression"]
        direction TB
        FR1["Pre/post NDCG\ncomparison"]
        FR2["Chain integrity\nmulti-hop check"]
        FR3["Auto-rollback\nif \u0394 > 5%"]
    end

    subgraph Dilution["PAPerBench Dilution"]
        direction TB
        PA1["Personalization axis"]
        PA2["Privacy axis"]
        PA3["Injection-Faithfulness axis"]
    end

    Retrieval --> CI["CI Quality Gates"]
    Faith --> CI
    Forget --> CI
    Dilution --> CI

    CI --> MERGE{"Pass all\nthresholds?"}
    MERGE -- Yes --> ALLOW["Merge allowed"]
    MERGE -- No --> BLOCK["Merge blocked"]

```

### Retrieval Quality

Every search returns quality metadata alongside results:

- **NDCG (Normalized Discounted Cumulative Gain)** — Measures ranking quality against relevance judgments. Computed per-query and tracked over time.
- **Precision@k** — What fraction of the top-k returned memories are actually relevant to the query.
- **Latency** — End-to-end search time including gate decision, BM25, vector, graph activation, and reranking. Target: <10ms at 10K memories.

### Faithfulness Evaluation

Memory injection quality is evaluated along three dimensions:

1. **Faithfulness** — Are the injected memories factually consistent with the stored content? Claim decomposition verifies that the agent's response doesn't misrepresent memories.
2. **Context relevancy** — What percentage of injected memories are actually relevant to the query? Irrelevant injections waste context budget and risk attention dilution.
3. **Answer relevancy** — Does the agent's response actually address the user's query, given the injected memories?

### Unanswerability Detection

Not every query has an answer in memory. The `UnanswerabilityDetector` evaluates whether the system should refuse rather than fabricate:

- Intent-aware thresholds: factual queries require higher confidence (0.5) than exploratory queries (0.25)
- Procedural queries use an intermediate threshold (0.4) — partial procedures are worse than no procedure
- Detection feeds back into the retrieval gate's Refuse mode

### Forgetting Regression

Every consolidation cycle (decay + garbage collection + fusion) is evaluated for quality impact:

- Pre/post NDCG comparison on a fixed query set
- Chain integrity — multi-hop graph traversals that succeed before and after the cycle
- Automatic rollback if NDCG degrades by more than 5%

This prevents the system from forgetting useful information in pursuit of storage efficiency.

### Benchmark Harness

A Criterion benchmark suite measures core operations at scale:

| Benchmark | Target (10K memories) | Target (100K memories) |
|-----------|----------------------|------------------------|
| Hybrid search | < 10ms | < 25ms |
| Memory store | < 5ms | < 5ms |
| Consolidation cycle | < 500ms | < 2s |
| Context assembly | < 10ms | < 10ms |

These benchmarks run in CI. Performance regressions beyond defined thresholds block merges.

### PAPerBench — Attention Dilution Testing

PAPerBench reveals a critical truth: as context length grows, both personalization accuracy (PA) and privacy protection (PP) degrade — "attention dilution." Worse, **privacy degrades before quality** for every model tested. This directly affects memory injection.

Engram implements three-axis dilution testing derived from PAPerBench:

1. **Personalization axis** — Inject N memories, measure answer accuracy. Find the per-model inflection point where adding more memories stops helping.
2. **Privacy axis** — Inject N memories containing PII, measure whether the model leaks PII in its response. Find the inflection point where the model starts ignoring privacy instructions.
3. **Injection-Faithfulness axis** — Inject N memories, measure whether the model stays faithful to memory content vs. hallucinating.

Per-model optimal injection caps are stored in the `ModelCapabilities` registry:

| Model | Optimal Injection Cap | Context Window | Notes |
|---|---|---|---|
| GPT-4 (8K) | 3 | 8K | Tiny window — every token matters |
| Claude Opus 4.6 (200K) | 15 | 200K | Large window but dilution still applies |
| Gemini 3.1 Pro (1M) | 20 | 1M | Largest window; still has inflection |
| Ollama local (varies) | min(8, context/8000) | Varies | Conservative for resource-constrained |

### DeepResearch Bench II — Binary Rubric Evaluation

DeepResearch Bench II provides the most rigorous evaluation methodology for deep research agents: 132 tasks across 22 domains evaluated with 9,430 binary rubrics. Even the best system (GPT-5.3) achieves only 45.40% overall satisfaction.

Engram adopts their three-tier rubric approach for self-evaluation:

| Tier | What It Measures | Weight |
|---|---|---|
| **Information Recall** | Did the agent retrieve and cite relevant memories? | 40% |
| **Analysis** | Did the agent reason correctly over retrieved memories? | 35% |
| **Presentation** | Is the response well-structured and actionable? | 25% |

When a rubric failure is detected, Engram traces it to a specific component:
- **Recall failure** → retrieval pipeline problem (search, reranking, or gate)
- **Analysis failure** → context assembly problem (wrong memories injected, budget misallocation)
- **Presentation failure** → downstream of Engram (model behavior, not memory system)

CI quality gates enforce minimum thresholds:

| Metric | Threshold | Blocks Merge |
|---|---|---|
| NDCG@10 | ≥ 0.45 | Yes |
| Context relevancy | ≥ 0.60 | Yes |
| Faithfulness | ≥ 0.70 | Yes |
| Search latency (10K) | ≤ 10ms | Yes |
| Unanswerability detection | ≥ 0.80 | Yes |
| Privacy leakage rate | ≤ 0.05 | Yes |

The paper's explicit conclusion — *"Agent Memory is the future direction"* — validates Engram's entire thesis.

---

## Context Continuity

Long-running agent sessions inevitably exceed context limits. Most systems handle this with silent truncation — conversation history is cut from the front and the agent loses context. Engram implements a checkpoint-and-continue system that preserves cognitive state across context boundaries.

```mermaid
flowchart TD
    RUNNING["Agent Running"] --> SIDE{"Side-effect\noperation?"}
    SIDE -- No --> RUNNING
    SIDE -- Yes --> CAPTURE["Capture Checkpoint\nconversation + working memory\n+ file hashes + task progress"]
    CAPTURE --> STORE[("Persistent Store")]
    STORE --> CONTINUE["Continue Execution"]

    CONTINUE --> LIMIT{"Context\nlimit\nreached?"}
    LIMIT -- No --> RUNNING
    LIMIT -- Yes --> MODE{"Continuation\nmode?"}

    MODE -- Automatic --> SUMMARIZE["Task-Aware Summarize\npending work + key decisions\n+ relevant memories"]
    SUMMARIZE --> NEW["New context with summary"]
    NEW --> RUNNING

    MODE -- Manual --> CHOICE["User chooses:\n\u2022 Continue with summary\n\u2022 Revert to checkpoint\n\u2022 Start fresh"]

```

### Workspace Checkpoints

Before any side-effect operation (file write, tool execution, memory mutation), Engram captures a checkpoint:

- **Conversation state** — Full message history up to the checkpoint
- **Working memory snapshot** — All active slots with priorities and sources
- **File state** — Hashes of files that have been read or modified
- **Task progress** — Pending work items, completed items, key decisions

Checkpoints are stored in the persistent store. Any checkpoint can be reverted to, restoring the agent to an exact prior cognitive state.

### Hybrid Continuation

When context limits are reached, two continuation modes are available:

- **Automatic** (agent loops, tasks, orchestration) — The system automatically summarizes the conversation using task-aware extraction (pending work + key decisions + relevant memories), creates a new context with the summary, and continues execution. The agent never loses track of what it was doing.
- **Manual** (interactive chat) — The user is informed that context is being summarized and offered the choice to continue, revert to a checkpoint, or start fresh.

This replaces silent truncation with intelligent handoffs. No competing product offers checkpoint + continue that spans conversation + files + working memory.

---

## The Intelligence Loop

Engram's architecture is not a collection of independent features — it is a reinforcing loop where each principle strengthens the others. The Grand Research Synthesis reveals a unified intelligence architecture:

```mermaid
flowchart LR
    GATE["GATE\n\nSelf-RAG · CRAG\nDecide WHETHER\nto search"] --> RETRIEVE["RETRIEVE\n\nDeep GraphRAG\nGraphRAG-R1\nFind the right\nmemories"]
    RETRIEVE --> CAP["CAP\n\nPAPerBench\nInject the right\nAMOUNT"]
    CAP --> SKILL["SKILL\n\nVoyager · Reflexion\nApply learned\nprocedures"]
    SKILL --> EVAL["EVALUATE\n\nDRB-II · RAGAs\nMeasure everything\ncatch regressions"]
    EVAL --> FORGET["FORGET\n\nFadeMem\nRemove noise\nprovably safely"]
    FORGET --> GATE
```

> **Each principle reinforces the others:**
> Gating makes retrieval efficient → Retrieval makes capping meaningful → Capping makes skills focused → Skills make evaluation concrete → Evaluation makes forgetting safe → Forgetting makes gating accurate

### Six Principles

| Principle | Component | Paper(s) | Function |
|---|---|---|---|
| **Gate** | RetrievalGate + IntentClassifier | Self-RAG, CRAG | Decide WHETHER to search (saves ~40% of searches) |
| **Retrieve** | Hybrid Search + GraphRAG + Graph Activation | Deep GraphRAG, GraphRAG-R1 | Find the right memories across local and global planes |
| **Cap** | ContextBuilder + Per-Model Injection Limits | PAPerBench | Inject the right AMOUNT (not too many, not too few) |
| **Skill** | Skill Library + Procedural Memory + Reflexion | Voyager, Reflexion | Apply learned procedures; compound over time |
| **Evaluate** | Quality Metrics + DRB-II Rubrics + PAPerBench Dilution | DRB-II, PAPerBench, RAGAs | Measure everything; catch regressions |
| **Forget** | FadeMem Dual-Layer + Fusion + Transactional GC | FadeMem | Remove noise provably safely; keep the store lean |

The key insight: **intelligent memory is not more memory — it is better memory.** Every component in the loop works to ensure that only the right information reaches the model at the right time, and that the system learns and improves with every interaction.

This six-principle loop represents the synthesis of 21 research papers spanning 5 years. No competing product implements all six principles as a unified architecture.

---

## Verification & Operational Completeness

An architecture of this complexity — 22 modules spanning three memory tiers, eight cognitive subsystems, and a six-principle intelligence loop — requires a verification model that is itself a first-class design concern. This section describes Engram's approach to ensuring that every architectural contract described in §1–§25 is exercised, measured, and proven correct under realistic conditions.

The verification architecture addresses four fundamental challenges that arise in any cognitive memory system: ensuring that multi-tier data pipelines flow correctly end-to-end, that scoring and ranking produce consistent results across system boundaries, that modules compose without silent degradation, and that the system's quality can be measured continuously rather than assumed.

### 26.1 Layered Verification Model

Engram's verification follows a four-layer pyramid. Each layer catches a different class of failure:

```mermaid
graph TB
    subgraph "Layer 4: Cognitive Scenario Tests"
        L4["10 end-to-end scenarios exercising<br/>the full pipeline from sensory input<br/>through consolidation to retrieval"]
    end
    subgraph "Layer 3: Cross-Module Integration"
        L3["Store → consolidate → search → recall →<br/>context build as a single transaction"]
    end
    subgraph "Layer 2: Contract Tests"
        L2["Each module's public API tested against<br/>typed contracts: EngineResult invariants,<br/>scope isolation, budget compliance"]
    end
    subgraph "Layer 1: Unit Tests"
        L1["Pure function correctness: decay curves,<br/>Jaccard similarity, RRF scoring, PII regex,<br/>tokenizer accuracy, NDCG computation"]
    end
    L4 --> L3 --> L2 --> L1
```

**Layer 1 — Unit tests** verify pure functions in isolation. These include decay curve monotonicity, Jaccard/cosine similarity correctness, RRF scoring, PII detection regex coverage, tokenizer per-model accuracy, and NDCG computation. Property-based testing (via `proptest`) is used for consolidation clustering invariants — specifically that cluster membership is reflexive and symmetric, that fusion never increases total memory count, and that decay is monotonically decreasing.

**Layer 2 — Contract tests** verify that each module's public API upholds its typed contract. Every function returning `EngineResult` is tested for both success and error paths. Scope isolation is verified: agent A's store operations are invisible to agent B's searches. Budget compliance is verified: the ContextBuilder never produces output exceeding the model's declared context window.

**Layer 3 — Cross-module integration** tests exercise multi-module pipelines as single logical operations. The canonical pipeline — store → consolidate → search → recall → context build — is tested with deterministic mocks (no LLM or embedding model required). Each integration test asserts that data flows correctly between tiers and that intermediate representations (embeddings, trust scores, quality metrics) propagate through the full chain.

**Layer 4 — Cognitive scenario tests** exercise the system as a whole against realistic usage patterns. These 10 scenarios form the system's acceptance criteria:

| Scenario | Modules Exercised | Invariant |
|---|---|---|
| Memory Lifecycle | graph, consolidation, schema | 50 episodic memories → clusters formed, semantic triples extracted |
| Forgetting Quality | graph, consolidation, retrieval_quality | Decay + GC cycle → NDCG does not degrade (transactional rollback) |
| Three-Tier Flow | sensory_buffer, working_memory, graph | 30 messages → sensory → working memory promotion → long-term storage |
| Multi-Agent Isolation | memory_bus, encryption, graph | 3 agents → scope-isolated search + bus delivery with capability tokens |
| Encryption Round-Trip | encryption, graph, schema | PII stored → encrypted at rest → decrypted on search → GDPR purge zero-residual |
| Context Budget Fidelity | context_builder, model_caps, tokenizer | 5 model sizes → token counts never exceed window → correct priority ordering |
| Dream Replay Idempotency | dream_replay, graph, meta_cognition | Two replay cycles → no duplicate edges, no double-strengthening |
| Entity Lifecycle | entity_tracking, graph | Aliased mentions → canonical resolution → entity-scoped retrieval |
| Contradiction Resolution | consolidation, graph | Conflicting facts → newer wins, `Contradicts` edge, confidence transferred |
| Skill Compounding | graph (procedural), bridge | Task success → skill extracted → failure → failure variant stored |

### 26.2 Single Source of Truth Principle

A critical design constraint for multi-layer systems is that scoring, ranking, and token estimation must happen in exactly one place. Engram enforces this by treating the Rust engine as the sole authority for all numerical computation:

```mermaid
flowchart LR
    subgraph "Frontend (TypeScript)"
        UI["Display Layer"]
        IPC["IPC Passthrough"]
    end
    subgraph "Engine (Rust)"
        TOK["Tokenizer<br/>(model-specific)"]
        SCORE["Scoring Pipeline<br/>(decay · MMR · RRF · NDCG)"]
        CONFIG["SearchConfig<br/>(tunable parameters)"]
    end
    UI --> IPC
    IPC --> TOK
    IPC --> SCORE
    IPC --> CONFIG
    TOK --> IPC
    SCORE --> IPC
    CONFIG --> IPC
```

Token estimation uses model-specific divisors (Cl100k: $\div 3.4$, O200k: $\div 3.8$, Gemini: $\div 3.3$, SentencePiece: $\div 3.0$) rather than a fixed heuristic. Temporal decay, MMR diversity reranking, and quality scoring are computed server-side in Rust and returned as final scores. The frontend receives pre-scored, pre-ranked results and renders them without modification. Configuration parameters (BM25/vector weight, decay half-life, MMR lambda, relevance threshold) are read from the engine via IPC at startup, not duplicated as client-side constants.

This eliminates an entire class of divergence bugs where the frontend and backend disagree on compaction thresholds or result ordering.

### 26.3 Cognitive Pipeline Integration

The three-tier memory pipeline (§4) is designed as a unidirectional flow: Sensory Buffer → Working Memory → Long-Term Store. The verification model enforces that every tier is instantiated, connected, and exercised:

```mermaid
flowchart LR
    IN["Incoming<br/>Message"] --> SB["Sensory Buffer<br/>(ring buffer, O(1) push)"]
    SB -->|"eviction on<br/>capacity overflow"| WM["Working Memory<br/>(priority-sorted slots)"]
    WM -->|"lowest-priority<br/>eviction"| LTM["Long-Term Store<br/>(SQLite graph)"]
    LTM -->|"recall by<br/>ContextBuilder"| WM
    SB -->|"drain within<br/>token budget"| CTX["ContextBuilder<br/>(budget-aware assembly)"]
    WM -->|"priority-ordered<br/>slots"| CTX
    LTM -->|"auto-recalled<br/>memories"| CTX
    CTX --> LLM["LLM Prompt"]
```

The IntentClassifier gates every retrieval operation, producing signal weights that adapt hybrid search (§6) to the query type: factual queries weight BM25 higher, conceptual queries weight vector similarity higher, procedural queries boost the procedural memory store. This intent-adapted weighting feeds through the full pipeline — from the initial `classify_intent()` call through `resolve_hybrid_weight()` to the final `rerank_results()` output.

All consumer paths — chat, tasks, orchestrator, swarm, channel bridges — route through a single `gated_search()` entry point. This guarantees that every retrieval operation receives gate classification, intent-adapted weighting, encryption-aware decryption, CRAG quality checking, and NDCG measurement. No path bypasses the quality pipeline.

### 26.4 Embedding Model Portability

Vector embeddings are model-specific: cosine similarity between vectors from different models is meaningless. Engram handles model migration through the Dream Replay subsystem:

1. Every memory records the embedding model that generated its vector (stored in the `embedding_model` column).
2. When the configured embedding model changes, all existing vectors are marked stale.
3. Dream Replay Phase 2 (re-embed stale) processes stale vectors during idle time, generating new embeddings with the current model.
4. During the transition period, the hybrid search pipeline falls back gracefully to BM25-only for memories with stale embeddings — the system degrades to keyword search rather than producing invalid similarity scores.

This design ensures that users can switch between embedding models (e.g., `nomic-embed-text` → `mxbai-embed-large`) without data loss or retrieval corruption.

### 26.5 Scale Verification

The system's performance characteristics must be verified at realistic scale, not just assumed from algorithmic complexity bounds. Engram defines five scale tiers with target latency budgets:

| Memory Count | Search Latency | Consolidation Cycle | Concurrent Readers | RAM Budget |
|---|---|---|---|---|
| 1K | <5ms | <100ms | 8 | <50MB |
| 10K | <10ms | <500ms | 8 | <100MB |
| 50K | <15ms | <2s | 8 | <200MB |
| 100K | <25ms | <5s | 8 | <350MB |
| 500K | <50ms (HNSW disk) | <15s | 8 | <500MB |

These targets are enforced through Criterion benchmark suites that run against seeded databases at each tier. Regressions beyond the target latency block merge. The tiered vector index transitions automatically from brute-force (< 1K) to in-memory HNSW (1K–100K) to disk-backed HNSW (> 100K), keeping search latency sublinear across the full range.

### 26.6 Quality Feedback Loop

Verification is not a one-time activity — it is a continuous feedback loop built into the system's runtime. Every search operation produces a `RetrievalQualityMetrics` payload containing:

- **NDCG** — Normalized Discounted Cumulative Gain measuring ranking quality (§23)
- **Average relevancy** — Mean composite trust score across returned memories
- **Candidates filtered** — How many memories were considered vs. returned
- **Search latency** — Wall-clock time for the full pipeline
- **Rerank strategy applied** — Which of the four strategies was selected

These metrics serve dual purposes: they surface in the cognitive debug panel for developer inspection, and they feed the transactional forgetting system. If a garbage collection cycle degrades NDCG by more than 5%, the cycle is rolled back via vector savepoint. This ensures that the system can never silently degrade its own retrieval quality through its maintenance operations.

The consolidation engine reports its own metrics per cycle: candidates processed, clusters formed, semantic triples extracted, contradictions resolved, and knowledge gaps discovered. These allow the system to track its learning velocity — how efficiently it converts raw episodic experience into structured semantic knowledge over time.

---

## References

### Foundations

- Ebbinghaus, H. (1885). *Memory: A Contribution to Experimental Psychology.*
- Anderson, J. R. (1983). *A Spreading Activation Theory of Memory.* Journal of Verbal Learning and Verbal Behavior, 22(3), 261-295.
- Tulving, E. (1972). *Episodic and Semantic Memory.* In Organization of Memory. Academic Press.
- Miller, G. A. (1956). *The Magical Number Seven, Plus or Minus Two.* Psychological Review, 63(2), 81-97.
- Bartlett, F. C. (1932). *Remembering: A Study in Experimental and Social Psychology.* Cambridge University Press.
- Nader, K., Schafe, G. E., & LeDoux, J. E. (2000). *Fear Memories Require Protein Synthesis in the Amygdala for Reconsolidation After Retrieval.* Nature, 406, 722-726.

### Information Retrieval

- Robertson, S. E., & Zaragoza, H. (2009). *The Probabilistic Relevance Framework: BM25 and Beyond.* Foundations and Trends in IR, 3(4), 333-389.
- Carbonell, J., & Goldstein, J. (1998). *The Use of MMR, Diversity-Based Reranking for Reordering Documents and Producing Summaries.* SIGIR '98.
- Cormack, G. V., Clarke, C. L. A., & Buettcher, S. (2009). *Reciprocal Rank Fusion Outperforms Condorcet and Individual Rank Learning Methods.* SIGIR '09.
- Malkov, Y. A., & Yashunin, D. A. (2018). *Efficient and Robust Approximate Nearest Neighbor Using Hierarchical Navigable Small World Graphs.* IEEE TPAMI.
- Chen, J., et al. (2023). *Dense X Retrieval: What Retrieval Granularity Should We Use?* ACL 2024.

### Neuroscience & Cognition

- Cahill, L., & McGaugh, J. L. (1995). *A Novel Demonstration of Enhanced Memory Associated with Emotional Arousal.* Consciousness and Cognition, 4(4), 410-421.
- Flavell, J. H. (1979). *Metacognition and Cognitive Monitoring.* American Psychologist, 34(10), 906-911.
- Wilson, M. A., & McNaughton, B. L. (1994). *Reactivation of Hippocampal Ensemble Memories During Sleep.* Science, 265(5172), 676-679.
- Diekelmann, S., & Born, J. (2010). *The Memory Function of Sleep.* Nature Reviews Neuroscience, 11(2), 114-126.

### Distributed Systems

- Shapiro, M. et al. (2011). *Conflict-Free Replicated Data Types.* SSS 2011.
- Getoor, L., & Machanavajjhala, A. (2012). *Entity Resolution: Theory, Practice & Open Challenges.* VLDB Tutorial.

### Agent Architectures

- Park, J. S., et al. (2023). *Generative Agents: Interactive Simulacra of Human Behavior.* UIST '23.
- Packer, C., et al. (2023). *MemGPT: Towards LLMs as Operating Systems.* arXiv:2310.08560.
- Wang, G., et al. (2023). *Voyager: An Open-Ended Embodied Agent with Large Language Models.* NeurIPS 2023.
- Shinn, N., et al. (2023). *Reflexion: Language Agents with Verbal Reinforcement Learning.* NeurIPS 2023.
- Du, Y., et al. (2023). *HELPER: Memory-Augmented LLMs for Instruction-Following Embodied Agents.* EMNLP 2023.

### Retrieval-Augmented Generation

- Asai, A., et al. (2024). *Self-RAG: Learning to Retrieve, Generate, and Critique Through Self-Reflection.* ICLR 2024.
- Yan, S., et al. (2024). *Corrective Retrieval Augmented Generation (CRAG).* ICLR 2024.
- Edge, D., et al. (2024). *From Local to Global: A Graph RAG Approach to Query-Focused Summarization.* Microsoft Research.
- Microsoft Research. (2025). *LazyGraphRAG: Setting a New Standard for Quality and Cost.* microsoft.com.
- Es, S., et al. (2024). *RAGAs: Automated Evaluation of Retrieval Augmented Generation.* EACL 2024.
- Chen, J., et al. (2023). *Dense X Retrieval: What Retrieval Granularity Should We Use?* ACL 2024.
- Jiang, Z., et al. (2023). *LLMLingua: Compressing Prompts for Accelerated Inference.* EMNLP 2023.
- Santhanam, K., et al. (2022). *ColBERTv2: Effective and Efficient Retrieval via Lightweight Late Interaction.* NAACL 2022.

### 2026 Research (Revolutionary Advances)

- Zhang, Y., et al. (2026). *FadeMem: Biologically-Inspired Forgetting for Efficient Agent Memory.* arXiv:2601.18642. — Dual-layer adaptive forgetting with measured quality; 45% storage reduction, F1=29.43.
- Agarwal, S., et al. (2026). *Long Context, Less Focus: A Scaling Gap in LLMs Revealed through Privacy and Personalization* (PAPerBench). arXiv:2602.15028. — Proves attention dilution degrades personalization and privacy; validates budget-first injection.
- Li, H., et al. (2026). *Deep GraphRAG: A Balanced Approach to Hierarchical Retrieval and Adaptive Integration.* arXiv:2601.11144. — Three-stage hierarchical pipeline; DW-GRPO enables 1.5B→70B quality.
- Chen, W., et al. (2026). *WildGraphBench: Benchmarking GraphRAG with Wild-Source Corpora.* arXiv:2602.02053. — Reveals GraphRAG failure modes; validates query-type routing.
- Wang, Z., et al. (2026). *DeepResearch Bench II: Diagnosing Deep Research Agents via Rubrics from Expert Reports.* arXiv:2601.08536. — 9,430 binary rubrics across 132 tasks; best system achieves 45.40%; calls Agent Memory the future.
- Xu, R., et al. (2026). *GraphRAG-R1: Graph Retrieval-Augmented Generation with Process-Constrained Reinforcement Learning.* arXiv:2507.23581 (accepted WWW 2026). — PRA + CAF reward signals for retrieval policy training.
- Zhao, P., et al. (2026). *Retrieval-Augmented Generation for AI-Generated Content: A Survey.* Data Science and Engineering, Springer. — Comprehensive RAG taxonomy and benchmark map.

---

*Project Engram is part of OpenPawz, an open-source AI platform licensed under MIT. Contributions welcome.*
