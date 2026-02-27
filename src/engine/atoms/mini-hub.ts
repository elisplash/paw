// src/engine/atoms/mini-hub.ts
// Phase 3.1 — Mini-Hub type definitions (atom layer).
// Pure interfaces, zero side effects, zero imports from molecules/organisms.

import type { MessageWithAttachments } from '../../state/index';

// ── Configuration ────────────────────────────────────────────────────────

/**
 * Configuration object passed to the mini-hub factory.
 * Captures everything needed to build a hub (agent info, position, etc.).
 */
export interface MiniHubConfig {
  /** Registry hub id (links to MiniHubInstance) */
  hubId: string;
  /** Agent id (backend identifier) */
  agentId: string;
  /** Display name */
  agentName: string;
  /** Avatar string (sprite id or emoji) */
  agentAvatar?: string;
  /** Agent accent colour (CSS colour value) */
  agentColor?: string;
  /** Existing backend session key (null → create on first send) */
  sessionKey?: string;
  /** Model override for this hub; null = agent default */
  modelOverride?: string;
  /** Initial floating window position */
  position?: { x: number; y: number };
  /** Squad mode: multiple agents in one hub */
  squadId?: string;
  squadMembers?: Array<{
    id: string;
    name: string;
    avatar?: string;
    color: string;
  }>;
}

// ── Controller ───────────────────────────────────────────────────────────

/**
 * Public API returned by `createMiniHub()`.
 * The consumer (orchestrator) interacts with the hub exclusively through
 * this interface — never via raw DOM lookups.
 */
export interface MiniHubController {
  /** Root DOM element (attach to document.body or a container) */
  el: HTMLElement;
  /** Registry hub id */
  hubId: string;

  // ── Session ──────────────────────────────────────────────────────────

  /** Current backend session key (null if not yet created). */
  getSessionKey(): string | null;
  /** Assign a backend session key after the first send. */
  setSessionKey(key: string): void;

  // ── Message rendering ────────────────────────────────────────────────

  /** Append a finalised message to the feed. */
  appendMessage(msg: MessageWithAttachments): void;
  /** Replace the entire message list (e.g. after loading history). */
  setMessages(msgs: MessageWithAttachments[]): void;

  // ── Streaming ────────────────────────────────────────────────────────

  /** Insert a streaming placeholder. */
  startStreaming(agentName: string): void;
  /** Append a text delta to the streaming message. */
  appendDelta(text: string): void;
  /** Append a thinking/reasoning delta. */
  appendThinking(text: string): void;
  /** Finalize the stream (render final markdown). */
  finalizeStream(content: string): void;
  /** Toggle the streaming activity indicator (pulsing dot in titlebar). */
  setStreamingActive(active: boolean): void;
  /** Whether the hub is currently in streaming state. */
  isStreamingActive(): boolean;

  // ── Model ────────────────────────────────────────────────────────────

  /** Set the selected model key. */
  setModel(modelKey: string): void;
  /** Get the currently selected model key. */
  getModel(): string;

  // ── Window state ─────────────────────────────────────────────────────

  /** Collapse to dock avatar. */
  minimize(): void;
  /** Expand from dock avatar. */
  restore(): void;
  /** Whether the hub is currently minimized. */
  isMinimized(): boolean;

  // ── Unread ───────────────────────────────────────────────────────────

  /** Increment unread count (while minimized). */
  incrementUnread(): void;
  /** Reset unread count to 0. */
  clearUnread(): void;

  // ── Focus / position ─────────────────────────────────────────────────

  /** Bring to front and focus the input. */
  focus(): void;
  /** Get current position. */
  getPosition(): { x: number; y: number };

  // ── Lifecycle ────────────────────────────────────────────────────────

  /** Full teardown: remove DOM, listeners, unsubscribe event bus. */
  destroy(): void;
}

// ── Dock ─────────────────────────────────────────────────────────────────

/**
 * Public API for the floating agent dock tray.
 */
export interface AgentDockController {
  /** Refresh the avatar list from the provided agents array. */
  refresh(agents: AgentDockEntry[]): void;
  /** Mark a hub as active (ring around avatar). */
  addHub(hubId: string, agentId: string): void;
  /** Remove active ring from a hub's avatar. */
  removeHub(hubId: string, agentId: string): void;
  /** Set unread badge count on an agent's avatar. */
  setUnread(agentId: string, count: number): void;
  /** Toggle breathing animation on an agent's dock avatar during streaming. */
  setStreaming(agentId: string, active: boolean): void;
  /** Full teardown. */
  destroy(): void;
}

/** Minimal agent data the dock needs — avoids importing the full Agent type. */
export interface AgentDockEntry {
  id: string;
  name: string;
  avatar: string;
  color: string;
}

// ── Squad Colors ─────────────────────────────────────────────────────────

/**
 * Pre-defined per-agent colors for squad mode (max 8 members).
 * Assigned by member index in the squad definition.
 */
export const SQUAD_COLORS = [
  'var(--accent)',         // red — coordinator
  'var(--kinetic-sage)',   // sage green
  '#6BA5E7',              // blue
  '#E7A76B',              // amber
  '#B56BE7',              // purple
  '#E76B8A',              // pink
  '#6BE7C4',              // teal
  '#E7D76B',              // gold
] as const;

/**
 * Build an agentMap from squad members for use in RenderOpts.
 * Maps agentId → { name, avatar, color } with auto-assigned squad colors.
 */
export function buildSquadAgentMap(
  members: Array<{ id: string; name: string; avatar?: string; color?: string }>,
): Map<string, { name: string; avatar?: string; color?: string }> {
  const map = new Map<string, { name: string; avatar?: string; color?: string }>();
  for (let i = 0; i < members.length; i++) {
    const m = members[i];
    map.set(m.id, {
      name: m.name,
      avatar: m.avatar,
      color: m.color || SQUAD_COLORS[i % SQUAD_COLORS.length],
    });
  }
  return map;
}
