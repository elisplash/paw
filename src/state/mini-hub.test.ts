// src/state/mini-hub.test.ts
// Unit tests for mini-hub state layer (Phase 1).

import { describe, it, expect, beforeEach, vi } from 'vitest';
import {
  createHub,
  createRegistry,
  addHub,
  removeHub,
  getHubBySession,
  getHubByAgent,
  getHub,
  getAllHubs,
  getOldestHub,
  cascadePosition,
  persistHubs,
  loadPersistedHubs,
  clearPersistedHubs,
  restoreHubs,
  type MiniHubRegistry,
} from './mini-hub';

// ── createHub ────────────────────────────────────────────────────────────

describe('createHub', () => {
  it('creates a hub with the given agentId', () => {
    const hub = createHub('aria');
    expect(hub.agentId).toBe('aria');
    expect(hub.id).toMatch(/^hub-aria-/);
    expect(hub.sessionKey).toBeNull();
    expect(hub.messages).toEqual([]);
    expect(hub.pendingAttachments).toEqual([]);
    expect(hub.streamState).toBeNull();
    expect(hub.modelOverride).toBeNull();
    expect(hub.isMinimized).toBe(false);
    expect(hub.unreadCount).toBe(0);
    expect(hub.position).toHaveProperty('x');
    expect(hub.position).toHaveProperty('y');
    expect(hub.createdAt).toBeGreaterThan(0);
  });

  it('accepts a custom id', () => {
    const hub = createHub('aria', { id: 'my-hub' });
    expect(hub.id).toBe('my-hub');
  });

  it('accepts a session key', () => {
    const hub = createHub('aria', { sessionKey: 'sess_123' });
    expect(hub.sessionKey).toBe('sess_123');
  });

  it('accepts a custom position', () => {
    const hub = createHub('aria', { position: { x: 100, y: 200 } });
    expect(hub.position).toEqual({ x: 100, y: 200 });
  });

  it('generates unique ids for different hubs', () => {
    const a = createHub('aria');
    const b = createHub('aria');
    expect(a.id).not.toBe(b.id);
  });
});

// ── createRegistry ───────────────────────────────────────────────────────

describe('createRegistry', () => {
  it('creates an empty registry with default maxHubs', () => {
    const reg = createRegistry();
    expect(reg.hubs.size).toBe(0);
    expect(reg.activeHubId).toBeNull();
    expect(reg.maxHubs).toBe(8);
  });

  it('accepts a custom maxHubs', () => {
    const reg = createRegistry(4);
    expect(reg.maxHubs).toBe(4);
  });
});

// ── addHub ───────────────────────────────────────────────────────────────

describe('addHub', () => {
  let reg: MiniHubRegistry;

  beforeEach(() => {
    reg = createRegistry(3);
  });

  it('adds a hub and returns true', () => {
    const hub = createHub('aria', { id: 'h1' });
    expect(addHub(reg, hub)).toBe(true);
    expect(reg.hubs.size).toBe(1);
    expect(reg.hubs.get('h1')).toBe(hub);
  });

  it('rejects when maxHubs is reached', () => {
    addHub(reg, createHub('a', { id: 'h1' }));
    addHub(reg, createHub('b', { id: 'h2' }));
    addHub(reg, createHub('c', { id: 'h3' }));
    expect(addHub(reg, createHub('d', { id: 'h4' }))).toBe(false);
    expect(reg.hubs.size).toBe(3);
  });
});

// ── removeHub ────────────────────────────────────────────────────────────

describe('removeHub', () => {
  let reg: MiniHubRegistry;

  beforeEach(() => {
    reg = createRegistry();
    addHub(reg, createHub('aria', { id: 'h1' }));
  });

  it('removes an existing hub and returns true', () => {
    expect(removeHub(reg, 'h1')).toBe(true);
    expect(reg.hubs.size).toBe(0);
  });

  it('returns false for a non-existent hub', () => {
    expect(removeHub(reg, 'nope')).toBe(false);
  });

  it('clears activeHubId if the removed hub was active', () => {
    reg.activeHubId = 'h1';
    removeHub(reg, 'h1');
    expect(reg.activeHubId).toBeNull();
  });

  it('does not clear activeHubId if a different hub was active', () => {
    addHub(reg, createHub('bot', { id: 'h2' }));
    reg.activeHubId = 'h2';
    removeHub(reg, 'h1');
    expect(reg.activeHubId).toBe('h2');
  });
});

// ── getHubBySession ──────────────────────────────────────────────────────

describe('getHubBySession', () => {
  it('finds a hub by its session key', () => {
    const reg = createRegistry();
    const hub = createHub('aria', { id: 'h1', sessionKey: 'sess_abc' });
    addHub(reg, hub);
    expect(getHubBySession(reg, 'sess_abc')).toBe(hub);
  });

  it('returns undefined when no hub matches', () => {
    const reg = createRegistry();
    addHub(reg, createHub('aria', { id: 'h1', sessionKey: 'sess_abc' }));
    expect(getHubBySession(reg, 'sess_xyz')).toBeUndefined();
  });
});

// ── getHubByAgent ────────────────────────────────────────────────────────

describe('getHubByAgent', () => {
  it('finds a hub by agent id', () => {
    const reg = createRegistry();
    const hub = createHub('aria', { id: 'h1' });
    addHub(reg, hub);
    expect(getHubByAgent(reg, 'aria')).toBe(hub);
  });

  it('returns undefined when no hub matches', () => {
    const reg = createRegistry();
    expect(getHubByAgent(reg, 'aria')).toBeUndefined();
  });
});

// ── getHub ───────────────────────────────────────────────────────────────

describe('getHub', () => {
  it('returns hub by id', () => {
    const reg = createRegistry();
    const hub = createHub('aria', { id: 'h1' });
    addHub(reg, hub);
    expect(getHub(reg, 'h1')).toBe(hub);
  });

  it('returns undefined for unknown id', () => {
    const reg = createRegistry();
    expect(getHub(reg, 'nope')).toBeUndefined();
  });
});

// ── getAllHubs ────────────────────────────────────────────────────────────

describe('getAllHubs', () => {
  it('returns hubs sorted by creation time', () => {
    const reg = createRegistry();
    const h1 = createHub('a', { id: 'h1' });
    h1.createdAt = 100;
    const h2 = createHub('b', { id: 'h2' });
    h2.createdAt = 50;
    const h3 = createHub('c', { id: 'h3' });
    h3.createdAt = 200;
    addHub(reg, h1);
    addHub(reg, h2);
    addHub(reg, h3);
    const all = getAllHubs(reg);
    expect(all.map((h) => h.id)).toEqual(['h2', 'h1', 'h3']);
  });

  it('returns empty array for empty registry', () => {
    expect(getAllHubs(createRegistry())).toEqual([]);
  });
});

// ── getOldestHub ─────────────────────────────────────────────────────────

describe('getOldestHub', () => {
  it('returns the hub with the lowest createdAt', () => {
    const reg = createRegistry();
    const h1 = createHub('a', { id: 'h1' });
    h1.createdAt = 100;
    const h2 = createHub('b', { id: 'h2' });
    h2.createdAt = 50;
    addHub(reg, h1);
    addHub(reg, h2);
    expect(getOldestHub(reg)?.id).toBe('h2');
  });

  it('returns undefined for empty registry', () => {
    expect(getOldestHub(createRegistry())).toBeUndefined();
  });
});

// ── cascadePosition ──────────────────────────────────────────────────────

describe('cascadePosition', () => {
  it('offsets position by 32px per existing hub', () => {
    const reg = createRegistry();
    const pos0 = cascadePosition(reg);
    addHub(reg, createHub('a', { id: 'h1' }));
    const pos1 = cascadePosition(reg);
    expect(pos1.x).toBe(pos0.x - 32);
    expect(pos1.y).toBe(pos0.y - 32);
  });
});

// ── Persistence ──────────────────────────────────────────────────────────

describe('persistence', () => {
  const mockStorage = new Map<string, string>();

  beforeEach(() => {
    mockStorage.clear();
    vi.stubGlobal('localStorage', {
      getItem: (k: string) => mockStorage.get(k) ?? null,
      setItem: (k: string, v: string) => mockStorage.set(k, v),
      removeItem: (k: string) => mockStorage.delete(k),
    });
  });

  it('round-trips through persistHubs → loadPersistedHubs', () => {
    const reg = createRegistry();
    const hub = createHub('aria', { id: 'hub-1', sessionKey: 'sess_1' });
    hub.modelOverride = 'gpt-4o';
    hub.position = { x: 42, y: 99 };
    addHub(reg, hub);
    persistHubs(reg);

    const entries = loadPersistedHubs();
    expect(entries).toHaveLength(1);
    expect(entries[0].id).toBe('hub-1');
    expect(entries[0].agentId).toBe('aria');
    expect(entries[0].sessionKey).toBe('sess_1');
    expect(entries[0].modelOverride).toBe('gpt-4o');
    expect(entries[0].position).toEqual({ x: 42, y: 99 });
  });

  it('loadPersistedHubs returns [] when nothing stored', () => {
    expect(loadPersistedHubs()).toEqual([]);
  });

  it('loadPersistedHubs ignores malformed data', () => {
    mockStorage.set('paw_minihub_positions', 'not json');
    expect(loadPersistedHubs()).toEqual([]);
  });

  it('loadPersistedHubs filters entries missing required fields', () => {
    mockStorage.set(
      'paw_minihub_positions',
      JSON.stringify([{ id: 'ok', agentId: 'a' }, { notAnId: true }]),
    );
    const entries = loadPersistedHubs();
    expect(entries).toHaveLength(1);
    expect(entries[0].id).toBe('ok');
  });

  it('clearPersistedHubs removes the key', () => {
    mockStorage.set('paw_minihub_positions', '[]');
    clearPersistedHubs();
    expect(mockStorage.has('paw_minihub_positions')).toBe(false);
  });
});

// ── restoreHubs ──────────────────────────────────────────────────────────

describe('restoreHubs', () => {
  beforeEach(() => {
    vi.stubGlobal('localStorage', {
      getItem: () => null,
      setItem: () => {},
      removeItem: () => {},
    });
  });

  it('creates hubs from persisted entries', () => {
    const reg = createRegistry();
    const count = restoreHubs(reg, [
      {
        id: 'h1',
        agentId: 'aria',
        sessionKey: 'sess_1',
        modelOverride: null,
        position: { x: 0, y: 0 },
      },
      {
        id: 'h2',
        agentId: 'bot',
        sessionKey: 'sess_2',
        modelOverride: 'gpt-4o',
        position: { x: 100, y: 100 },
      },
    ]);
    expect(count).toBe(2);
    expect(reg.hubs.size).toBe(2);
    const h2 = reg.hubs.get('h2')!;
    expect(h2.agentId).toBe('bot');
    expect(h2.modelOverride).toBe('gpt-4o');
    expect(h2.isMinimized).toBe(true); // restored hubs start minimized
  });

  it('respects maxHubs limit', () => {
    const reg = createRegistry(1);
    const count = restoreHubs(reg, [
      { id: 'h1', agentId: 'a', sessionKey: null, modelOverride: null, position: { x: 0, y: 0 } },
      { id: 'h2', agentId: 'b', sessionKey: null, modelOverride: null, position: { x: 0, y: 0 } },
    ]);
    expect(count).toBe(1);
    expect(reg.hubs.size).toBe(1);
  });
});
