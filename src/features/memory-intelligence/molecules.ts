// ─── Memory Intelligence · Molecules ───────────────────────────────────
// Composed functions with side effects: Tauri IPC for memory operations.
// Builds on atoms for hybrid search, temporal decay, and MMR.

import {
  type Memory,
  type MemorySearchOptions,
  type MemoryStoreOptions,
  type MemoryStats,
  type SearchConfig,
  DEFAULT_SEARCH_CONFIG,
  applyDecay,
  mmrRerank,
  formatMemoryForContext,
  groupByCategory,
} from './atoms';

const CONFIG_STORAGE_KEY = 'paw_memory_search_config';

// ── Search Config Persistence ──────────────────────────────────────────

/** Load search config from localStorage */
export function loadSearchConfig(): SearchConfig {
  try {
    const raw = localStorage.getItem(CONFIG_STORAGE_KEY);
    if (raw) return { ...DEFAULT_SEARCH_CONFIG, ...JSON.parse(raw) };
  } catch {
    /* ignore */
  }
  return { ...DEFAULT_SEARCH_CONFIG };
}

/** Save search config to localStorage */
export function saveSearchConfig(config: SearchConfig): void {
  localStorage.setItem(CONFIG_STORAGE_KEY, JSON.stringify(config));
}

// ── Tauri IPC Wrappers ─────────────────────────────────────────────────

/** Store a memory via the engine (handles embedding automatically). */
export async function storeMemory(opts: MemoryStoreOptions): Promise<string> {
  // @ts-ignore — Tauri invoke
  const { invoke } = window.__TAURI__.core;
  return invoke('engine_memory_store', {
    content: opts.content,
    category: opts.category ?? 'general',
    importance: opts.importance ?? 5,
    agentId: opts.agentId ?? null,
  });
}

/** Search memories via the engine (hybrid BM25 + vector + temporal decay + MMR). */
export async function searchMemories(opts: MemorySearchOptions): Promise<Memory[]> {
  // @ts-ignore — Tauri invoke
  const { invoke } = window.__TAURI__.core;
  return invoke('engine_memory_search', {
    query: opts.query,
    limit: opts.limit ?? 10,
    agentId: opts.agentId ?? null,
  });
}

/** Get memory store stats from the engine. */
export async function getMemoryStats(): Promise<MemoryStats> {
  // @ts-ignore — Tauri invoke
  const { invoke } = window.__TAURI__.core;
  return invoke('engine_memory_stats');
}

/** Delete a memory by ID. */
export async function deleteMemory(memoryId: string): Promise<void> {
  // @ts-ignore — Tauri invoke
  const { invoke } = window.__TAURI__.core;
  return invoke('engine_memory_delete', { id: memoryId });
}

// ── Composite Operations ───────────────────────────────────────────────

/** Search memories for a specific agent, with client-side decay + MMR refinement. */
export async function searchForAgent(query: string, agentId: string, limit = 5): Promise<Memory[]> {
  const config = loadSearchConfig();
  const results = await searchMemories({ query, limit: limit * 2, agentId });

  // Apply client-side decay refinement and MMR
  const decayed = applyDecay(results, config.decayHalfLifeDays);
  return mmrRerank(decayed, limit, config.mmrLambda);
}

/** Build memory context string for injection into system prompt. */
export async function buildMemoryContext(
  query: string,
  agentId?: string,
  limit = 5,
): Promise<string | null> {
  const results = agentId
    ? await searchForAgent(query, agentId, limit)
    : await searchMemories({ query, limit });

  if (results.length === 0) return null;

  const lines = results.map(formatMemoryForContext);
  return `## Relevant Memories\n${lines.join('\n')}`;
}

/** Get categorized memory overview for an agent. */
export async function getAgentMemoryOverview(
  agentId: string,
): Promise<{ stats: MemoryStats; grouped: Record<string, Memory[]> }> {
  const [stats, memories] = await Promise.all([
    getMemoryStats(),
    searchMemories({ query: '*', limit: 100, agentId }),
  ]);
  const grouped = groupByCategory(memories);
  return { stats, grouped };
}

// ── Re-exports ─────────────────────────────────────────────────────────

export type {
  Memory,
  MemorySearchOptions,
  MemoryStoreOptions,
  MemoryStats,
  SearchConfig,
} from './atoms';

export {
  DEFAULT_SEARCH_CONFIG,
  MEMORY_CATEGORIES,
  temporalDecayFactor,
  applyDecay,
  jaccardSimilarity,
  mmrRerank,
  formatMemoryForContext,
  groupByCategory,
  describeAge,
} from './atoms';
