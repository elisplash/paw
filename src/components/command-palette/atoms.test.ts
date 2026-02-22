import { describe, it, expect } from 'vitest';
import { buildPaletteItems, filterPaletteItems, clampIndex, type AgentInfo } from './atoms';

describe('buildPaletteItems', () => {
  it('returns view items when no agents provided', () => {
    const items = buildPaletteItems([]);
    expect(items.length).toBeGreaterThan(0);
    expect(items.every((i) => i.kind === 'view')).toBe(true);
    expect(items.some((i) => i.label === 'Today')).toBe(true);
    expect(items.some((i) => i.label === 'Settings')).toBe(true);
  });

  it('includes agent items before view items', () => {
    const agents: AgentInfo[] = [
      { id: 'a1', name: 'Alice', avatar: '🧑' },
      { id: 'a2', name: 'Bob', avatar: '🤠' },
    ];
    const items = buildPaletteItems(agents);
    const agentItems = items.filter((i) => i.kind === 'agent');
    const viewItems = items.filter((i) => i.kind === 'view');
    expect(agentItems).toHaveLength(2);
    expect(viewItems.length).toBeGreaterThan(0);
    // Agents come first
    expect(items[0].kind).toBe('agent');
    expect(items[0].label).toBe('Alice');
    expect(items[1].label).toBe('Bob');
  });

  it('agent items have correct id and payload', () => {
    const items = buildPaletteItems([{ id: 'x1', name: 'Xavier', avatar: '🎸' }]);
    const agent = items.find((i) => i.kind === 'agent')!;
    expect(agent.id).toBe('agent-x1');
    expect(agent.payload).toBe('x1');
    expect(agent.icon).toBe('🎸');
  });

  it('view items have correct id and payload', () => {
    const items = buildPaletteItems([]);
    const chat = items.find((i) => i.label === 'Chat')!;
    expect(chat.id).toBe('view-chat');
    expect(chat.payload).toBe('chat');
    expect(chat.kind).toBe('view');
  });
});

describe('filterPaletteItems', () => {
  const agents: AgentInfo[] = [
    { id: 'a1', name: 'Alice', avatar: '🧑' },
    { id: 'a2', name: 'Research Bot', avatar: '🔬' },
  ];
  const items = buildPaletteItems(agents);

  it('returns all items for empty query', () => {
    expect(filterPaletteItems(items, '')).toEqual(items);
    expect(filterPaletteItems(items, '   ')).toEqual(items);
  });

  it('filters by label substring (case-insensitive)', () => {
    const result = filterPaletteItems(items, 'alice');
    expect(result).toHaveLength(1);
    expect(result[0].label).toBe('Alice');
  });

  it('matches partial label', () => {
    const result = filterPaletteItems(items, 'set');
    expect(result.some((i) => i.label === 'Settings')).toBe(true);
  });

  it('matches by kind', () => {
    const result = filterPaletteItems(items, 'agent');
    // All agent items + any view items with "agent" in description
    expect(result.length).toBeGreaterThanOrEqual(2);
  });

  it('returns empty array for no match', () => {
    const result = filterPaletteItems(items, 'zzzzzzz_no_match');
    expect(result).toHaveLength(0);
  });
});

describe('clampIndex', () => {
  it('returns -1 for empty list', () => {
    expect(clampIndex(0, 0)).toBe(-1);
    expect(clampIndex(5, 0)).toBe(-1);
  });

  it('clamps negative to 0', () => {
    expect(clampIndex(-1, 5)).toBe(0);
    expect(clampIndex(-100, 3)).toBe(0);
  });

  it('clamps above max to length - 1', () => {
    expect(clampIndex(10, 5)).toBe(4);
    expect(clampIndex(5, 5)).toBe(4);
  });

  it('passes through valid index', () => {
    expect(clampIndex(0, 5)).toBe(0);
    expect(clampIndex(2, 5)).toBe(2);
    expect(clampIndex(4, 5)).toBe(4);
  });
});
