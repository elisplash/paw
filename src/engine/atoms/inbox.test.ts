// src/engine/atoms/inbox.test.ts
// Unit tests for inbox atom layer — pure functions.

import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  sortConversations,
  filterConversations,
  filterByTab,
  truncatePreview,
  formatRelativeTime,
  findConversation,
  updateConversation,
  removeConversation,
  createInboxState,
  type ConversationEntry,
} from './inbox';

// ── Factory ──────────────────────────────────────────────────────────────

function entry(overrides: Partial<ConversationEntry> = {}): ConversationEntry {
  return {
    sessionKey: 'sess_1',
    agentId: 'aria',
    agentName: 'Aria',
    agentAvatar: '🐕',
    agentColor: '#ff0000',
    lastMessage: 'Hello world',
    lastRole: 'assistant',
    lastTs: 1000,
    unread: 0,
    label: 'Test chat',
    isStreaming: false,
    kind: 'direct',
    pinned: false,
    ...overrides,
  };
}

// ── sortConversations ────────────────────────────────────────────────────

describe('sortConversations', () => {
  it('sorts by lastTs descending', () => {
    const entries = [
      entry({ sessionKey: 'a', lastTs: 100 }),
      entry({ sessionKey: 'b', lastTs: 300 }),
      entry({ sessionKey: 'c', lastTs: 200 }),
    ];
    const sorted = sortConversations(entries);
    expect(sorted.map((e) => e.sessionKey)).toEqual(['b', 'c', 'a']);
  });

  it('pinned entries come first', () => {
    const entries = [
      entry({ sessionKey: 'a', lastTs: 300 }),
      entry({ sessionKey: 'b', lastTs: 100, pinned: true }),
      entry({ sessionKey: 'c', lastTs: 200 }),
    ];
    const sorted = sortConversations(entries);
    expect(sorted[0].sessionKey).toBe('b');
  });

  it('does not mutate the original array', () => {
    const entries = [entry({ lastTs: 2 }), entry({ lastTs: 1 })];
    const sorted = sortConversations(entries);
    expect(sorted).not.toBe(entries);
  });
});

// ── filterConversations ──────────────────────────────────────────────────

describe('filterConversations', () => {
  const entries = [
    entry({ agentName: 'Aria', label: 'Code review', lastMessage: 'Done' }),
    entry({ agentName: 'Scout', label: 'Research', lastMessage: 'Found results' }),
  ];

  it('matches by agent name', () => {
    expect(filterConversations(entries, 'aria')).toHaveLength(1);
  });

  it('matches by label', () => {
    expect(filterConversations(entries, 'research')).toHaveLength(1);
  });

  it('matches by last message', () => {
    expect(filterConversations(entries, 'found')).toHaveLength(1);
  });

  it('returns all for empty query', () => {
    expect(filterConversations(entries, '')).toHaveLength(2);
    expect(filterConversations(entries, '   ')).toHaveLength(2);
  });

  it('is case-insensitive', () => {
    expect(filterConversations(entries, 'ARIA')).toHaveLength(1);
  });
});

// ── filterByTab ──────────────────────────────────────────────────────────

describe('filterByTab', () => {
  const entries = [
    entry({ kind: 'direct', unread: 0 }),
    entry({ kind: 'group', unread: 2 }),
    entry({ kind: 'direct', unread: 1 }),
  ];

  it('"all" returns everything', () => {
    expect(filterByTab(entries, 'all')).toHaveLength(3);
  });

  it('"unread" filters to unread > 0', () => {
    expect(filterByTab(entries, 'unread')).toHaveLength(2);
  });

  it('"agents" filters to direct', () => {
    expect(filterByTab(entries, 'agents')).toHaveLength(2);
  });

  it('"groups" filters to group', () => {
    expect(filterByTab(entries, 'groups')).toHaveLength(1);
  });
});

// ── truncatePreview ──────────────────────────────────────────────────────

describe('truncatePreview', () => {
  it('returns short strings as-is', () => {
    expect(truncatePreview('Hello')).toBe('Hello');
  });

  it('truncates long strings with ellipsis', () => {
    const long = 'This is a very long message that should definitely be truncated at sixty characters';
    const result = truncatePreview(long);
    expect(result.endsWith('…')).toBe(true);
    expect(result.length).toBeLessThanOrEqual(61); // 60 + ellipsis
  });

  it('collapses whitespace', () => {
    expect(truncatePreview('  lots   of   spaces  ')).toBe('lots of spaces');
  });

  it('respects custom maxLen', () => {
    const result = truncatePreview('Hello world this is a test', 10);
    expect(result.length).toBeLessThanOrEqual(11);
  });
});

// ── formatRelativeTime ───────────────────────────────────────────────────

describe('formatRelativeTime', () => {
  const now = new Date('2026-02-27T12:00:00Z').getTime();

  it('returns "now" for timestamps < 60s ago', () => {
    expect(formatRelativeTime(now - 30_000, now)).toBe('now');
  });

  it('returns minutes for < 1 hour', () => {
    expect(formatRelativeTime(now - 5 * 60_000, now)).toBe('5m');
  });

  it('returns hours for < 24 hours', () => {
    expect(formatRelativeTime(now - 3 * 3600_000, now)).toBe('3h');
  });

  it('returns "Yesterday" for yesterday', () => {
    const yesterday = now - 24 * 3600_000;
    expect(formatRelativeTime(yesterday, now)).toBe('Yesterday');
  });

  it('returns month + day for older same-year dates', () => {
    const jan15 = new Date('2026-01-15T10:00:00Z').getTime();
    expect(formatRelativeTime(jan15, now)).toBe('Jan 15');
  });

  it('includes year for previous years', () => {
    const old = new Date('2024-06-01T10:00:00Z').getTime();
    expect(formatRelativeTime(old, now)).toBe('Jun 1, 2024');
  });

  it('returns "now" for future timestamps', () => {
    expect(formatRelativeTime(now + 10_000, now)).toBe('now');
  });
});

// ── findConversation ─────────────────────────────────────────────────────

describe('findConversation', () => {
  it('finds by session key', () => {
    const entries = [entry({ sessionKey: 'a' }), entry({ sessionKey: 'b' })];
    expect(findConversation(entries, 'b')?.sessionKey).toBe('b');
  });

  it('returns undefined when not found', () => {
    expect(findConversation([entry()], 'nope')).toBeUndefined();
  });
});

// ── updateConversation ───────────────────────────────────────────────────

describe('updateConversation', () => {
  it('patches the matching entry', () => {
    const entries = [entry({ sessionKey: 'a', unread: 0 })];
    const updated = updateConversation(entries, 'a', { unread: 3 });
    expect(updated[0].unread).toBe(3);
  });

  it('does not mutate the original', () => {
    const entries = [entry({ sessionKey: 'a', unread: 0 })];
    updateConversation(entries, 'a', { unread: 3 });
    expect(entries[0].unread).toBe(0);
  });

  it('leaves non-matching entries unchanged', () => {
    const entries = [entry({ sessionKey: 'a' }), entry({ sessionKey: 'b', unread: 5 })];
    const updated = updateConversation(entries, 'a', { unread: 1 });
    expect(updated[1].unread).toBe(5);
  });
});

// ── removeConversation ───────────────────────────────────────────────────

describe('removeConversation', () => {
  it('removes matching entry', () => {
    const entries = [entry({ sessionKey: 'a' }), entry({ sessionKey: 'b' })];
    const result = removeConversation(entries, 'a');
    expect(result).toHaveLength(1);
    expect(result[0].sessionKey).toBe('b');
  });

  it('returns same-length array if not found', () => {
    const entries = [entry({ sessionKey: 'a' })];
    expect(removeConversation(entries, 'nope')).toHaveLength(1);
  });
});

// ── createInboxState ─────────────────────────────────────────────────────

describe('createInboxState', () => {
  beforeEach(() => {
    vi.stubGlobal('localStorage', {
      getItem: () => null,
      setItem: () => {},
      removeItem: () => {},
    });
  });

  it('creates a default inbox state', () => {
    const state = createInboxState();
    expect(state.conversations).toEqual([]);
    expect(state.activeSessionKey).toBeNull();
    expect(state.searchQuery).toBe('');
    expect(state.sidebarOpen).toBe(true);
    expect(state.filter).toBe('all');
  });
});
