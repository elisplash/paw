// src/state/mini-hub.ts
// Multi-instance chat state for mini-hubs.
// Phase 1 of MINI_HUB_PLAN.md.

import type { MessageWithAttachments, StreamState } from './index';

// ── Types ────────────────────────────────────────────────────────────────

/**
 * State for a single mini-hub instance.
 * Each hub is scoped to one agent + one session.
 */
export interface MiniHubInstance {
  /** Unique hub identifier, e.g. 'hub-agent-aria' */
  id: string;
  /** Backend session key (null = not yet created) */
  sessionKey: string | null;
  /** Primary agent id */
  agentId: string;
  /** Messages currently displayed in this hub */
  messages: MessageWithAttachments[];
  /** Files waiting to be sent */
  pendingAttachments: File[];
  /** Active stream reference (null when idle) */
  streamState: StreamState | null;
  /** Per-hub model override; null = use agent default */
  modelOverride: string | null;
  /** Whether the hub is collapsed to just a dock avatar */
  isMinimized: boolean;
  /** Number of unread messages (incremented while minimized) */
  unreadCount: number;
  /** Floating window position */
  position: { x: number; y: number };
  /** When the hub was created (epoch ms) */
  createdAt: number;
  /** Squad id when operating in multi-agent mode. */
  squadId?: string;
  /** Squad member metadata for rendering. */
  squadMembers?: Array<{ id: string; name: string; avatar?: string; color: string }>;
}

/**
 * Top-level registry that tracks all active mini-hub instances.
 */
export interface MiniHubRegistry {
  /** Map of hubId → instance */
  hubs: Map<string, MiniHubInstance>;
  /** The currently focused hub (null = none) */
  activeHubId: string | null;
  /** Hard cap on simultaneous hubs (default 8) */
  maxHubs: number;
}

// ── Serialized shape for localStorage ────────────────────────────────────

interface PersistedHubEntry {
  id: string;
  agentId: string;
  sessionKey: string | null;
  modelOverride: string | null;
  position: { x: number; y: number };
}

// ── localStorage key ─────────────────────────────────────────────────────

const STORAGE_KEY = 'paw_minihub_positions';

// ── Atom-level pure functions (no side effects) ──────────────────────────

/**
 * Create a fresh MiniHubInstance with sensible defaults.
 * The hub id is derived from agentId to keep things predictable.
 * If a specific id is needed, pass it in opts.
 */
export function createHub(
  agentId: string,
  opts?: {
    sessionKey?: string;
    position?: { x: number; y: number };
    id?: string;
  },
): MiniHubInstance {
  const hubId =
    opts?.id ??
    `hub-${agentId}-${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;
  return {
    id: hubId,
    sessionKey: opts?.sessionKey ?? null,
    agentId,
    messages: [],
    pendingAttachments: [],
    streamState: null,
    modelOverride: null,
    isMinimized: false,
    unreadCount: 0,
    position: opts?.position ?? defaultPosition(),
    createdAt: Date.now(),
  };
}

/**
 * Default position for a new hub — stacked from bottom-right.
 * Each hub offsets by 32px so they cascade.
 */
function defaultPosition(): { x: number; y: number } {
  // Safe fallback when running outside browser (tests)
  const w = typeof window !== 'undefined' ? window.innerWidth : 1280;
  const h = typeof window !== 'undefined' ? window.innerHeight : 720;
  // Position bottom-right with padding, capped so it never goes off-screen
  const hubW = Math.min(360, Math.floor(w * 0.92));
  const hubH = Math.min(500, Math.floor(h * 0.85));
  return {
    x: Math.max(8, w - hubW - 12),
    y: Math.max(8, h - hubH - 12),
  };
}

/**
 * Create an empty registry.
 */
export function createRegistry(maxHubs = 8): MiniHubRegistry {
  return {
    hubs: new Map(),
    activeHubId: null,
    maxHubs,
  };
}

/**
 * Add a hub to the registry.
 * Returns false if maxHubs would be exceeded.
 */
export function addHub(registry: MiniHubRegistry, hub: MiniHubInstance): boolean {
  if (registry.hubs.size >= registry.maxHubs) return false;
  registry.hubs.set(hub.id, hub);
  return true;
}

/**
 * Remove a hub from the registry by id.
 * Returns true if the hub existed and was removed.
 */
export function removeHub(registry: MiniHubRegistry, hubId: string): boolean {
  const removed = registry.hubs.delete(hubId);
  if (registry.activeHubId === hubId) registry.activeHubId = null;
  return removed;
}

/**
 * Find a hub by its backend session key.
 */
export function getHubBySession(
  registry: MiniHubRegistry,
  sessionKey: string,
): MiniHubInstance | undefined {
  for (const hub of registry.hubs.values()) {
    if (hub.sessionKey === sessionKey) return hub;
  }
  return undefined;
}

/**
 * Find a hub by agent id.
 * Returns the first match (there could be multiple hubs per agent in theory).
 */
export function getHubByAgent(
  registry: MiniHubRegistry,
  agentId: string,
): MiniHubInstance | undefined {
  for (const hub of registry.hubs.values()) {
    if (hub.agentId === agentId) return hub;
  }
  return undefined;
}

/**
 * Get a hub by its id.
 */
export function getHub(registry: MiniHubRegistry, hubId: string): MiniHubInstance | undefined {
  return registry.hubs.get(hubId);
}

/**
 * Get all hub instances as an array, ordered by creation time (oldest first).
 */
export function getAllHubs(registry: MiniHubRegistry): MiniHubInstance[] {
  return [...registry.hubs.values()].sort((a, b) => a.createdAt - b.createdAt);
}

/**
 * Get the oldest hub (candidate for eviction when maxHubs is reached).
 */
export function getOldestHub(registry: MiniHubRegistry): MiniHubInstance | undefined {
  let oldest: MiniHubInstance | undefined;
  for (const hub of registry.hubs.values()) {
    if (!oldest || hub.createdAt < oldest.createdAt) oldest = hub;
  }
  return oldest;
}

/**
 * Cascade positions so hubs don't perfectly overlap.
 * Offsets each hub by 32px from the previous one.
 */
export function cascadePosition(registry: MiniHubRegistry): { x: number; y: number } {
  const base = defaultPosition();
  const count = registry.hubs.size;
  return {
    x: base.x - count * 32,
    y: base.y - count * 32,
  };
}

// ── Persistence ──────────────────────────────────────────────────────────

/**
 * Persist hub positions + open state to localStorage.
 * Only saves minimal data (positions, agent, session, model).
 * Messages are NOT persisted — they reload from the engine.
 */
export function persistHubs(registry: MiniHubRegistry): void {
  try {
    const entries: PersistedHubEntry[] = [];
    for (const hub of registry.hubs.values()) {
      entries.push({
        id: hub.id,
        agentId: hub.agentId,
        sessionKey: hub.sessionKey,
        modelOverride: hub.modelOverride,
        position: hub.position,
      });
    }
    localStorage.setItem(STORAGE_KEY, JSON.stringify(entries));
  } catch {
    // localStorage may be unavailable in some environments
  }
}

/**
 * Load persisted hub entries from localStorage.
 * Returns an array of entries that can be used to re-create hubs.
 * Does NOT create hubs — the caller decides what to do with them.
 */
export function loadPersistedHubs(): PersistedHubEntry[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    // Validate each entry has required fields
    return parsed.filter(
      (e: unknown): e is PersistedHubEntry =>
        typeof e === 'object' &&
        e !== null &&
        typeof (e as PersistedHubEntry).id === 'string' &&
        typeof (e as PersistedHubEntry).agentId === 'string',
    );
  } catch {
    return [];
  }
}

/**
 * Clear persisted hub state.
 */
export function clearPersistedHubs(): void {
  try {
    localStorage.removeItem(STORAGE_KEY);
  } catch {
    // noop
  }
}

/**
 * Restore hubs from persisted entries into a registry.
 * Returns the number of hubs restored.
 */
export function restoreHubs(registry: MiniHubRegistry, entries: PersistedHubEntry[]): number {
  let restored = 0;
  for (const entry of entries) {
    if (registry.hubs.size >= registry.maxHubs) break;
    const hub = createHub(entry.agentId, {
      id: entry.id,
      sessionKey: entry.sessionKey ?? undefined,
      position: entry.position,
    });
    hub.modelOverride = entry.modelOverride;
    hub.isMinimized = true; // Restored hubs start minimized
    registry.hubs.set(hub.id, hub);
    restored++;
  }
  return restored;
}
