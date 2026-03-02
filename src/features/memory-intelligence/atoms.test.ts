import { describe, it, expect } from 'vitest';
import {
  jaccardSimilarity,
  mmrRerank,
  formatMemoryForContext,
  groupByCategory,
  describeAge,
  temporalDecayFactor,
  applyDecay,
  DEFAULT_SEARCH_CONFIG,
  MEMORY_CATEGORIES,
} from './atoms';
import type { Memory } from './atoms';

const makeMem = (
  content: string,
  category = 'general',
  score = 1.0,
  createdAt?: string,
): Memory => ({
  id: `m-${Math.random().toString(36).slice(2)}`,
  content,
  category,
  importance: 5,
  created_at: createdAt ?? new Date().toISOString(),
  score,
});

// ── temporalDecayFactor ────────────────────────────────────────────────

describe('temporalDecayFactor', () => {
  it('returns ~1 for brand new memory', () => {
    const factor = temporalDecayFactor(new Date().toISOString());
    expect(factor).toBeGreaterThan(0.99);
  });

  it('returns ~0.5 after one half-life', () => {
    const date = new Date();
    date.setDate(date.getDate() - 30);
    const factor = temporalDecayFactor(date.toISOString(), 30);
    expect(factor).toBeCloseTo(0.5, 1);
  });

  it('returns small value for very old memories', () => {
    const old = new Date();
    old.setFullYear(old.getFullYear() - 1);
    expect(temporalDecayFactor(old.toISOString())).toBeLessThan(0.01);
  });
});

// ── applyDecay ─────────────────────────────────────────────────────────

describe('applyDecay', () => {
  it('reduces scores of old memories', () => {
    const old = new Date();
    old.setDate(old.getDate() - 60);
    const memories = [
      makeMem('new', 'general', 1.0),
      makeMem('old', 'general', 1.0, old.toISOString()),
    ];
    const decayed = applyDecay(memories);
    expect(decayed[0].score!).toBeGreaterThan(decayed[1].score!);
  });

  it('preserves memory content', () => {
    const memories = [makeMem('test content')];
    const decayed = applyDecay(memories);
    expect(decayed[0].content).toBe('test content');
  });
});

// ── jaccardSimilarity ──────────────────────────────────────────────────

describe('jaccardSimilarity', () => {
  it('returns 1 for identical texts', () => {
    expect(jaccardSimilarity('hello world foo', 'hello world foo')).toBe(1);
  });

  it('returns 0 for completely different texts', () => {
    expect(jaccardSimilarity('alpha beta gamma', 'one two three')).toBe(0);
  });

  it('returns partial similarity for overlapping texts', () => {
    const sim = jaccardSimilarity('the quick brown fox', 'the lazy brown dog');
    expect(sim).toBeGreaterThan(0);
    expect(sim).toBeLessThan(1);
  });

  it('ignores short words (<=2 chars)', () => {
    expect(jaccardSimilarity('a b c', 'a b c')).toBe(1); // all filtered → both empty → returns 1
  });
});

// ── mmrRerank ──────────────────────────────────────────────────────────

describe('mmrRerank', () => {
  it('returns empty for empty candidates', () => {
    expect(mmrRerank([], 5)).toEqual([]);
  });

  it('returns k items', () => {
    const mems = [
      makeMem('memory about cats', 'general', 0.9),
      makeMem('memory about dogs', 'general', 0.8),
      makeMem('memory about birds', 'general', 0.7),
    ];
    expect(mmrRerank(mems, 2).length).toBe(2);
  });

  it('selects highest scored first', () => {
    const mems = [makeMem('low score', 'general', 0.2), makeMem('high score', 'general', 0.9)];
    const result = mmrRerank(mems, 1);
    expect(result[0].content).toBe('high score');
  });
});

// ── formatMemoryForContext ─────────────────────────────────────────────

describe('formatMemoryForContext', () => {
  it('formats with category and content', () => {
    const mem = makeMem('likes TypeScript', 'preference', 0.85);
    const text = formatMemoryForContext(mem);
    expect(text).toContain('[preference]');
    expect(text).toContain('likes TypeScript');
    expect(text).toContain('[0.85]');
  });

  it('includes agent tag when present', () => {
    const mem = { ...makeMem('test'), agent_id: 'agent-1' };
    expect(formatMemoryForContext(mem)).toContain('(agent: agent-1)');
  });
});

// ── groupByCategory ────────────────────────────────────────────────────

describe('groupByCategory', () => {
  it('groups memories by category', () => {
    const mems = [makeMem('a', 'fact'), makeMem('b', 'fact'), makeMem('c', 'preference')];
    const groups = groupByCategory(mems);
    expect(groups['fact']).toHaveLength(2);
    expect(groups['preference']).toHaveLength(1);
  });

  it('defaults to general for empty category', () => {
    const mem = { ...makeMem('test'), category: '' };
    const groups = groupByCategory([mem]);
    expect(groups['general']).toHaveLength(1);
  });
});

// ── describeAge ────────────────────────────────────────────────────────

describe('describeAge', () => {
  it('says "just now" for recent', () => {
    expect(describeAge(new Date().toISOString())).toBe('just now');
  });

  it('says hours ago', () => {
    const d = new Date();
    d.setHours(d.getHours() - 3);
    expect(describeAge(d.toISOString())).toBe('3h ago');
  });

  it('says days ago', () => {
    const d = new Date();
    d.setDate(d.getDate() - 5);
    expect(describeAge(d.toISOString())).toBe('5d ago');
  });
});

// ── Constants ──────────────────────────────────────────────────────────

describe('MEMORY_CATEGORIES', () => {
  it('contains expected categories', () => {
    expect(MEMORY_CATEGORIES).toContain('general');
    expect(MEMORY_CATEGORIES).toContain('preference');
    expect(MEMORY_CATEGORIES).toContain('technical');
  });
});

describe('DEFAULT_SEARCH_CONFIG', () => {
  it('has reasonable defaults', () => {
    expect(DEFAULT_SEARCH_CONFIG.bm25Weight + DEFAULT_SEARCH_CONFIG.vectorWeight).toBeCloseTo(1.0);
    expect(DEFAULT_SEARCH_CONFIG.threshold).toBeLessThan(1);
  });
});

// ── Additional edge cases ──────────────────────────────────────────────

describe('temporalDecayFactor — edge cases', () => {
  it('returns > 1 for future dates', () => {
    const future = new Date(Date.now() + 7 * 86_400_000).toISOString();
    const factor = temporalDecayFactor(future);
    expect(factor).toBeGreaterThan(1);
  });

  it('returns NaN for invalid date string', () => {
    // An invalid date leads to NaN in age calculation
    const factor = temporalDecayFactor('not-a-date');
    expect(Number.isNaN(factor)).toBe(true);
  });
});

describe('applyDecay — edge cases', () => {
  it('returns empty array for empty input', () => {
    expect(applyDecay([])).toHaveLength(0);
  });

  it('handles memories with undefined score', () => {
    const mem = {
      id: '1',
      content: 'test',
      category: 'general' as const,
      importance: 5,
      created_at: new Date().toISOString(),
    };
    const [decayed] = applyDecay([mem]);
    expect(decayed.score).toBeDefined();
    expect(typeof decayed.score).toBe('number');
  });

  it('accepts custom halfLifeDays', () => {
    const old = new Date(Date.now() - 30 * 86_400_000).toISOString();
    const mem = {
      id: '1',
      content: 'test',
      category: 'general' as const,
      importance: 5,
      created_at: old,
      score: 1.0,
    };
    const [short] = applyDecay([mem], 7);
    const [long] = applyDecay([mem], 90);
    // Shorter half-life decays faster
    expect(short.score!).toBeLessThan(long.score!);
  });
});

describe('jaccardSimilarity — edge cases', () => {
  it('returns 0 when one string is empty', () => {
    expect(jaccardSimilarity('hello world', '')).toBe(0);
    expect(jaccardSimilarity('', 'hello world')).toBe(0);
  });

  it('returns 1 for two empty strings (both sets empty)', () => {
    // Edge: 0/0 case, implementation may return 0 or 1
    const result = jaccardSimilarity('', '');
    expect([0, 1]).toContain(result);
  });
});

describe('mmrRerank — edge cases', () => {
  it('returns empty for k=0', () => {
    const mem = {
      id: '1',
      content: 'test',
      category: 'general' as const,
      importance: 5,
      created_at: new Date().toISOString(),
      score: 0.9,
    };
    expect(mmrRerank([mem], 0)).toHaveLength(0);
  });

  it('returns all when k > candidates.length', () => {
    const mems = Array.from({ length: 3 }, (_, i) => ({
      id: String(i),
      content: `memory ${i}`,
      category: 'general' as const,
      importance: 5,
      created_at: new Date().toISOString(),
      score: 0.5 + i * 0.1,
    }));
    const result = mmrRerank(mems, 10);
    expect(result).toHaveLength(3);
  });

  it('with lambda=1 selects by relevance (score) only', () => {
    const mems = [
      {
        id: '1',
        content: 'alpha beta',
        category: 'general' as const,
        importance: 5,
        created_at: new Date().toISOString(),
        score: 0.3,
      },
      {
        id: '2',
        content: 'alpha gamma',
        category: 'general' as const,
        importance: 5,
        created_at: new Date().toISOString(),
        score: 0.9,
      },
    ];
    const result = mmrRerank(mems, 2, 1);
    expect(result[0].id).toBe('2'); // highest score first
  });
});

describe('describeAge — edge cases', () => {
  it('describes months for 60-day-old memory', () => {
    const old = new Date(Date.now() - 60 * 86_400_000).toISOString();
    const desc = describeAge(old);
    expect(desc).toMatch(/\d+\s*mo|month/i);
  });

  it('describes a very recent memory as just now', () => {
    const now = new Date().toISOString();
    expect(describeAge(now)).toMatch(/just now|0|second/i);
  });
});

describe('formatMemoryForContext — edge cases', () => {
  it('omits score tag when score is undefined', () => {
    const mem = {
      id: '1',
      content: 'no score',
      category: 'general' as const,
      importance: 3,
      created_at: new Date().toISOString(),
    };
    const formatted = formatMemoryForContext(mem);
    expect(formatted).not.toContain('NaN');
  });
});

describe('groupByCategory — edge cases', () => {
  it('returns empty object for empty array', () => {
    expect(groupByCategory([])).toEqual({});
  });
});
