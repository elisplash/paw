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

// ── Deep memory algorithm tests ────────────────────────────────────────

describe('mmrRerank — diversity vs relevance', () => {
  it('with lambda=0 maximizes diversity (least similar first after top)', () => {
    // Three memories: two very similar, one different
    const mems = [
      makeMem('the quick brown fox jumps over the lazy dog', 'general', 0.9),
      makeMem('the quick brown fox runs over the lazy dog', 'general', 0.85),
      makeMem('quantum computing advances in modern physics', 'technical', 0.8),
    ];
    const result = mmrRerank(mems, 3, 0);
    // First is still highest score, but second should be the diverse one
    expect(result[0].content).toContain('jumps');
    expect(result[1].content).toContain('quantum');
  });

  it('preserves diversity-correctness with all identical content', () => {
    const mems = [
      makeMem('identical content here', 'general', 0.9),
      makeMem('identical content here', 'general', 0.8),
      makeMem('identical content here', 'general', 0.7),
    ];
    const result = mmrRerank(mems, 3);
    // All selected despite identical content
    expect(result).toHaveLength(3);
  });

  it('single candidate returns it regardless of lambda', () => {
    const mem = makeMem('only one', 'general', 0.5);
    expect(mmrRerank([mem], 1, 0)).toHaveLength(1);
    expect(mmrRerank([mem], 1, 1)).toHaveLength(1);
    expect(mmrRerank([mem], 1, 0.5)[0].content).toBe('only one');
  });
});

describe('applyDecay — ordering guarantees', () => {
  it('newer memories decay less than older ones', () => {
    const now = new Date();
    const oneDay = new Date(now.getTime() - 86_400_000);
    const oneWeek = new Date(now.getTime() - 7 * 86_400_000);
    const oneMonth = new Date(now.getTime() - 30 * 86_400_000);

    const mems = [
      makeMem('recent', 'general', 1.0, now.toISOString()),
      makeMem('day old', 'general', 1.0, oneDay.toISOString()),
      makeMem('week old', 'general', 1.0, oneWeek.toISOString()),
      makeMem('month old', 'general', 1.0, oneMonth.toISOString()),
    ];
    const decayed = applyDecay(mems);
    // Scores should be strictly decreasing by age
    for (let i = 0; i < decayed.length - 1; i++) {
      expect(decayed[i].score!).toBeGreaterThan(decayed[i + 1].score!);
    }
  });

  it('preserves all memory fields besides score', () => {
    const mem = makeMem('important fact', 'fact', 0.8);
    mem.agent_id = 'agent-42';
    const [decayed] = applyDecay([mem]);
    expect(decayed.content).toBe('important fact');
    expect(decayed.category).toBe('fact');
    expect(decayed.agent_id).toBe('agent-42');
    expect(decayed.id).toBe(mem.id);
    expect(decayed.importance).toBe(5);
  });
});

describe('temporalDecayFactor — mathematical properties', () => {
  it('decay is monotonically decreasing with age', () => {
    const factors = [1, 7, 14, 30, 60, 90, 180, 365].map((days) => {
      const d = new Date(Date.now() - days * 86_400_000).toISOString();
      return temporalDecayFactor(d);
    });
    for (let i = 0; i < factors.length - 1; i++) {
      expect(factors[i]).toBeGreaterThan(factors[i + 1]);
    }
  });

  it('two half-lives gives ~0.25', () => {
    const d = new Date(Date.now() - 60 * 86_400_000).toISOString();
    expect(temporalDecayFactor(d, 30)).toBeCloseTo(0.25, 1);
  });

  it('custom half-life of 1 day decays fast', () => {
    const d = new Date(Date.now() - 3 * 86_400_000).toISOString();
    const factor = temporalDecayFactor(d, 1);
    expect(factor).toBeLessThan(0.15); // 2^(-3) = 0.125
  });
});

describe('jaccardSimilarity — word-level correctness', () => {
  it('ignores case differences', () => {
    expect(jaccardSimilarity('Hello World Foo', 'hello world foo')).toBe(1);
  });

  it('ignores words with 2 or fewer characters', () => {
    // 'the' has 3 chars → included; 'a' and 'is' are filtered
    const sim = jaccardSimilarity('the dog is big', 'the cat is big');
    // Sets: {the, dog, big} vs {the, cat, big} → intersection=2, union=4 → 0.5
    expect(sim).toBeCloseTo(0.5, 2);
  });

  it('handles strings with only short words', () => {
    // All words ≤2 chars → both sets empty → returns 1
    expect(jaccardSimilarity('a b c', 'x y z')).toBe(1);
  });

  it('returns correct value for known overlap', () => {
    // Words: {machine, learning, great} vs {machine, learning, hard}
    // intersection=2, union=4 → 0.5
    const sim = jaccardSimilarity('machine learning is great', 'machine learning is hard');
    expect(sim).toBeCloseTo(0.5, 2);
  });
});

describe('describeAge — full range coverage', () => {
  it('shows months for 90-day-old memory', () => {
    const d = new Date(Date.now() - 90 * 86_400_000).toISOString();
    expect(describeAge(d)).toBe('3mo ago');
  });

  it('shows months for year-old memory', () => {
    const d = new Date(Date.now() - 365 * 86_400_000).toISOString();
    expect(describeAge(d)).toBe('12mo ago');
  });

  it('boundary: exactly 24 hours shows 1d ago', () => {
    const d = new Date(Date.now() - 24 * 60 * 60 * 1000).toISOString();
    expect(describeAge(d)).toBe('1d ago');
  });

  it('boundary: exactly 30 days shows 1mo ago', () => {
    const d = new Date(Date.now() - 30 * 86_400_000).toISOString();
    expect(describeAge(d)).toBe('1mo ago');
  });

  it('23 hours shows hours', () => {
    const d = new Date(Date.now() - 23 * 60 * 60 * 1000).toISOString();
    expect(describeAge(d)).toBe('23h ago');
  });
});

describe('formatMemoryForContext — formatting correctness', () => {
  it('includes dash prefix', () => {
    const mem = makeMem('test', 'general', 0.5);
    expect(formatMemoryForContext(mem)).toMatch(/^- /);
  });

  it('formats score to 2 decimal places', () => {
    const mem = makeMem('test', 'general', 0.12345);
    expect(formatMemoryForContext(mem)).toContain('[0.12]');
  });

  it('shows category in brackets', () => {
    const mem = makeMem('test', 'technical', 0.5);
    expect(formatMemoryForContext(mem)).toContain('[technical]');
  });

  it('omits agent tag when no agent_id', () => {
    const mem = makeMem('test', 'general', 0.5);
    expect(formatMemoryForContext(mem)).not.toContain('(agent:');
  });
});

describe('groupByCategory — grouping correctness', () => {
  it('handles all 18 memory categories', () => {
    const mems = MEMORY_CATEGORIES.map((cat) => makeMem(`content for ${cat}`, cat));
    const groups = groupByCategory(mems);
    expect(Object.keys(groups)).toHaveLength(MEMORY_CATEGORIES.length);
    for (const cat of MEMORY_CATEGORIES) {
      expect(groups[cat]).toHaveLength(1);
    }
  });

  it('preserves memory references in groups', () => {
    const mem = makeMem('unique', 'insight');
    const groups = groupByCategory([mem]);
    expect(groups['insight'][0]).toBe(mem);
  });
});

describe('MEMORY_CATEGORIES — completeness', () => {
  it('has exactly 18 categories', () => {
    expect(MEMORY_CATEGORIES).toHaveLength(18);
  });

  it('contains all expected categories', () => {
    const expected = [
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
    ];
    for (const cat of expected) {
      expect(MEMORY_CATEGORIES).toContain(cat);
    }
  });

  it('has no duplicates', () => {
    const unique = new Set(MEMORY_CATEGORIES);
    expect(unique.size).toBe(MEMORY_CATEGORIES.length);
  });
});

describe('DEFAULT_SEARCH_CONFIG — constraints', () => {
  it('weights sum to 1.0', () => {
    expect(DEFAULT_SEARCH_CONFIG.bm25Weight + DEFAULT_SEARCH_CONFIG.vectorWeight).toBeCloseTo(1.0);
  });

  it('mmrLambda is between 0 and 1', () => {
    expect(DEFAULT_SEARCH_CONFIG.mmrLambda).toBeGreaterThanOrEqual(0);
    expect(DEFAULT_SEARCH_CONFIG.mmrLambda).toBeLessThanOrEqual(1);
  });

  it('threshold is between 0 and 1', () => {
    expect(DEFAULT_SEARCH_CONFIG.threshold).toBeGreaterThan(0);
    expect(DEFAULT_SEARCH_CONFIG.threshold).toBeLessThan(1);
  });

  it('decayHalfLifeDays is positive', () => {
    expect(DEFAULT_SEARCH_CONFIG.decayHalfLifeDays).toBeGreaterThan(0);
  });
});
