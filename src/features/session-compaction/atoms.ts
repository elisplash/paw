// ─────────────────────────────────────────────────────────────────────────────
// Session Compaction — Atoms
// Pure functions for token estimation and compaction readiness checks.
// ─────────────────────────────────────────────────────────────────────────────

import type { EngineStoredMessage } from '../../engine';

// ── Types ──────────────────────────────────────────────────────────────────

export interface CompactionStats {
  messageCount: number;
  estimatedTokens: number;
  needsCompaction: boolean;
  /** How many messages would be summarized (old) vs kept (recent) */
  toSummarize: number;
  toKeep: number;
}

export interface CompactionConfig {
  /** Minimum message count before compaction is available */
  minMessages: number;
  /** Estimated token threshold to recommend compaction */
  tokenThreshold: number;
  /** How many recent messages to preserve verbatim */
  keepRecent: number;
}

// ── Constants ──────────────────────────────────────────────────────────────

export const DEFAULT_COMPACTION_CONFIG: CompactionConfig = {
  minMessages: 20,
  tokenThreshold: 60_000,
  keepRecent: 6,
};

// ── Pure Functions ─────────────────────────────────────────────────────────

/**
 * Estimate tokens for a stored message (~4 chars per token).
 */
export function estimateMessageTokens(msg: EngineStoredMessage): number {
  const textLen = (msg.content ?? '').length;
  const tcLen = (msg.tool_calls_json ?? '').length;
  return Math.ceil((textLen + tcLen) / 4) + 4;
}

/**
 * Analyze a list of messages and determine compaction readiness.
 */
export function analyzeCompactionNeed(
  messages: EngineStoredMessage[],
  config: CompactionConfig = DEFAULT_COMPACTION_CONFIG,
): CompactionStats {
  const messageCount = messages.length;
  const estimatedTokens = messages.reduce((sum, m) => sum + estimateMessageTokens(m), 0);
  const needsCompaction =
    messageCount >= config.minMessages && estimatedTokens > config.tokenThreshold;

  const keepCount = Math.min(config.keepRecent, messageCount);
  const toSummarize = Math.max(0, messageCount - keepCount);

  return {
    messageCount,
    estimatedTokens,
    needsCompaction,
    toSummarize,
    toKeep: keepCount,
  };
}

/**
 * Format a compaction result for display.
 */
export function formatCompactionResult(result: {
  messages_before: number;
  messages_after: number;
  tokens_before: number;
  tokens_after: number;
  summary_length: number;
}): string {
  const saved = result.tokens_before - result.tokens_after;
  const pct = result.tokens_before > 0 ? Math.round((saved / result.tokens_before) * 100) : 0;

  return [
    `**Session Compacted**`,
    `Messages: ${result.messages_before} → ${result.messages_after}`,
    `Tokens: ~${result.tokens_before.toLocaleString()} → ~${result.tokens_after.toLocaleString()} (${pct}% reduction)`,
    `Summary: ${result.summary_length} chars`,
  ].join('\n');
}
