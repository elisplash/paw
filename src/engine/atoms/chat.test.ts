// @vitest-environment jsdom
import { describe, it, expect } from 'vitest';
import {
  generateSessionLabel,
  extractContent,
  findLastIndex,
  fileTypeIcon,
  fileToBase64,
  fmtK,
  estimateContextBreakdown,
} from './chat';

// ── generateSessionLabel ──────────────────────────────────────────────────

describe('generateSessionLabel', () => {
  it('returns a trimmed label from a simple message', () => {
    expect(generateSessionLabel('Hello world')).toBe('Hello world');
  });

  it('strips leading slash commands', () => {
    expect(generateSessionLabel('/model gpt-4o explain the code')).toBe('gpt-4o explain the code');
  });

  it('strips markdown characters', () => {
    expect(generateSessionLabel('# **Bold** and _italic_')).toBe('Bold and italic');
  });

  it('collapses whitespace', () => {
    expect(generateSessionLabel('  lots   of   spaces  ')).toBe('lots of spaces');
  });

  it('truncates to 50 chars with ellipsis on word boundary', () => {
    const long = 'This is a very long message that should be truncated at fifty characters maximum';
    const label = generateSessionLabel(long);
    expect(label.length).toBeLessThanOrEqual(50);
    expect(label.endsWith('…')).toBe(true);
  });

  it('returns "New chat" for empty/whitespace input', () => {
    expect(generateSessionLabel('')).toBe('New chat');
    expect(generateSessionLabel('   ')).toBe('New chat');
  });

  it('returns "New chat" for markdown-only input', () => {
    expect(generateSessionLabel('### **')).toBe('New chat');
  });
});

// ── extractContent ────────────────────────────────────────────────────────

describe('extractContent', () => {
  it('returns string content as-is', () => {
    expect(extractContent('hello')).toBe('hello');
  });

  it('extracts text from content block arrays', () => {
    const blocks = [
      { type: 'text', text: 'first' },
      { type: 'tool_use', id: '1', name: 'test' },
      { type: 'text', text: 'second' },
    ];
    expect(extractContent(blocks)).toBe('first\nsecond');
  });

  it('extracts text from a single content block object', () => {
    expect(extractContent({ type: 'text', text: 'solo' })).toBe('solo');
  });

  it('returns empty string for null/undefined', () => {
    expect(extractContent(null)).toBe('');
    expect(extractContent(undefined)).toBe('');
  });

  it('stringifies non-matching objects', () => {
    expect(extractContent(42)).toBe('42');
    expect(extractContent(true)).toBe('true');
  });

  it('ignores non-text blocks in arrays', () => {
    const blocks = [
      { type: 'image', url: 'http://example.com' },
      { type: 'text', text: 'only this' },
    ];
    expect(extractContent(blocks)).toBe('only this');
  });
});

// ── findLastIndex ─────────────────────────────────────────────────────────

describe('findLastIndex', () => {
  it('finds last matching element', () => {
    expect(findLastIndex([1, 2, 3, 2, 1], (n) => n === 2)).toBe(3);
  });

  it('returns -1 when no match', () => {
    expect(findLastIndex([1, 2, 3], (n) => n === 5)).toBe(-1);
  });

  it('handles empty array', () => {
    expect(findLastIndex([], () => true)).toBe(-1);
  });

  it('returns first element when only match', () => {
    expect(findLastIndex([1, 2, 3], (n) => n === 1)).toBe(0);
  });
});

// ── fileTypeIcon ──────────────────────────────────────────────────────────

describe('fileTypeIcon', () => {
  it('returns "image" for image MIME types', () => {
    expect(fileTypeIcon('image/png')).toBe('image');
    expect(fileTypeIcon('image/jpeg')).toBe('image');
    expect(fileTypeIcon('image/svg+xml')).toBe('image');
  });

  it('returns "file-text" for PDF and text types', () => {
    expect(fileTypeIcon('application/pdf')).toBe('file-text');
    expect(fileTypeIcon('text/plain')).toBe('file-text');
    expect(fileTypeIcon('text/html')).toBe('file-text');
  });

  it('returns generic "file" for unknown types', () => {
    expect(fileTypeIcon('application/zip')).toBe('file');
    expect(fileTypeIcon('application/octet-stream')).toBe('file');
  });
});

// ── fmtK ──────────────────────────────────────────────────────────────────

describe('fmtK', () => {
  it('formats small numbers as-is', () => {
    expect(fmtK(0)).toBe('0');
    expect(fmtK(500)).toBe('500');
    expect(fmtK(999)).toBe('999');
  });

  it('formats 1000+ as K suffix', () => {
    expect(fmtK(1000)).toBe('1.0K');
    expect(fmtK(1500)).toBe('1.5K');
    expect(fmtK(128000)).toBe('128.0K');
  });
});

// ── estimateContextBreakdown ──────────────────────────────────────────────

describe('estimateContextBreakdown', () => {
  it('returns zero percentages for zero usage', () => {
    const result = estimateContextBreakdown({
      sessionTokensUsed: 0,
      modelContextLimit: 128000,
      sessionInputTokens: 0,
      sessionOutputTokens: 0,
      sessionToolResultTokens: 0,
      messages: [],
    });
    expect(result.total).toBe(0);
    expect(result.pct).toBe(0);
    expect(result.system).toBe(0);
  });

  it('computes breakdown correctly with usage data', () => {
    const result = estimateContextBreakdown({
      sessionTokensUsed: 5000,
      modelContextLimit: 128000,
      sessionInputTokens: 3000,
      sessionOutputTokens: 2000,
      sessionToolResultTokens: 500,
      messages: [
        { content: 'Hello there' }, // ~7 chars → ceil(7/4) + 4 = 6 tokens
        { content: 'I can help with that, let me explain in detail.' }, // ~48 chars → ceil(48/4) + 4 = 16 tokens
      ],
    });
    expect(result.total).toBe(5000);
    expect(result.limit).toBe(128000);
    expect(result.pct).toBeCloseTo(3.90625, 2);
    expect(result.output).toBe(2000);
    expect(result.toolResults).toBe(500);
    // system = input - msgs - toolResults = 3000 - 22 - 500 = 2478
    expect(result.system).toBeGreaterThan(0);
  });

  it('caps percentage at 100 when over limit', () => {
    const result = estimateContextBreakdown({
      sessionTokensUsed: 200000,
      modelContextLimit: 128000,
      sessionInputTokens: 150000,
      sessionOutputTokens: 50000,
      sessionToolResultTokens: 0,
      messages: [],
    });
    expect(result.pct).toBe(100);
  });

  it('handles zero context limit gracefully', () => {
    const result = estimateContextBreakdown({
      sessionTokensUsed: 100,
      modelContextLimit: 0,
      sessionInputTokens: 50,
      sessionOutputTokens: 50,
      sessionToolResultTokens: 0,
      messages: [],
    });
    expect(result.pct).toBe(0);
  });
});

// ── fileToBase64 ──────────────────────────────────────────────────────────

describe('fileToBase64', () => {
  it('resolves with base64 content from a file', async () => {
    const file = new File(['hello'], 'test.txt', { type: 'text/plain' });
    const result = await fileToBase64(file);
    expect(typeof result).toBe('string');
    expect(result.length).toBeGreaterThan(0);
  });

  it('strips data URI prefix — returns only the base64 part', async () => {
    const file = new File(['abc'], 'test.txt', { type: 'text/plain' });
    const result = await fileToBase64(file);
    // Should NOT contain the data: prefix
    expect(result).not.toContain('data:');
  });

  it('handles empty file', async () => {
    const file = new File([], 'empty.txt', { type: 'text/plain' });
    const result = await fileToBase64(file);
    expect(typeof result).toBe('string');
  });
});

// ── generateSessionLabel — edge cases ─────────────────────────────────

describe('generateSessionLabel — edge cases', () => {
  it('keeps message exactly 50 chars without ellipsis', () => {
    const exact = 'A'.repeat(50);
    const label = generateSessionLabel(exact);
    expect(label).toBe(exact);
    expect(label.length).toBe(50);
  });

  it('truncates message of 51 chars', () => {
    const over = 'word '.repeat(11); // 55 chars
    const label = generateSessionLabel(over);
    expect(label.length).toBeLessThanOrEqual(50);
    expect(label.endsWith('…')).toBe(true);
  });

  it('handles message with only slash command', () => {
    expect(generateSessionLabel('/model')).toBe('New chat');
  });

  it('handles backtick fences', () => {
    expect(generateSessionLabel('```code```')).toBe('code');
  });
});

// ── extractContent — edge cases ───────────────────────────────────────

describe('extractContent — edge cases', () => {
  it('returns empty string for empty array', () => {
    expect(extractContent([])).toBe('');
  });

  it('filters out blocks where text is not a string', () => {
    const blocks = [{ type: 'text', text: 123 }];
    expect(extractContent(blocks)).toBe('');
  });
});

// ── findLastIndex — edge cases ────────────────────────────────────────

describe('findLastIndex — edge cases', () => {
  it('returns last index when all elements match', () => {
    expect(findLastIndex([1, 1, 1], () => true)).toBe(2);
  });

  it('works with single-element array that matches', () => {
    expect(findLastIndex([42], (n) => n === 42)).toBe(0);
  });
});

// ── fileTypeIcon — edge cases ─────────────────────────────────────────

describe('fileTypeIcon — edge cases', () => {
  it('returns "image" for image/webp', () => {
    expect(fileTypeIcon('image/webp')).toBe('image');
  });

  it('returns "file-text" for text/csv', () => {
    expect(fileTypeIcon('text/csv')).toBe('file-text');
  });

  it('returns "file" for empty string', () => {
    expect(fileTypeIcon('')).toBe('file');
  });
});

// ── fmtK — edge cases ────────────────────────────────────────────────

describe('fmtK — edge cases', () => {
  it('formats boundary 999 as plain number', () => {
    expect(fmtK(999)).toBe('999');
  });

  it('handles negative numbers', () => {
    expect(fmtK(-500)).toBe('-500');
  });

  it('handles very large numbers', () => {
    expect(fmtK(1_000_000)).toBe('1000.0K');
  });
});

// ── estimateContextBreakdown — edge cases ─────────────────────────────

describe('estimateContextBreakdown — edge cases', () => {
  it('handles messages with no content field', () => {
    const result = estimateContextBreakdown({
      sessionTokensUsed: 100,
      modelContextLimit: 1000,
      sessionInputTokens: 100,
      sessionOutputTokens: 0,
      sessionToolResultTokens: 0,
      messages: [{ content: undefined } as any, {}],
    });
    // Each msg with no content: ceil(0/4) + 4 = 4
    expect(result.messages).toBe(8);
  });

  it('percentage fields are non-negative', () => {
    const result = estimateContextBreakdown({
      sessionTokensUsed: 500,
      modelContextLimit: 1000,
      sessionInputTokens: 300,
      sessionOutputTokens: 200,
      sessionToolResultTokens: 100,
      messages: [{ content: 'Hi' }],
    });
    expect(result.systemPct).toBeGreaterThanOrEqual(0);
    expect(result.messagesPct).toBeGreaterThanOrEqual(0);
    expect(result.toolResultsPct).toBeGreaterThanOrEqual(0);
    expect(result.outputPct).toBeGreaterThanOrEqual(0);
  });
});
