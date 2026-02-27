// src/engine/organisms/mini_hub_orchestrator.ts
// Phase 3.4 — Mini-Hub Orchestrator (organism layer).
// Lifecycle coordinator: creates/destroys mini-hubs, wires event bus
// subscribers, manages the agent dock, and handles persistence.

import type { MiniHubController, AgentDockController } from '../atoms/mini-hub';
import { SQUAD_COLORS } from '../atoms/mini-hub';
import type { Agent } from '../../types';

import {
  appState,
  agentSessionMap,
  persistAgentSessionMap,
  type MessageWithAttachments,
} from '../../state/index';
import {
  createHub,
  addHub,
  removeHub,
  getHub,
  getHubByAgent,
  getHubBySession,
  getOldestHub,
  cascadePosition,
  persistHubs,
  loadPersistedHubs,
  restoreHubs,
  getAllHubs,
} from '../../state/mini-hub';
import { createMiniHub } from '../molecules/mini-hub';
import { createAgentDock } from '../molecules/agent_dock';
import { subscribeSession, type StreamHandlers } from '../molecules/event_bus';
import { engineChatSend } from '../molecules/bridge';
import { pawEngine } from '../molecules/ipc_client';
import { extractContent, fileToBase64 } from '../atoms/chat';
import { showToast } from '../../components/toast';

// ── Module state ─────────────────────────────────────────────────────────

/** Map hubId → { controller, unsubscribe } */
const _liveHubs = new Map<
  string,
  {
    ctrl: MiniHubController;
    unsubscribe: (() => void) | null;
  }
>();

let _dock: AgentDockController | null = null;
let _getAgents: (() => Agent[]) | null = null;
let _initialized = false;

// ── Public API ───────────────────────────────────────────────────────────

/**
 * One-time initialization. Call after app boot and agents have loaded.
 * Sets up the dock and optionally restores previously-open hubs.
 *
 * @param getAgents  Function that returns the current agents array.
 * @param dockContainer  Element to mount the dock into (default: document.body).
 */
export function initMiniHubSystem(
  getAgents: () => Agent[],
  dockContainer: HTMLElement = document.body,
): void {
  if (_initialized) return;
  _initialized = true;
  _getAgents = getAgents;

  // Create dock
  _dock = createAgentDock(dockContainer, (agentId) => {
    openMiniHub(agentId);
  });

  // Initial dock refresh
  refreshDock();

  // Restore persisted hubs
  const entries = loadPersistedHubs();
  if (entries.length > 0) {
    const count = restoreHubs(appState.miniHubs, entries);
    if (count > 0) {
      // Spin up controllers for each restored hub
      for (const hub of getAllHubs(appState.miniHubs)) {
        _spawnController(hub.id, hub.agentId, hub.sessionKey ?? undefined, hub.position);
        const live = _liveHubs.get(hub.id);
        if (live) {
          live.ctrl.minimize(); // Restored hubs start minimized
          _dock?.addHub(hub.id, hub.agentId);
        }
      }
      console.debug(`[mini-hub] Restored ${count} hubs from localStorage`);
    }
  }
}

/**
 * Refresh the dock's agent list (call when agents are added/removed).
 */
export function refreshDock(): void {
  if (!_dock || !_getAgents) return;
  const agents = _getAgents();
  _dock.refresh(
    agents.map((a) => ({
      id: a.id,
      name: a.name,
      avatar: a.avatar,
      color: a.color,
    })),
  );
}

/**
 * Open (or restore) a mini-hub for the given agent.
 * If a hub for this agent already exists and is minimized, restore it.
 * If it doesn't exist, create a new one.
 */
export function openMiniHub(agentId: string, sessionKey?: string): void {
  // Already open? → restore
  const existing = getHubByAgent(appState.miniHubs, agentId);
  if (existing) {
    const live = _liveHubs.get(existing.id);
    if (live) {
      if (live.ctrl.isMinimized()) live.ctrl.restore();
      live.ctrl.focus();
      return;
    }
  }

  // Check maxHubs cap
  if (appState.miniHubs.hubs.size >= appState.miniHubs.maxHubs) {
    // Close oldest
    const oldest = getOldestHub(appState.miniHubs);
    if (oldest) {
      closeMiniHub(oldest.id);
    }
  }

  // Resolve session key from agentSessionMap if not provided
  const resolvedSessionKey = sessionKey ?? agentSessionMap.get(agentId) ?? undefined;

  // Create state entry
  const pos = cascadePosition(appState.miniHubs);
  const hub = createHub(agentId, {
    sessionKey: resolvedSessionKey,
    position: pos,
  });
  if (!addHub(appState.miniHubs, hub)) {
    showToast('Maximum mini-hubs reached', 'info');
    return;
  }

  _spawnController(hub.id, agentId, resolvedSessionKey, pos);
  _dock?.addHub(hub.id, agentId);
  persistHubs(appState.miniHubs);

  // Load history if session exists
  if (resolvedSessionKey) {
    _loadHistory(hub.id, resolvedSessionKey);
  }
}

/**
 * Close a mini-hub by hub id. Unsubscribes event bus, removes DOM,
 * removes from registry. Does NOT end the backend session.
 */
export function closeMiniHub(hubId: string): void {
  const live = _liveHubs.get(hubId);
  if (live) {
    live.unsubscribe?.();
    live.ctrl.destroy();
    _liveHubs.delete(hubId);
  }

  const hub = getHub(appState.miniHubs, hubId);
  if (hub) {
    _dock?.removeHub(hubId, hub.agentId);
    _dock?.setUnread(hub.agentId, 0);
    removeHub(appState.miniHubs, hubId);
  }

  persistHubs(appState.miniHubs);
}

/**
 * Close all mini-hubs.
 */
export function closeAllMiniHubs(): void {
  const ids = [..._liveHubs.keys()];
  for (const id of ids) {
    closeMiniHub(id);
  }
}

/**
 * Check if a mini-hub is open for the given agent.
 */
export function isMiniHubOpen(agentId: string): boolean {
  return !!getHubByAgent(appState.miniHubs, agentId);
}

/**
 * Get unread count for an agent's mini-hub.
 */
export function getMiniHubUnread(agentId: string): number {
  const hub = getHubByAgent(appState.miniHubs, agentId);
  return hub?.unreadCount ?? 0;
}

/**
 * Open a squad (multi-agent) mini-hub.
 * Loads the squad from the engine, resolves member agents, and creates a
 * hub with all member info so the renderer can color-code messages.
 *
 * The coordinator agent is used as the "primary" agent for the hub.
 */
export async function openSquadHub(squadId: string): Promise<void> {
  if (!_getAgents) return;

  try {
    const squads = await pawEngine.squadsList();
    const squad = squads.find((s) => s.id === squadId);
    if (!squad) {
      showToast(`Squad "${squadId}" not found`, 'error');
      return;
    }

    // Resolve members from agents list
    const agents = _getAgents();
    const members: Array<{ id: string; name: string; avatar?: string; color: string }> = [];
    for (let i = 0; i < squad.members.length; i++) {
      const member = squad.members[i];
      const agent = agents.find((a) => a.id === member.agent_id);
      members.push({
        id: member.agent_id,
        name: agent?.name ?? member.agent_id,
        avatar: agent?.avatar,
        color: agent?.color || (SQUAD_COLORS[i % SQUAD_COLORS.length] as string),
      });
    }

    if (members.length === 0) {
      showToast('Squad has no members', 'error');
      return;
    }

    // Use the coordinator (first member) as primary agent
    const coordinator = members[0];

    // Check if already open for this agent
    const existing = getHubByAgent(appState.miniHubs, coordinator.id);
    if (existing) {
      const live = _liveHubs.get(existing.id);
      if (live) {
        if (live.ctrl.isMinimized()) live.ctrl.restore();
        live.ctrl.focus();
        return;
      }
    }

    // Check maxHubs cap
    if (appState.miniHubs.hubs.size >= appState.miniHubs.maxHubs) {
      const oldest = getOldestHub(appState.miniHubs);
      if (oldest) closeMiniHub(oldest.id);
    }

    // Create state entry with squad metadata
    const pos = cascadePosition(appState.miniHubs);
    const hub = createHub(coordinator.id, { position: pos });
    hub.squadId = squadId;
    hub.squadMembers = members;

    if (!addHub(appState.miniHubs, hub)) {
      showToast('Maximum mini-hubs reached', 'info');
      return;
    }

    // Spawn controller with squad config
    _spawnController(hub.id, coordinator.id, undefined, pos, {
      squadId,
      squadMembers: members,
      squadName: squad.name,
    });
    _dock?.addHub(hub.id, coordinator.id);
    persistHubs(appState.miniHubs);
  } catch (e) {
    console.error('[mini-hub] Failed to open squad hub:', e);
    showToast('Failed to open squad chat', 'error');
  }
}

// ── Internal: spawn a MiniHubController ──────────────────────────────────

function _spawnController(
  hubId: string,
  agentId: string,
  sessionKey?: string,
  position?: { x: number; y: number },
  squadOpts?: {
    squadId: string;
    squadMembers: Array<{ id: string; name: string; avatar?: string; color: string }>;
    squadName?: string;
  },
) {
  const agent = _getAgents?.().find((a) => a.id === agentId);
  if (!agent) {
    console.warn(`[mini-hub] Agent "${agentId}" not found, skipping hub creation`);
    return;
  }

  const ctrl = createMiniHub(
    {
      hubId,
      agentId,
      agentName: squadOpts?.squadName ?? agent.name,
      agentAvatar: agent.avatar,
      agentColor: agent.color,
      sessionKey,
      modelOverride: getHub(appState.miniHubs, hubId)?.modelOverride ?? undefined,
      position,
      squadId: squadOpts?.squadId,
      squadMembers: squadOpts?.squadMembers,
    },
    {
      onSend: _handleSend,
      onClose: closeMiniHub,
      onMaximize: _handleMaximize,
      onPositionChange: _handlePositionChange,
      onModelChange: _handleModelChange,
    },
  );

  document.body.appendChild(ctrl.el);

  let unsubscribe: (() => void) | null = null;
  if (sessionKey) {
    unsubscribe = _subscribeHub(hubId, sessionKey, ctrl);
  }

  _liveHubs.set(hubId, { ctrl, unsubscribe });
}

// ── Internal: event bus subscription ─────────────────────────────────────

function _subscribeHub(
  _hubId: string,
  sessionKey: string,
  ctrl: MiniHubController,
): () => void {
  // Resolve the agent id for dock streaming indicators
  const hub = getHub(appState.miniHubs, _hubId);
  const agentId = hub?.agentId;

  // Build squad member lookup for multi-agent sessions
  const squadMemberMap = hub?.squadMembers
    ? new Map(hub.squadMembers.map((m) => [m.id, m]))
    : undefined;
  // Track which agent is currently streaming (for squad mode)
  let _currentStreamingAgentId: string | undefined;

  const handlers: StreamHandlers = {
    onDelta(text: string) {
      ctrl.appendDelta(text);
    },
    onThinking(text: string) {
      ctrl.appendThinking(text);
    },
    onToken(_usage) {
      // Token tracking for mini-hubs is handled at registry level — skip for now
    },
    onModel(model: string) {
      ctrl.setModel(model);
    },
    onStreamEnd(content: string) {
      onHubStreamComplete(sessionKey, content);
      ctrl.setStreamingActive(false);
      if (agentId) _dock?.setStreaming(agentId, false);
      _currentStreamingAgentId = undefined;
    },
    onStreamError(error: string) {
      onHubStreamError(sessionKey, error);
      ctrl.setStreamingActive(false);
      if (agentId) _dock?.setStreaming(agentId, false);
      _currentStreamingAgentId = undefined;
    },
    onToolStart(toolName: string) {
      const memberName = _currentStreamingAgentId && squadMemberMap?.get(_currentStreamingAgentId)?.name;
      console.debug(`[mini-hub] Tool started: ${memberName ? `${memberName} → ` : ''}${toolName}`);
    },
    onToolEnd(_toolName: string) {
      // Tool completion
    },

    // Squad-specific: agent-identified events
    onAgentDelta(evtAgentId: string, _text: string) {
      // If a different agent starts producing deltas in a squad session,
      // finalize the previous streaming block and start a new one
      if (squadMemberMap && evtAgentId !== _currentStreamingAgentId) {
        if (_currentStreamingAgentId) {
          // A different agent is now speaking — finalize previous stream
          // The content will have already been appended via onDelta
        }
        _currentStreamingAgentId = evtAgentId;
      }
    },
    onAgentStart(evtAgentId: string) {
      // In squad mode, show which member started working
      if (squadMemberMap) {
        const member = squadMemberMap.get(evtAgentId);
        if (member) {
          _currentStreamingAgentId = evtAgentId;
          ctrl.startStreaming(member.name);
          ctrl.setStreamingActive(true);
          if (agentId) _dock?.setStreaming(agentId, true);
        }
      }
    },
  };

  return subscribeSession(sessionKey, handlers);
}

// ── Internal: send message ───────────────────────────────────────────────

async function _handleSend(hubId: string, content: string, attachments: File[]) {
  const hub = getHub(appState.miniHubs, hubId);
  const live = _liveHubs.get(hubId);
  if (!hub || !live) return;

  const agent = _getAgents?.().find((a) => a.id === hub.agentId);
  if (!agent) return;

  // Add user message to feed
  const userMsg: MessageWithAttachments = {
    role: 'user',
    content,
    timestamp: new Date(),
  };
  live.ctrl.appendMessage(userMsg);
  hub.messages.push(userMsg);

  // Start streaming placeholder
  live.ctrl.startStreaming(agent.name);
  _dock?.setStreaming(hub.agentId, true);

  // Build session key — create one if needed
  let sk = hub.sessionKey;
  if (!sk) {
    sk = `eng-hub-${hub.agentId}-${Date.now().toString(36)}`;
    hub.sessionKey = sk;
    live.ctrl.setSessionKey(sk);
    agentSessionMap.set(hub.agentId, sk);
    persistAgentSessionMap();

    // Subscribe to events for this new session
    if (live.unsubscribe) live.unsubscribe();
    live.unsubscribe = _subscribeHub(hubId, sk, live.ctrl);
  }

  try {
    // Convert attachments
    const convertedAttachments = await Promise.all(
      attachments.map(async (file) => ({
        mimeType: file.type || 'application/octet-stream',
        content: await fileToBase64(file),
        name: file.name,
      })),
    );

    const model = live.ctrl.getModel() || agent.model;

    const result = await engineChatSend(sk, content, {
      model: model !== 'default' ? model : undefined,
      agentProfile: {
        id: agent.id,
        name: agent.name,
        bio: agent.bio,
        systemPrompt: agent.systemPrompt,
        model: agent.model,
        personality: agent.personality,
        boundaries: agent.boundaries,
        autoApproveAll: agent.autoApproveAll,
      },
      attachments: convertedAttachments.length > 0 ? convertedAttachments : undefined,
    });

    // The event bus will handle streaming deltas via the subscription.
    // Store the session key from the result if different.
    if (result.sessionKey && result.sessionKey !== sk) {
      hub.sessionKey = result.sessionKey;
      live.ctrl.setSessionKey(result.sessionKey);

      // Re-subscribe with new session key
      if (live.unsubscribe) live.unsubscribe();
      live.unsubscribe = _subscribeHub(hubId, result.sessionKey, live.ctrl);
    }

    persistHubs(appState.miniHubs);
  } catch (e) {
    console.error('[mini-hub] Send error:', e);
    live.ctrl.finalizeStream(
      `Error: ${e instanceof Error ? e.message : 'Failed to send message'}`,
    );
    _dock?.setStreaming(hub.agentId, false);
    showToast(
      `Mini-hub error: ${e instanceof Error ? e.message : 'Failed to send'}`,
      'error',
    );
  }
}

// ── Internal: maximize (switch to main chat) ─────────────────────────────

function _handleMaximize(hubId: string) {
  const hub = getHub(appState.miniHubs, hubId);
  if (!hub) return;

  // Set the main chat to this agent + session
  if (hub.sessionKey) {
    appState.currentSessionKey = hub.sessionKey;
  }

  // Close the mini-hub (the main chat controller will pick up the session)
  closeMiniHub(hubId);

  // Navigate to chat view if not already there
  // The orchestrator shouldn't depend on router — emit a custom event instead
  window.dispatchEvent(
    new CustomEvent('paw:navigate', {
      detail: { view: 'chat', agentId: hub.agentId },
    }),
  );
}

// ── Internal: position change ────────────────────────────────────────────

function _handlePositionChange(hubId: string, pos: { x: number; y: number }) {
  const hub = getHub(appState.miniHubs, hubId);
  if (hub) {
    hub.position = pos;
    persistHubs(appState.miniHubs);
  }
}

// ── Internal: model change ───────────────────────────────────────────────

function _handleModelChange(hubId: string, model: string) {
  const hub = getHub(appState.miniHubs, hubId);
  if (hub) {
    hub.modelOverride = model || null;
    persistHubs(appState.miniHubs);
  }
}

// ── Internal: load chat history into a hub ───────────────────────────────

async function _loadHistory(hubId: string, sessionKey: string) {
  const live = _liveHubs.get(hubId);
  const hub = getHub(appState.miniHubs, hubId);
  if (!live || !hub) return;

  try {
    const stored = await pawEngine.chatHistory(sessionKey, 200);
    if (stored.length > 0) {
      const msgs: MessageWithAttachments[] = stored
        .filter((m) => m.role === 'user' || m.role === 'assistant')
        .map((m) => ({
          role: m.role as 'user' | 'assistant',
          content: extractContent(m.content),
          timestamp: new Date(m.created_at),
          toolCalls: m.tool_calls_json ? JSON.parse(m.tool_calls_json) : undefined,
          agentId: m.agent_id,
        }));

      hub.messages = msgs;
      live.ctrl.setMessages(msgs);
    }

    // Phase 4.3: If an active stream exists for this session, re-attach streaming UI.
    // This handles the case where a hub is opened for an agent that is currently
    // running in the background (e.g. restored from persistence while agent is working).
    const activeStream = appState.activeStreams.get(sessionKey);
    if (activeStream) {
      const agent = _getAgents?.().find((a) => a.id === hub.agentId);
      live.ctrl.startStreaming(agent?.name ?? hub.agentId);
      _dock?.setStreaming(hub.agentId, true);

      // If content has already been accumulated, feed it into the hub
      if (activeStream.content) {
        live.ctrl.appendDelta(activeStream.content);
      }
    }
  } catch (e) {
    console.warn(`[mini-hub] Failed to load history for ${sessionKey}:`, e);
  }
}

// ── Lifecycle event handlers ─────────────────────────────────────────────

/**
 * Called when a stream completes for a mini-hub session.
 * Updates unread counts for minimized hubs and sends notifications.
 */
export function onHubStreamComplete(sessionKey: string, finalContent: string) {
  const hub = getHubBySession(appState.miniHubs, sessionKey);
  if (!hub) return;

  const live = _liveHubs.get(hub.id);
  if (!live) return;

  live.ctrl.finalizeStream(finalContent);

  if (live.ctrl.isMinimized()) {
    hub.unreadCount++;
    live.ctrl.incrementUnread();
    _dock?.setUnread(hub.agentId, hub.unreadCount);

    const agent = _getAgents?.().find((a) => a.id === hub.agentId);
    const agentName = agent?.name ?? hub.agentId;
    showToast(`${agentName} finished a task`, 'info');

    // Desktop notification (if permitted and document is not focused)
    _desktopNotify(`${agentName} finished a task`, finalContent);
  }
}

/**
 * Called when a stream errors for a mini-hub session.
 */
export function onHubStreamError(sessionKey: string, errorMsg: string) {
  const hub = getHubBySession(appState.miniHubs, sessionKey);
  if (!hub) return;

  const live = _liveHubs.get(hub.id);
  if (!live) return;

  live.ctrl.finalizeStream(`Error: ${errorMsg}`);

  if (live.ctrl.isMinimized()) {
    hub.unreadCount++;
    live.ctrl.incrementUnread();
    _dock?.setUnread(hub.agentId, hub.unreadCount);

    const agent = _getAgents?.().find((a) => a.id === hub.agentId);
    const agentName = agent?.name ?? hub.agentId;
    showToast(`${agentName} encountered an error`, 'error');
  }
}

// ── Internal: Desktop notification ───────────────────────────────────────

/**
 * Show a desktop notification if the Notification API is available,
 * permission is granted, and the window is not focused.
 */
function _desktopNotify(title: string, body: string) {
  if (typeof Notification === 'undefined') return;
  if (document.hasFocus()) return;
  if (Notification.permission !== 'granted') {
    // Request permission for future notifications (non-blocking)
    if (Notification.permission !== 'denied') {
      Notification.requestPermission().catch(() => {});
    }
    return;
  }

  try {
    const n = new Notification(title, {
      body: body.slice(0, 200),
      icon: '/favicon.ico',
      tag: `paw-minihub-${Date.now()}`,
      silent: false,
    });
    // Auto-close after 8 seconds
    setTimeout(() => n.close(), 8000);
  } catch {
    // Desktop notifications may not be supported in all environments
  }
}
