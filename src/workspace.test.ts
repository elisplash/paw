import { describe, it, expect } from 'vitest';
import { generateFindingId, parseAgentResponse } from './workspace';
import type { ResearchSource } from './workspace';

// ── generateFindingId ──────────────────────────────────────────────────

describe('generateFindingId', () => {
  it('returns a non-empty string', () => {
    const id = generateFindingId();
    expect(typeof id).toBe('string');
    expect(id.length).toBeGreaterThan(0);
  });

  it('generates unique IDs', () => {
    const ids = new Set(Array.from({ length: 100 }, () => generateFindingId()));
    expect(ids.size).toBe(100);
  });

  it('contains a hyphen separator', () => {
    const id = generateFindingId();
    expect(id).toContain('-');
  });
});

// ── parseAgentResponse ─────────────────────────────────────────────────

const sampleSources: ResearchSource[] = [
  {
    url: 'https://example.com/article',
    title: 'Example Article',
    credibility: 4,
    extractedAt: new Date().toISOString(),
    snippets: ['key snippet'],
  },
];

describe('parseAgentResponse', () => {
  it('extracts query from input', () => {
    const result = parseAgentResponse('test query', 'Some content here', []);
    expect(result.query).toBe('test query');
  });

  it('preserves raw content', () => {
    const raw = 'The full raw content body';
    const result = parseAgentResponse('q', raw, []);
    expect(result.content).toBe(raw);
  });

  it('passes through sources', () => {
    const result = parseAgentResponse('q', 'content', sampleSources);
    expect(result.sources).toHaveLength(1);
    expect(result.sources[0].url).toBe('https://example.com/article');
  });

  it('extracts bold bullet key points', () => {
    const content = `# Research
- **Key Finding One**: This is important and long enough to qualify as a key point.
- **Key Finding Two**: Another important piece of information worth noting.
Some regular paragraph text.`;
    const result = parseAgentResponse('research', content, []);
    expect(result.keyPoints.length).toBeGreaterThanOrEqual(1);
  });

  it('extracts numbered bold key points', () => {
    const content = `1. **First important finding**: something that is meaningful and detailed enough.
2. **Second important finding**: another piece of research data to note.`;
    const result = parseAgentResponse('research', content, []);
    expect(result.keyPoints.length).toBeGreaterThanOrEqual(1);
  });

  it('limits key points to 5', () => {
    const lines = Array.from(
      { length: 10 },
      (_, i) =>
        `- **Point ${i}**: This is a sufficiently long key point that meets the length threshold.`,
    ).join('\n');
    const result = parseAgentResponse('q', lines, []);
    expect(result.keyPoints.length).toBeLessThanOrEqual(5);
  });

  it('extracts summary from first qualifying paragraph', () => {
    const content = `# Title
This is a paragraph that qualifies as a summary because it is longer than fifty characters and less than five hundred.`;
    const result = parseAgentResponse('q', content, []);
    expect(result.summary).toBeTruthy();
    expect(result.summary!.length).toBeGreaterThan(50);
  });

  it('returns undefined summary when no qualifying paragraph', () => {
    const content = '# Just a header\n- bullet one\n- bullet two';
    const result = parseAgentResponse('q', content, []);
    expect(result.summary).toBeUndefined();
  });

  it('extracts tags like performance and security', () => {
    const content =
      'This article covers performance optimization and security best practices for web applications.';
    const result = parseAgentResponse('q', content, []);
    expect(result.tags).toContain('performance');
    expect(result.tags).toContain('security');
  });

  it('deduplicates tags', () => {
    const content = 'Performance is key. Focus on performance optimization for best performance.';
    const result = parseAgentResponse('q', content, []);
    const perfCount = result.tags.filter((t) => t === 'performance').length;
    expect(perfCount).toBe(1);
  });

  it('limits tags to 5', () => {
    const content =
      'performance optimization security best practice tip warning gotcha security performance optimization tip best practice warning gotcha';
    const result = parseAgentResponse('q', content, []);
    expect(result.tags.length).toBeLessThanOrEqual(5);
  });

  it('returns empty tags for content with no matching patterns', () => {
    const result = parseAgentResponse('q', 'Nothing special here, just regular text.', []);
    expect(result.tags).toHaveLength(0);
  });

  it('handles empty content', () => {
    const result = parseAgentResponse('q', '', []);
    expect(result.keyPoints).toHaveLength(0);
    expect(result.summary).toBeUndefined();
    expect(result.tags).toHaveLength(0);
  });
});
