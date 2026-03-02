import { describe, it, expect } from 'vitest';
import {
  estimateMessageTokens,
  analyzeCompactionNeed,
  formatCompactionResult,
  DEFAULT_COMPACTION_CONFIG,
} from './atoms';
import type { EngineStoredMessage } from '../../engine';

const makeMsg = (content: string, toolCalls = ''): EngineStoredMessage => ({
  id: '1',
  session_id: 's1',
  role: 'assistant',
  content,
  tool_calls_json: toolCalls,
  created_at: new Date().toISOString(),
});

// ── estimateMessageTokens ──────────────────────────────────────────────

describe('estimateMessageTokens', () => {
  it('estimates ~1 token per 4 chars + overhead', () => {
    const msg = makeMsg('Hello world! This is a test message.');
    const tokens = estimateMessageTokens(msg);
    expect(tokens).toBeGreaterThan(5);
    expect(tokens).toBeLessThan(50);
  });

  it('includes tool calls in estimate', () => {
    const msg = makeMsg('short', '{"name":"read_file","args":{}}');
    const withoutTool = estimateMessageTokens(makeMsg('short'));
    expect(estimateMessageTokens(msg)).toBeGreaterThan(withoutTool);
  });

  it('handles empty message', () => {
    const msg = makeMsg('');
    expect(estimateMessageTokens(msg)).toBeGreaterThanOrEqual(4); // overhead
  });
});

// ── analyzeCompactionNeed ──────────────────────────────────────────────

describe('analyzeCompactionNeed', () => {
  it('says no compaction needed for few messages', () => {
    const msgs = Array.from({ length: 5 }, (_, i) => makeMsg(`msg ${i}`));
    const stats = analyzeCompactionNeed(msgs);
    expect(stats.needsCompaction).toBe(false);
    expect(stats.messageCount).toBe(5);
  });

  it('says compaction needed for many long messages', () => {
    const longContent = 'x'.repeat(10000);
    const msgs = Array.from({ length: 25 }, () => makeMsg(longContent));
    const stats = analyzeCompactionNeed(msgs);
    expect(stats.needsCompaction).toBe(true);
    expect(stats.toSummarize).toBe(25 - DEFAULT_COMPACTION_CONFIG.keepRecent);
    expect(stats.toKeep).toBe(DEFAULT_COMPACTION_CONFIG.keepRecent);
  });

  it('respects custom config', () => {
    const msgs = Array.from({ length: 10 }, () => makeMsg('x'.repeat(40000)));
    const stats = analyzeCompactionNeed(msgs, {
      minMessages: 5,
      tokenThreshold: 1000,
      keepRecent: 3,
    });
    expect(stats.needsCompaction).toBe(true);
    expect(stats.toKeep).toBe(3);
  });
});

// ── formatCompactionResult ─────────────────────────────────────────────

describe('formatCompactionResult', () => {
  it('formats result with reduction percentage', () => {
    const text = formatCompactionResult({
      messages_before: 50,
      messages_after: 10,
      tokens_before: 60000,
      tokens_after: 15000,
      summary_length: 500,
    });
    expect(text).toContain('50 → 10');
    expect(text).toContain('75%');
    expect(text).toContain('500 chars');
  });

  it('handles zero tokens gracefully', () => {
    const text = formatCompactionResult({
      messages_before: 0,
      messages_after: 0,
      tokens_before: 0,
      tokens_after: 0,
      summary_length: 0,
    });
    expect(text).toContain('0%');
  });
});

// ── Additional session compaction edge cases ───────────────────────────

describe('estimateMessageTokens — edge cases', () => {
  it('handles null content gracefully', () => {
    const msg = makeMsg('');
    // @ts-expect-error testing null content
    msg.content = null;
    const tokens = estimateMessageTokens(msg);
    expect(tokens).toBeGreaterThanOrEqual(4);
  });

  it('handles null tool_calls_json gracefully', () => {
    const msg = makeMsg('hello');
    // @ts-expect-error testing null
    msg.tool_calls_json = null;
    const tokens = estimateMessageTokens(msg);
    expect(tokens).toBeGreaterThanOrEqual(4);
  });

  it('scales roughly with content length', () => {
    const short = estimateMessageTokens(makeMsg('hi'));
    const long = estimateMessageTokens(makeMsg('x'.repeat(1000)));
    expect(long).toBeGreaterThan(short * 5);
  });
});

describe('analyzeCompactionNeed — edge cases', () => {
  it('keeps all messages when count < keepRecent', () => {
    const msgs = Array.from({ length: 3 }, (_, i) => makeMsg(`msg ${i}`));
    const stats = analyzeCompactionNeed(msgs, {
      minMessages: 2,
      tokenThreshold: 0,
      keepRecent: 10,
    });
    expect(stats.toKeep).toBe(3);
    expect(stats.toSummarize).toBe(0);
  });

  it('returns false when messages >= min but tokens < threshold', () => {
    const msgs = Array.from({ length: 25 }, (_, i) => makeMsg(`msg ${i}`));
    const stats = analyzeCompactionNeed(msgs, {
      minMessages: 5,
      tokenThreshold: 999_999,
      keepRecent: 6,
    });
    expect(stats.needsCompaction).toBe(false);
  });

  it('returns false when tokens > threshold but messages < min', () => {
    const msgs = Array.from({ length: 3 }, () => makeMsg('x'.repeat(100_000)));
    const stats = analyzeCompactionNeed(msgs, {
      minMessages: 50,
      tokenThreshold: 100,
      keepRecent: 2,
    });
    expect(stats.needsCompaction).toBe(false);
  });

  it('handles empty message array', () => {
    const stats = analyzeCompactionNeed([]);
    expect(stats.messageCount).toBe(0);
    expect(stats.estimatedTokens).toBe(0);
    expect(stats.needsCompaction).toBe(false);
    expect(stats.toKeep).toBe(0);
    expect(stats.toSummarize).toBe(0);
  });

  it('sums tokens correctly across messages', () => {
    // Each message: ceil((8 + 0)/4) + 4 = ceil(2) + 4 = 6 tokens
    const msgs = Array.from({ length: 10 }, () => makeMsg('12345678'));
    const stats = analyzeCompactionNeed(msgs);
    expect(stats.estimatedTokens).toBe(60); // 10 * 6
  });
});

describe('formatCompactionResult — edge cases', () => {
  it('formats large numbers with locale separators', () => {
    const text = formatCompactionResult({
      messages_before: 500,
      messages_after: 20,
      tokens_before: 1_000_000,
      tokens_after: 50_000,
      summary_length: 2000,
    });
    expect(text).toContain('95%');
    expect(text).toContain('500 → 20');
  });

  it('handles 100% reduction', () => {
    const text = formatCompactionResult({
      messages_before: 10,
      messages_after: 1,
      tokens_before: 5000,
      tokens_after: 0,
      summary_length: 100,
    });
    expect(text).toContain('100%');
  });

  it('shows summary length', () => {
    const text = formatCompactionResult({
      messages_before: 10,
      messages_after: 5,
      tokens_before: 1000,
      tokens_after: 500,
      summary_length: 350,
    });
    expect(text).toContain('350 chars');
  });
});

describe('DEFAULT_COMPACTION_CONFIG', () => {
  it('has minMessages >= 1', () => {
    expect(DEFAULT_COMPACTION_CONFIG.minMessages).toBeGreaterThanOrEqual(1);
  });

  it('keepRecent is reasonable (< minMessages)', () => {
    expect(DEFAULT_COMPACTION_CONFIG.keepRecent).toBeLessThan(
      DEFAULT_COMPACTION_CONFIG.minMessages,
    );
  });

  it('tokenThreshold is positive', () => {
    expect(DEFAULT_COMPACTION_CONFIG.tokenThreshold).toBeGreaterThan(0);
  });
});
