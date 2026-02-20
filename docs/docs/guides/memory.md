---
sidebar_position: 2
title: Memory
---

# Memory

Pawz has a built-in memory system that lets agents remember facts, preferences, and context across conversations.

## How it works

```
User message → auto-recall (retrieve relevant memories)
                    ↓
            inject into context
                    ↓
            agent responds
                    ↓
            auto-capture (extract memorable facts)
                    ↓
            embed + store
```

## Configuration

Go to **Settings → Sessions** to configure memory:

| Setting | Default | Description |
|---------|---------|-------------|
| **Embedding model** | `nomic-embed-text` | Model used to create vector embeddings |
| **Embedding base URL** | `http://localhost:11434` | Ollama endpoint for embeddings |
| **Embedding dimensions** | 768 | Vector size (match your model) |
| **Auto-recall** | On | Automatically retrieve relevant memories |
| **Auto-capture** | On | Automatically extract facts from conversations |
| **Recall limit** | 5 | Max memories injected per message |
| **Recall threshold** | 0.3 | Minimum relevance score (0–1) |

## Memory categories

Each memory is tagged with a category:

| Category | Use case |
|----------|----------|
| `general` | Catch-all |
| `preference` | User likes/dislikes, style preferences |
| `instruction` | Standing orders, rules |
| `context` | Background information |
| `fact` | Concrete facts, dates, numbers |
| `project` | Project-specific context |
| `person` | Info about people |
| `technical` | Technical details, tools, configs |

## Hybrid search

Memory retrieval uses a hybrid algorithm combining multiple signals:

1. **BM25** — text relevance via SQLite FTS5
2. **Vector cosine similarity** — semantic meaning via embeddings
3. **Weighted merge** — BM25 (0.4) + vector (0.6)
4. **Temporal decay** — half-life of 30 days (recent memories rank higher)
5. **MMR re-ranking** — λ=0.7 balances relevance vs. diversity

### Tuning search

In the Memory Palace, you can adjust:

| Parameter | Default | Effect |
|-----------|---------|--------|
| BM25 weight | 0.4 | Text match importance |
| Vector weight | 0.6 | Semantic match importance |
| Decay half-life | 30 days | How fast old memories fade |
| MMR lambda | 0.7 | 1.0 = pure relevance, 0.0 = max diversity |
| Threshold | 0.3 | Minimum score to include |

## Memory Palace

The Memory Palace is a dedicated view for managing all stored memories:

- **Browse** — see all memories with category tags and importance scores
- **Search** — test queries against the hybrid search algorithm
- **Add** — manually store a memory
- **Edit** — update content, category, or importance
- **Delete** — remove individual memories

## Slash commands

Quick memory operations from any chat:

```
/remember <text>    Store a memory manually
/forget <id>        Delete a memory by ID
/recall <query>     Search memories and show results
```

## Auto-capture

When auto-capture is enabled, the engine extracts memorable facts from conversations using heuristics. It looks for:

- User preferences ("I prefer...", "I like...")
- Explicit instructions ("Always...", "Never...")
- Personal context (names, locations, roles)
- Concrete facts (dates, numbers, decisions)

## Embedding setup

Pawz auto-manages embeddings via Ollama:

1. Checks if Ollama is reachable
2. Auto-starts Ollama if needed
3. Checks if the embedding model is available
4. Auto-pulls the model if missing
5. Tests embedding generation

If you switch embedding models, use **backfill** to re-embed existing memories.

## Embedding backends

The engine tries endpoints in order:
1. Ollama `/api/embed` (current API)
2. Ollama `/api/embeddings` (legacy API)
3. OpenAI-compatible `/v1/embeddings`

This means you can use any OpenAI-compatible embedding API by changing the base URL.
