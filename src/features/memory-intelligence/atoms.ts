// ─── Memory Intelligence · Atoms ───────────────────────────────────────
// Pure types and functions for the hybrid memory search system.
// No side effects — no Tauri IPC, no DOM, no localStorage.

// ── Types ──────────────────────────────────────────────────────────────

export interface Memory {
  id: string;
  content: string;
  category: string;
  importance: number;
  created_at: string;
  score?: number;
  agent_id?: string;
}

export interface MemorySearchOptions {
  query: string;
  limit?: number;
  agentId?: string;
}

export interface MemoryStoreOptions {
  content: string;
  category?: string;
  importance?: number;
  agentId?: string;
}

export interface MemoryStats {
  total_memories: number;
  categories: [string, number][];
  has_embeddings: boolean;
}

/** Search strategy configuration */
export interface SearchConfig {
  /** Weight for BM25 text match (0–1, default 0.4) */
  bm25Weight: number;
  /** Weight for vector semantic match (0–1, default 0.6) */
  vectorWeight: number;
  /** Temporal decay half-life in days (default 30) */
  decayHalfLifeDays: number;
  /** MMR lambda: 1.0 = pure relevance, 0.0 = pure diversity (default 0.7) */
  mmrLambda: number;
  /** Minimum similarity threshold (default 0.3) */
  threshold: number;
}

// ── Constants ──────────────────────────────────────────────────────────

export const DEFAULT_SEARCH_CONFIG: SearchConfig = {
  bm25Weight: 0.4,
  vectorWeight: 0.6,
  decayHalfLifeDays: 30,
  mmrLambda: 0.7,
  threshold: 0.3,
};

export const MEMORY_CATEGORIES = [
  'general',
  'preference',
  'fact',
  'skill',
  'context',
  'instruction',
  'correction',
  'feedback',
  'project',
  'person',
  'technical',
  'session',
  'task_result',
  'summary',
  'conversation',
  'insight',
  'error_log',
  'procedure',
] as const;

export type MemoryCategory = (typeof MEMORY_CATEGORIES)[number];

// ── Pure Helpers ───────────────────────────────────────────────────────

/** Calculate temporal decay factor for a memory.
 *  Returns a value between 0 and 1 (1 = brand new, 0.5 = one half-life old). */
export function temporalDecayFactor(createdAt: string, halfLifeDays = 30): number {
  const created = new Date(createdAt).getTime();
  const now = Date.now();
  const ageDays = (now - created) / (1000 * 60 * 60 * 24);
  const decayConstant = Math.LN2 / halfLifeDays;
  return Math.exp(-decayConstant * ageDays);
}

/** Apply temporal decay to a memory's score in-place. */
export function applyDecay(memories: Memory[], halfLifeDays = 30): Memory[] {
  return memories.map((m) => ({
    ...m,
    score: (m.score ?? 0) * temporalDecayFactor(m.created_at, halfLifeDays),
  }));
}

/** Jaccard similarity between two text strings (word-level). */
export function jaccardSimilarity(a: string, b: string): number {
  const wordsA = new Set(
    a
      .toLowerCase()
      .split(/\s+/)
      .filter((w) => w.length > 2),
  );
  const wordsB = new Set(
    b
      .toLowerCase()
      .split(/\s+/)
      .filter((w) => w.length > 2),
  );
  if (wordsA.size === 0 && wordsB.size === 0) return 1;
  let intersection = 0;
  for (const w of wordsA) if (wordsB.has(w)) intersection++;
  const union = new Set([...wordsA, ...wordsB]).size;
  return union === 0 ? 0 : intersection / union;
}

/** MMR re-rank on the frontend (e.g. for display dedup).
 *  lambda: 1.0 = pure relevance, 0.0 = pure diversity. */
export function mmrRerank(candidates: Memory[], k: number, lambda = 0.7): Memory[] {
  if (candidates.length === 0 || k === 0) return [];

  const sorted = [...candidates].sort((a, b) => (b.score ?? 0) - (a.score ?? 0));
  const selected: Memory[] = [sorted[0]];
  const remaining = sorted.slice(1);

  while (selected.length < k && remaining.length > 0) {
    let bestIdx = 0;
    let bestMmr = -Infinity;

    for (let i = 0; i < remaining.length; i++) {
      const relevance = remaining[i].score ?? 0;
      const maxSim = Math.max(
        ...selected.map((s) => jaccardSimilarity(remaining[i].content, s.content)),
      );
      const mmr = lambda * relevance - (1 - lambda) * maxSim;
      if (mmr > bestMmr) {
        bestMmr = mmr;
        bestIdx = i;
      }
    }

    selected.push(remaining.splice(bestIdx, 1)[0]);
  }

  return selected;
}

/** Format a memory for display in the chat context. */
export function formatMemoryForContext(mem: Memory): string {
  const agentTag = mem.agent_id ? ` (agent: ${mem.agent_id})` : '';
  const scoreTag = mem.score != null ? ` [${mem.score.toFixed(2)}]` : '';
  return `- [${mem.category}]${agentTag}${scoreTag} ${mem.content}`;
}

/** Group memories by category. */
export function groupByCategory(memories: Memory[]): Record<string, Memory[]> {
  const groups: Record<string, Memory[]> = {};
  for (const mem of memories) {
    const cat = mem.category || 'general';
    if (!groups[cat]) groups[cat] = [];
    groups[cat].push(mem);
  }
  return groups;
}

/** Describe memory age in human-readable form. */
export function describeAge(createdAt: string): string {
  const ageMs = Date.now() - new Date(createdAt).getTime();
  const hours = ageMs / (1000 * 60 * 60);
  if (hours < 1) return 'just now';
  if (hours < 24) return `${Math.floor(hours)}h ago`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days}d ago`;
  const months = Math.floor(days / 30);
  return `${months}mo ago`;
}
