// ─────────────────────────────────────────────────────────────────────────────
// Memory-Aware Flows — Atoms (Pure Logic)
// Types and helpers for memory read/write nodes in flow execution.
// No DOM, no IPC — fully testable.
// ─────────────────────────────────────────────────────────────────────────────

// ── Types ──────────────────────────────────────────────────────────────────

/** Config specific to memory-write nodes. */
export interface MemoryWriteConfig {
  /** What to store — 'output' uses node output, 'custom' uses memoryContent */
  memorySource: 'output' | 'custom';
  /** Custom content to store (when memorySource === 'custom') */
  memoryContent?: string;
  /** Memory category (e.g. 'insight', 'fact', 'preference') */
  memoryCategory: string;
  /** Importance level 0–1 (default 0.5) */
  memoryImportance: number;
  /** Optional agent ID to scope the memory */
  memoryAgentId?: string;
}

/** Config specific to memory-recall nodes. */
export interface MemoryRecallConfig {
  /** Search query — 'input' uses upstream input, 'custom' uses memoryQuery */
  memoryQuerySource: 'input' | 'custom';
  /** Custom search query (when memoryQuerySource === 'custom') */
  memoryQuery?: string;
  /** Maximum results to return */
  memoryLimit: number;
  /** Optional agent ID to scope the search */
  memoryAgentId?: string;
  /** Minimum relevance threshold 0–1 */
  memoryThreshold: number;
  /** Output format: 'text' joins as text, 'json' returns array */
  memoryOutputFormat: 'text' | 'json';
}

/** Config specific to squad nodes. */
export interface SquadNodeConfig {
  /** Squad ID to invoke */
  squadId: string;
  /** Objective/task for the squad */
  squadObjective?: string;
  /** Timeout for squad execution in ms (default 300000 = 5min) */
  squadTimeoutMs: number;
  /** Max rounds of discussion */
  squadMaxRounds: number;
}

// ── Defaults ───────────────────────────────────────────────────────────────

export const DEFAULT_MEMORY_WRITE: MemoryWriteConfig = {
  memorySource: 'output',
  memoryCategory: 'insight',
  memoryImportance: 0.5,
};

export const DEFAULT_MEMORY_RECALL: MemoryRecallConfig = {
  memoryQuerySource: 'input',
  memoryLimit: 5,
  memoryThreshold: 0.3,
  memoryOutputFormat: 'text',
};

export const DEFAULT_SQUAD_CONFIG: SquadNodeConfig = {
  squadId: '',
  squadTimeoutMs: 300_000,
  squadMaxRounds: 5,
};

// ── Memory Categories ──────────────────────────────────────────────────────

/** Common memory categories available in the UI dropdown. */
export const MEMORY_CATEGORY_OPTIONS = [
  { value: 'general', label: 'General', icon: 'category' },
  { value: 'preference', label: 'Preference', icon: 'tune' },
  { value: 'fact', label: 'Fact', icon: 'fact_check' },
  { value: 'skill', label: 'Skill', icon: 'build' },
  { value: 'context', label: 'Context', icon: 'info' },
  { value: 'instruction', label: 'Instruction', icon: 'school' },
  { value: 'correction', label: 'Correction', icon: 'edit' },
  { value: 'feedback', label: 'Feedback', icon: 'thumbs_up_down' },
  { value: 'project', label: 'Project', icon: 'folder' },
  { value: 'person', label: 'Person', icon: 'person' },
  { value: 'technical', label: 'Technical', icon: 'code' },
  { value: 'session', label: 'Session', icon: 'history' },
  { value: 'task_result', label: 'Task Result', icon: 'task_alt' },
  { value: 'summary', label: 'Summary', icon: 'summarize' },
  { value: 'conversation', label: 'Conversation', icon: 'chat' },
  { value: 'insight', label: 'Insight', icon: 'lightbulb' },
  { value: 'error_log', label: 'Error Log', icon: 'error_outline' },
  { value: 'procedure', label: 'Procedure', icon: 'list_alt' },
] as const;

// ── Helpers ────────────────────────────────────────────────────────────────

/**
 * Extract memory-write config from a node's config object.
 */
export function getMemoryWriteConfig(config: Record<string, unknown>): MemoryWriteConfig {
  return {
    memorySource: (config.memorySource as 'output' | 'custom') ?? DEFAULT_MEMORY_WRITE.memorySource,
    memoryContent: (config.memoryContent as string) ?? undefined,
    memoryCategory: (config.memoryCategory as string) ?? DEFAULT_MEMORY_WRITE.memoryCategory,
    memoryImportance: (config.memoryImportance as number) ?? DEFAULT_MEMORY_WRITE.memoryImportance,
    memoryAgentId: (config.memoryAgentId as string) ?? undefined,
  };
}

/**
 * Extract memory-recall config from a node's config object.
 */
export function getMemoryRecallConfig(config: Record<string, unknown>): MemoryRecallConfig {
  return {
    memoryQuerySource:
      (config.memoryQuerySource as 'input' | 'custom') ?? DEFAULT_MEMORY_RECALL.memoryQuerySource,
    memoryQuery: (config.memoryQuery as string) ?? undefined,
    memoryLimit: (config.memoryLimit as number) ?? DEFAULT_MEMORY_RECALL.memoryLimit,
    memoryAgentId: (config.memoryAgentId as string) ?? undefined,
    memoryThreshold: (config.memoryThreshold as number) ?? DEFAULT_MEMORY_RECALL.memoryThreshold,
    memoryOutputFormat:
      (config.memoryOutputFormat as 'text' | 'json') ?? DEFAULT_MEMORY_RECALL.memoryOutputFormat,
  };
}

/**
 * Extract squad node config from a node's config object.
 */
export function getSquadNodeConfig(config: Record<string, unknown>): SquadNodeConfig {
  return {
    squadId: (config.squadId as string) ?? DEFAULT_SQUAD_CONFIG.squadId,
    squadObjective: (config.squadObjective as string) ?? undefined,
    squadTimeoutMs: (config.squadTimeoutMs as number) ?? DEFAULT_SQUAD_CONFIG.squadTimeoutMs,
    squadMaxRounds: (config.squadMaxRounds as number) ?? DEFAULT_SQUAD_CONFIG.squadMaxRounds,
  };
}

/**
 * Format recalled memories into a string for downstream nodes.
 */
export function formatRecalledMemories(
  memories: Array<{ content: string; category: string; importance: number; score?: number }>,
  format: 'text' | 'json',
): string {
  if (memories.length === 0) return format === 'json' ? '[]' : 'No relevant memories found.';

  if (format === 'json') {
    return JSON.stringify(memories, null, 2);
  }

  // Text format: numbered list with scores
  return memories
    .map((m, i) => {
      const scoreStr = m.score !== undefined ? ` (relevance: ${(m.score * 100).toFixed(0)}%)` : '';
      return `${i + 1}. [${m.category}] ${m.content}${scoreStr}`;
    })
    .join('\n');
}

/**
 * Build the content to store in memory from node output.
 */
export function buildMemoryContent(
  nodeOutput: string,
  config: MemoryWriteConfig,
  nodeLabel: string,
): string {
  if (config.memorySource === 'custom' && config.memoryContent) {
    return config.memoryContent;
  }
  // Default: use the node's output, prefixed with context
  return `[Flow node: ${nodeLabel}] ${nodeOutput}`;
}
