// ─────────────────────────────────────────────────────────────────────────────
// Session Compaction — Molecules
// Composed behaviours: trigger compaction via engine, auto-compact checks.
// ─────────────────────────────────────────────────────────────────────────────

import { pawEngine } from '../../engine';
import {
  analyzeCompactionNeed,
  formatCompactionResult,
  DEFAULT_COMPACTION_CONFIG,
  type CompactionConfig,
} from './atoms';

// ── Types ──────────────────────────────────────────────────────────────────

export interface CompactionOutcome {
  success: boolean;
  message: string;
  result?: {
    messages_before: number;
    messages_after: number;
    tokens_before: number;
    tokens_after: number;
    summary_length: number;
  };
}

// ── Executors ──────────────────────────────────────────────────────────────

/**
 * Perform compaction on a session via the Rust engine.
 */
export async function compactSession(sessionId: string): Promise<CompactionOutcome> {
  try {
    const result = await pawEngine.sessionCompact(sessionId);
    return {
      success: true,
      message: formatCompactionResult(result),
      result,
    };
  } catch (e) {
    return {
      success: false,
      message: `Compaction failed: ${e}`,
    };
  }
}

/**
 * Check if a session should be auto-compacted.
 * Returns a message suggestion if compaction is recommended, null otherwise.
 */
export async function checkAutoCompaction(
  sessionId: string,
  config: CompactionConfig = DEFAULT_COMPACTION_CONFIG,
): Promise<string | null> {
  try {
    const messages = await pawEngine.chatHistory(sessionId, 10_000);
    const stats = analyzeCompactionNeed(messages, config);

    if (stats.needsCompaction) {
      return (
        `This session has ${stats.messageCount} messages (~${stats.estimatedTokens.toLocaleString()} tokens). ` +
        `Consider running \`/compact\` to free context space.`
      );
    }
    return null;
  } catch {
    return null;
  }
}
