import { describe, it, expect } from 'vitest';
import { specialtyIcon, messageKindLabel, formatTime } from './atoms';

// ── specialtyIcon ──────────────────────────────────────────────────────

describe('specialtyIcon', () => {
  it('returns code icon for coder', () => {
    expect(specialtyIcon('coder')).toContain('code');
  });

  it('returns search icon for researcher', () => {
    expect(specialtyIcon('researcher')).toContain('search');
  });

  it('falls back to smart_toy for unknown', () => {
    expect(specialtyIcon('unknown')).toContain('smart_toy');
  });

  it('returns HTML span', () => {
    expect(specialtyIcon('coder')).toMatch(/<span.*>code<\/span>/);
  });
});

// ── messageKindLabel ───────────────────────────────────────────────────

describe('messageKindLabel', () => {
  it('maps known kinds', () => {
    expect(messageKindLabel('delegation')).toBe('Delegation');
    expect(messageKindLabel('progress')).toBe('Progress');
    expect(messageKindLabel('result')).toBe('Result');
    expect(messageKindLabel('error')).toBe('Error');
  });

  it('returns raw kind for unknown', () => {
    expect(messageKindLabel('custom')).toBe('custom');
  });
});

// ── formatTime ─────────────────────────────────────────────────────────

describe('formatTime', () => {
  it('formats valid ISO date', () => {
    const result = formatTime('2024-01-15T10:30:45Z');
    expect(result).toBeTruthy();
    expect(typeof result).toBe('string');
  });

  it('returns something for invalid date (no crash)', () => {
    expect(typeof formatTime('not-a-date')).toBe('string');
  });
});
