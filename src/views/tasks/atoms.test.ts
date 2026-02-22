import { describe, it, expect } from 'vitest';
import { COLUMNS } from './atoms';

// ── COLUMNS ────────────────────────────────────────────────────────────

describe('COLUMNS', () => {
  it('has 6 columns', () => {
    expect(COLUMNS).toHaveLength(6);
  });

  it('starts with inbox', () => {
    expect(COLUMNS[0]).toBe('inbox');
  });

  it('ends with done', () => {
    expect(COLUMNS[COLUMNS.length - 1]).toBe('done');
  });

  it('contains all expected statuses', () => {
    expect(COLUMNS).toContain('inbox');
    expect(COLUMNS).toContain('assigned');
    expect(COLUMNS).toContain('in_progress');
    expect(COLUMNS).toContain('review');
    expect(COLUMNS).toContain('blocked');
    expect(COLUMNS).toContain('done');
  });

  it('has no duplicates', () => {
    const unique = new Set(COLUMNS);
    expect(unique.size).toBe(COLUMNS.length);
  });

  it('follows expected workflow order', () => {
    const inboxIdx = COLUMNS.indexOf('inbox');
    const assignedIdx = COLUMNS.indexOf('assigned');
    const progressIdx = COLUMNS.indexOf('in_progress');
    const doneIdx = COLUMNS.indexOf('done');
    expect(inboxIdx).toBeLessThan(assignedIdx);
    expect(assignedIdx).toBeLessThan(progressIdx);
    expect(progressIdx).toBeLessThan(doneIdx);
  });
});
