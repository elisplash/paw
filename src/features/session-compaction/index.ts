// ─────────────────────────────────────────────────────────────────────────────
// Session Compaction — Public API
// ─────────────────────────────────────────────────────────────────────────────

// Atoms (pure)
export {
  type CompactionStats,
  type CompactionConfig,
  DEFAULT_COMPACTION_CONFIG,
  estimateMessageTokens,
  analyzeCompactionNeed,
  formatCompactionResult,
} from './atoms';

// Molecules (side-effects)
export { type CompactionOutcome, compactSession, checkAutoCompaction } from './molecules';
